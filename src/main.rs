use std::env;

use futures::{
    // futures_unordered::FuturesUnordered,
    FutureExt,
    stream::{Stream, StreamExt},
};
use chrono::{DateTime, Utc};
use hyper::{body::HttpBody, Client, Uri};
use hyper_tls::HttpsConnector;
use tokio::io::{self, AsyncWriteExt};

use dom_parser::{
    get_links,
};
use linkresult::{
    Link,
    UriResult,
};
use std::time::Instant;
use futures::future::BoxFuture;
use std::error::Error;

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
    println!("HOST:{}", &url.host().unwrap());
    let protocol = format!("{}://", &url.scheme().unwrap());
    let mut uri_result: UriResult = dom_parser::get_links(
        protocol.as_str(),
        &None,
        &url.host().unwrap(),
        &mut body,
        true,
        start_time,
    ).unwrap();
    println!("links: {:?}", uri_result);

    //TODO: recursive function, multi-threaded, return link object with metadata, response timings

    let mut known_links = vec![Link::from_str("/")];
    let parent_uri = Some(Link::from_str(url.host().unwrap()));
    let total_links = recursive_load_page_and_get_links(
        &protocol,
        &parent_uri,
        url.host().unwrap(),
        &uri_result.links,
        &mut known_links,
    ).await?;

    println!("total_links: {:?}", total_links);

    Ok(())
}

async fn recursive_load_page_and_get_links(
// async fn recursive_load_page_and_get_links(
parent_protocol: &str,
parent_uri: &Option<Link>,
host: &str,
links: &Vec<Link>,
known_links: &mut Vec<Link>,
) -> Result<Vec<Link>> {
// ) -> BoxFuture<'static, ()> {
//     async move {
    let mut all_known_links: Vec<Link> = vec![];
    all_known_links.append(known_links);

    for link in links {
        let item_url_string = create_url_string(&parent_protocol, &host, &link.uri);
        println!("item_url_string {}", item_url_string);
        let item_url = item_url_string.parse::<hyper::Uri>().unwrap();
        println!("trying {}", item_url);
        let mut links_to_visit = find_links_to_visit(&parent_protocol, parent_uri, &mut all_known_links, &item_url).await?;

        println!("found {} links to visit: {:?}", links_to_visit.len(), links_to_visit);

        recursive_load_page_and_get_links(
            parent_protocol,
            &parent_uri,
            host,
            &links_to_visit,
            &mut all_known_links,
        );
        all_known_links.append(&mut links_to_visit);
    }

    Ok(all_known_links)
    // }.boxed()
}

async fn find_links_to_visit(parent_protocol: &&str, parent_uri: &Option<Link>, all_known_links: &mut Vec<Link>, item_url: &Uri) -> Result<Vec<Link>> {
    let mut item_body = fetch_url(&item_url).await?;
    let uri_result: UriResult = dom_parser::get_links(
        &parent_protocol,
        parent_uri,
        &item_url.host().unwrap(),
        &mut item_body,
        true,
        Utc::now(),
    ).unwrap();

    let mut links_to_visit: Vec<Link> = uri_result.links.iter()
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
    let mut response = client.get(url.clone()).await?;

    println!("Status: {}", response.status());
    println!("Headers: {:#?}\n", response.headers());

    let body: String = String::from_utf8_lossy(hyper::body::to_bytes(response.into_body()).await?.as_ref()).to_string();
    // println!("BODY: {}", body);

    println!("\nDone!");

    Ok(body)
}