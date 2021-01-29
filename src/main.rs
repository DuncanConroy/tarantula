use std::env;

use hyper::{body::HttpBody, Client};
use hyper_tls::HttpsConnector;
use tokio::io::{self, AsyncWriteExt};

use dom_parser::parse_body;

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
    let dom = dom_parser::parse_body(&url.host().unwrap(), &mut body);

    Ok(())
}

async fn fetch_url(url: &hyper::Uri) -> Result<String> {
    println!("URI: {}", url);

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let mut response = client.get(url.clone()).await?;

    println!("Status: {}", response.status());
    println!("Headers: {:#?}\n", response.headers());

    // let body = String::from(res.data().await?.unwrap());
    let body = String::from_utf8(hyper::body::to_bytes(response.into_body()).await?.to_vec())?;
    // println!("BODY: {}", body);

    // Stream the body, writing each chunk to stdout as we get it
    // (instead of buffering and printing at the end).
    // while let Some(next) = res.data().await {
    //     let chunk = next?;
    //     io::stdout().write_all(&chunk).await?;
    // }

    // println!("\n\nDone!");

    Ok(body)
}

