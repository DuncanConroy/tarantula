use std::env;

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
    let links = dom_parser::get_links(&url.host().unwrap(), &mut body);

    // links.iter()
    //     .map(|it|it.parse::<hyper::Uri>().unwrap())
    //     .then(|uri|fetch_url(&uri));

    Ok(())
}

async fn fetch_url(url: &hyper::Uri) -> Result<String> {
    println!("URI: {}", url);

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let mut response = client.get(url.clone()).await?;

    println!("Status: {}", response.status());
    println!("Headers: {:#?}\n", response.headers());

    let body:String = String::from_utf8_lossy(hyper::body::to_bytes(response.into_body()).await?.as_ref()).to_string();
    // println!("BODY: {}", body);

    println!("\n\nDone!");

    Ok(body)
}