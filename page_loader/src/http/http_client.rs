use async_trait::async_trait;
use hyper::{Body, Client, Request, Response};
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;

#[async_trait]
pub trait HttpClient: Sync + Send {
    async fn head(&self, uri: String) -> Result<Response<Body>, String>;
}

pub struct HttpClientImpl {
    client: Client<HttpsConnector<HttpConnector>>,
}

impl HttpClientImpl {
    pub fn new() -> HttpClientImpl {
        let connector = HttpsConnector::new();
        HttpClientImpl {
            client: Client::builder().build::<_, hyper::Body>(connector)
        }
    }
}

#[async_trait]
impl HttpClient for HttpClientImpl {
    async fn head(&self, uri: String) -> Result<Response<Body>, String> {
        let req = Request::builder()
            .method("HEAD")
            .uri(uri.clone())
            .body(Body::from(""))
            .expect("HEAD request builder");

        Ok(self.client.request(req).await.unwrap())
    }
}