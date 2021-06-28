use async_trait::async_trait;
use hyper::{Body, Response};

#[async_trait]
pub trait HttpClient: Sync + Send {
    async fn head(&self, uri: String) -> Result<Response<Body>, String>;
}

pub struct HttpClientImpl {}

impl HttpClientImpl {
    pub fn new() -> HttpClientImpl {
        HttpClientImpl {}
    }
}

#[async_trait]
impl HttpClient for HttpClientImpl {
    async fn head(&self, uri: String) -> Result<Response<Body>, String> {
        todo!()
    }
}