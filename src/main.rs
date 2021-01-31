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

    // let mut total_links: Vec<String>;
    // links.iter().for_each(|it| {
    //     let item_url = &it.parse::<hyper::Uri>().unwrap();
    //     let mut item_body = fetch_url(item_url).await?;
    //     let item_links = dom_parser::get_links(&item_url.host().unwrap(), &mut body);
    // });

    // let total_links = links.iter()
    //     .map(|it| it.parse::<hyper::Uri>().unwrap())
    //     .map(|it| fetch_url(&it.host().unwrap()))
    //     .collect::<FuturesUnordered<_>>()
    //     .collect::<Vec<_>>()
    //     .await;

    //TODO: recursive function, multi-threaded, return link object with metadata

    let mut total_links: Vec<String> = vec![];
    // total_links.append(&mut links);
    for link in links {
        // if total_links.contains(&link) {
        //     println!("Skipping {}, as it's already a known link.", link);
        //     continue;
        // }

        let item_url_string = "https://".to_owned() + url.host().unwrap() + &link;
        let item_url = item_url_string.parse::<hyper::Uri>().unwrap();
        println!("trying {}", item_url);
        let mut item_body = fetch_url(&item_url).await?;
        let mut item_links = dom_parser::get_links(&item_url.host().unwrap(), &mut item_body, true);
        total_links.append(&mut item_links);
    }

    println!("total_links: {:?}", total_links);

    Ok(())
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