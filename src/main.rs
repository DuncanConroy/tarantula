use std::env;

use async_recursion::async_recursion;
use chrono::{Utc};
use hyper::{Body, Client, Uri, Request};
use hyper_tls::HttpsConnector;

use linkresult::{Link, UriResult, ResponseTimings};

// A simple type alias so as to DRY.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();

    // Some simple CLI args requirements...
    let url = match env::args().nth(1) {
        Some(url) => url,
        None => {
            println!("Usage: client <url>");
            return Ok(());
        }
    };

    let start_time = Utc::now();
    let url = url.parse::<hyper::Uri>().unwrap();

    let mut body = fetch_url(&url).await?;
    let response_timings = ResponseTimings {
        request_start_time: start_time,
        parse_complete_time: None,
        request_complete_time: Some(Utc::now()),
    };
    println!("HOST:{}", &url.host().unwrap());
    let protocol = format!("{}://", &url.scheme().unwrap());
    let uri_result: UriResult = dom_parser::get_links(
        protocol.as_str(),
        None,
        &url.host().unwrap(),
        &mut body,
        true,
        response_timings,
    ).unwrap();
    println!("links: {:?}", uri_result);

    //TODO: recursive function, multi-threaded, return link object with metadata, response timings

    let known_links = vec![Link::from_str("/")];
    let parent_uri = Some(Link::from_str(url.host().unwrap()));
    let total_links = recursive_load_page_and_get_links(
        LoadPageArguments {
            parent_protocol: protocol,
            parent_uri,
            host: url.host().unwrap().to_string(),
            links: uri_result.links,
            known_links,
        }
    ).await?;

    println!("total_links: {:?}", total_links);

    Ok(())
}

struct LoadPageArguments {
    parent_protocol: String,
    parent_uri: Option<Link>,
    host: String,
    links: Vec<Link>,
    known_links: Vec<Link>,
}

unsafe impl Send for LoadPageArguments {}

#[async_recursion]
async fn recursive_load_page_and_get_links(load_page_arguments: LoadPageArguments) -> Result<Vec<Link>> {
    let mut all_known_links: Vec<Link> = vec![];
    all_known_links.append(&mut load_page_arguments.known_links.clone());

    for link in load_page_arguments.links {
        let item_url_string = create_url_string(&load_page_arguments.parent_protocol, &load_page_arguments.host, &link.uri);
        println!("item_url_string {}", item_url_string);
        let item_url = item_url_string.parse::<hyper::Uri>().unwrap();
        println!("trying {}", item_url);
        let mut links_to_visit = find_links_to_visit(&load_page_arguments.parent_protocol,
                                                     load_page_arguments.parent_uri.clone(),
                                                     all_known_links.clone(),
                                                     item_url).await?;

        println!("found {} links to visit: {:?}", links_to_visit.len(), links_to_visit);

        all_known_links.append(&mut links_to_visit);
        recursive_load_page_and_get_links(
            LoadPageArguments {
                parent_protocol: load_page_arguments.parent_protocol.clone(),
                parent_uri: load_page_arguments.parent_uri.clone(),
                host: load_page_arguments.host.clone(),
                links: links_to_visit.clone(),
                known_links: all_known_links.clone(),
            }
        ).await?;
    }

    Ok(all_known_links)
}

async fn find_links_to_visit(parent_protocol: &str, parent_uri: Option<Link>, all_known_links: Vec<Link>, item_url: Uri) -> Result<Vec<Link>> {
    let request_start_time = Utc::now();
    let mut item_body = fetch_url(&item_url).await?;
    let response_timings = ResponseTimings {
        request_start_time,
        parse_complete_time: None,
        request_complete_time: Some(Utc::now()),
    };
    if item_body.is_empty() {
        println!("No body found, now HTML to parse -> skipping");
        return Ok(Vec::<Link>::new());
    }

    let uri_result: UriResult = dom_parser::get_links(
        &parent_protocol,
        parent_uri,
        &item_url.host().unwrap(),
        &mut item_body,
        true,
        response_timings,
    ).unwrap();

    let links_to_visit: Vec<Link> = uri_result.links.iter()
        .filter(|it| !all_known_links.contains(&it))
        .map(|it| it)
        .cloned()
        .collect();
    Ok(links_to_visit)
}

fn create_url_string(protocol: &str, host: &str, link: &String) -> String {
    println!("#-> {},{},{}", host, protocol, link);
    if link.starts_with("http") {
        link.to_owned()
    } else {
        format!("{}{}{}", protocol, host, link)
    }
}

async fn fetch_url(url: &hyper::Uri) -> Result<String> {
    println!("URI: {}", url);

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    let req = Request::builder()
        .method("HEAD")
        .uri(url)
        .body(Body::from(""))
        .expect("HEAD request builder");

    let head = client.request(req).await?;
    if !head.status().is_success() {
        return Ok(String::from(""));
        // todo: should be in metadata/response
    }
    let content_type =head.headers().get("content-type");
    if content_type.is_none() {
        return Err(format!("No content-type header found! {:?}", head).into());
    }
    if !content_type.unwrap().to_str().unwrap().to_string().contains("text/html") {
        return Ok(String::from(""));
    }

    let response = client.get(url.clone()).await?;

    // println!("Status: {}", response.status());
    // println!("Headers: {:#?}\n", response.headers());

    let body: String = String::from_utf8_lossy(hyper::body::to_bytes(response.into_body()).await?.as_ref()).to_string();
    // println!("BODY: {}", body);

    // println!("\nDone!");

    Ok(body)
}