use linkresult::{Link, ResponseTimings};
use hyper::{Client, Request, Response, Body, StatusCode, Uri};
use hyper::http::HeaderValue;
use hyper_tls::HttpsConnector;
use crate::page::Page;
use hyper::body::HttpBody;

pub mod page;

// A simple type alias so as to DRY.
pub type DynResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub async fn fetch_url(url: &hyper::Uri) -> DynResult<String> {
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
    let content_type = head.headers().get("content-type");
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

pub async fn fetch_page(page: &mut Page) -> DynResult<String> {
    println!("URI: {}", page.uri);

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    let req = Request::builder()
        .method("HEAD")
        .uri(&page.uri)
        .body(Body::from(""))
        .expect("HEAD request builder");

    let head = client.request(req).await?;
    if !head.status().is_success() {
        page.set_response(head);
        return Err(format!("HTTP Status: {}", page.get_response().status()).into());
    }

    if let Some(content_type) = page.get_content_type() {
        if !content_type.contains("text/html") { return Err(format!("Content-Type: {}", content_type).into()); }

        page.set_response(client.get(page.uri.clone()).await?);

        // println!("Status: {}", response.status());
        // println!("Headers: {:#?}\n", response.headers());

        // println!("BODY: {}", body);

        // println!("\nDone!");
    }

    Ok(String::from(""))
}