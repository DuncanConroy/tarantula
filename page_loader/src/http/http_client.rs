use std::ops::Sub;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use hyper::{Body, Client, Request, Response};
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;
use log::{debug, info};
use rand::random;

#[async_trait]
pub trait HttpClient: Sync + Send {
    async fn head(&self, uri: String) -> hyper::Result<Response<Body>>;
    async fn get(&self, uri: String) -> hyper::Result<Response<Body>>;
}

struct Singularity {
    url: Option<String>,
}

unsafe impl Sync for Singularity {}

unsafe impl Send for Singularity {}

pub struct HttpClientImpl {
    user_agent: String,
    client: Client<HttpsConnector<HttpConnector>>,
    rate_limiting_ms: usize,
    last_request_timestamp: Arc<Mutex<Option<Instant>>>,
    singularity_lock: Arc<Mutex<Singularity>>,
}

impl HttpClientImpl {
    pub fn new(user_agent: String, rate_limiting_ms: usize) -> HttpClientImpl {
        let connector = HttpsConnector::new();
        HttpClientImpl {
            user_agent,
            client: Client::builder().build::<_, hyper::Body>(connector),
            rate_limiting_ms,
            last_request_timestamp: Arc::new(Mutex::new(Some(Instant::now().sub(Duration::from_millis(rate_limiting_ms as u64))))),
            singularity_lock: Arc::new(Mutex::new(Singularity { url: None })),
        }
    }

    async fn send_request(&self, method: &str, uri: String) -> hyper::Result<Response<Body>> {
        'retry: loop {
            if self.is_blocked() {
                let sleep_duration = (random::<f64>() * self.rate_limiting_ms as f64) as u64 + self.rate_limiting_ms as u64;
                debug!("Rate limiting request {}. Random limit: {}ms; Config Setting: {}ms", uri, sleep_duration, self.rate_limiting_ms);
                tokio::time::sleep(Duration::from_millis(sleep_duration)).await;
            } else if self.singularity_lock.lock().unwrap().url.is_none() {
                self.singularity_lock.lock().unwrap().url.replace(uri.clone());

                let req = Request::builder()
                    .header("user-agent", self.user_agent.clone())
                    .method(method)
                    .uri(uri.clone())
                    .body(Body::from(""))
                    .expect(&format!("{} request builder", method));

                info!("request {}", uri);
                let result = self.client.request(req).await;
                let instant = self.last_request_timestamp.lock().unwrap().unwrap();
                info!("request end {}, last_request_timestamp {:?}", uri,instant);
                self.last_request_timestamp.lock().unwrap().replace(Instant::now());
                let instant = self.last_request_timestamp.lock().unwrap().unwrap();
                info!("request end {}, last_request_timestamp {:?}", uri,instant);

                self.singularity_lock.lock().unwrap().url.take();

                return result;
            }

            continue 'retry;
        }
    }

    fn is_blocked(&self) -> bool {
        debug!("is_blocked: elapsed {}", self.last_request_timestamp.lock().unwrap().unwrap().elapsed().as_millis());
        self.singularity_lock.lock().unwrap().url.is_some()
            || self.last_request_timestamp.lock().unwrap().unwrap()
            .elapsed().as_millis() <= self.rate_limiting_ms as u128
    }
}

#[async_trait]
impl HttpClient for HttpClientImpl {
    async fn head(&self, uri: String) -> hyper::Result<Response<Body>> {
        self.send_request("HEAD", uri).await
    }

    async fn get(&self, uri: String) -> hyper::Result<Response<Body>> {
        self.send_request("GET", uri).await
    }
}

#[cfg(test)]
mod tests {
    fn test() {
        todo!("needs test to make sure rate limit is respected");
    }
}