use std::env;

use futures::{
    // futures_unordered::FuturesUnordered,
    stream::{Stream, StreamExt},
};
use hyper::{body::HttpBody, Client};
use hyper_tls::HttpsConnector;
use tokio::io::{self, AsyncWriteExt};

use dom_parser::get_links;

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

    let url = url.parse::<hyper::Uri>().unwrap();

    let mut body = fetch_url(&url).await?;
    println!("HOST:{}", &url.host().unwrap());
    let mut links = dom_parser::get_links(&url.host().unwrap(), &mut body, true);
    println!("links: {:?}", links);

    //TODO: recursive function, multi-threaded, return link object with metadata

    let protocol = format!("{}://", &url.scheme().unwrap());
    let mut known_links = vec!["/".to_string()];
    let total_links = recursive_load_page_and_get_links(&protocol, url.host().unwrap(), &links, &mut known_links);

    println!("total_links: {:?}", total_links.await?);

    Ok(())
}

async fn recursive_load_page_and_get_links(protocol: &str, host: &str, links: &Vec<String>, known_links: &mut Vec<String>) -> Result<Vec<String>> {
    let mut all_known_links: Vec<String> = vec![];
    all_known_links.append(known_links);

    for link in links {
        // if total_links.contains(&link) {
        //     println!("Skipping {}, as it's already a known link.", link);
        //     continue;
        // }

        let item_url_string = create_url_string(&protocol, &host, &link);
        println!("item_url_string {}", item_url_string);
        let item_url = item_url_string.parse::<hyper::Uri>().unwrap();
        println!("trying {}", item_url);
        let mut item_body = fetch_url(&item_url).await?;
        let mut item_links = dom_parser::get_links(&item_url.host().unwrap(), &mut item_body, true);
        let mut links_to_visit: Vec<String> = item_links.iter()
            .filter(|it| !all_known_links.contains(it))
            .map(|it| it.to_string())
            .collect();
        println!("found {} links to visit: {:?}", links_to_visit.len(), links_to_visit);

        recursive_load_page_and_get_links(protocol, host, &links_to_visit, &mut all_known_links);
        all_known_links.append(&mut links_to_visit);
    }

    Ok(all_known_links)
}

fn create_url_string(protocol: &str, host: &str, link: &String) -> String {
    println!("#-> {},{},{}",host,protocol,link);
    if link.starts_with("http") {
        link.to_owned ()
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