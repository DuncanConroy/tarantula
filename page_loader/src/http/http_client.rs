use std::fmt::Debug;
use std::time::Duration;

use async_trait::async_trait;
use hyper::{Body, Client, Response};
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;

#[async_trait]
pub trait HttpClient: Sync + Send + Debug {
    async fn head(&self, uri: String) -> Result<Response<Body>, String>;
    async fn get(&self, uri: String) -> Result<Response<Body>, String>;
}

#[derive(Debug)]
pub struct HttpClientImpl {
    user_agent: String,
    client: Client<HttpsConnector<HttpConnector>>,
}

impl HttpClientImpl {
    pub fn new(user_agent: String) -> HttpClientImpl {
        let connector = HttpsConnector::new();
        HttpClientImpl {
            user_agent,
            client: Client::builder().build::<_, hyper::Body>(connector),
        }
    }

    #[cfg(test)]
    #[allow(unused_variables)] // allowing, as this should only panic
    async fn send_request(&self, method: &str, uri: String) -> Result<Response<Body>, String> {
        panic!("Don't send requests in test!")
    }

    #[cfg(not(test))]
    async fn send_request(&self, method: &str, uri: String) -> Result<Response<Body>, String> {
        use hyper::Request;
        loop {
            let req = Request::builder()
                .header("user-agent", self.user_agent.clone())
                .method(method)
                .uri(uri.clone())
                .body(Body::from(""))
                .expect(&format!("{} request builder", method));

            match self.client.request(req).await {
                Ok(result) => { return Ok(result); }
                _ => {
                    tokio::time::sleep(Duration::from_millis(1_000)).await;
                }
            }
        }
    }
}

#[async_trait]
impl HttpClient for HttpClientImpl {
    async fn head(&self, uri: String) -> Result<Response<Body>, String> {
        self.send_request("HEAD", uri).await
    }

    async fn get(&self, uri: String) -> Result<Response<Body>, String> {
        self.send_request("GET", uri).await
    }
}