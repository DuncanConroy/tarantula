use std::ops::{AddAssign, Sub};
use std::sync::{Mutex, Arc};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use hyper::{Body, Client, Response, Request};
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;
use log::debug;
use rand::random;

#[async_trait]
pub trait HttpClient: Sync + Send {
    async fn head(&self, uri: String) -> Result<Response<Body>, String>;
    async fn get(&self, uri: String) -> Result<Response<Body>, String>;
}

pub struct HttpClientImpl {
    user_agent: String,
    client: Client<HttpsConnector<HttpConnector>>,
    rate_limiting_ms: usize,
    last_request_timestamp: Arc<Mutex<Instant>>,
}

impl HttpClientImpl {
    pub fn new(user_agent: String, rate_limiting_ms: usize) -> HttpClientImpl {
        let connector = HttpsConnector::new();
        HttpClientImpl {
            user_agent,
            client: Client::builder().build::<_, hyper::Body>(connector),
            rate_limiting_ms,
            last_request_timestamp: Arc::new(Mutex::new(Instant::now().sub(Duration::from_millis(rate_limiting_ms as u64)))),
        }
    }

    async fn send_request(&self, method: &str, uri: String) -> Result<Response<Body>, String> {
        'retry: loop {
            if self.last_request_timestamp.lock().unwrap().elapsed().as_millis() <= self.rate_limiting_ms as u128 {
                let sleep_duration = (random::<f64>() * self.rate_limiting_ms as f64) as u64 + self.rate_limiting_ms as u64;
                debug!("Rate limiting requests. Random limit: {}ms; Config Setting: {}ms", sleep_duration, self.rate_limiting_ms);
                tokio::time::sleep(Duration::from_millis(sleep_duration)).await;
            }
            match self.last_request_timestamp.lock() {
                Ok(mut instant) => {
                    let elapsed = instant.elapsed();
                    instant.add_assign(elapsed);
                }
                Err(_) => {
                    continue 'retry;
                }
            }

            let req = Request::builder()
                .header("user-agent", self.user_agent.clone())
                .method(method)
                .uri(uri.clone())
                .body(Body::from(""))
                .expect(&format!("{} request builder", method));

            match self.client.request(req).await {
                Ok(result) => {
                    return Ok(result);
                }
                Err(error_message) => {
                    debug!("Request unsuccessful. Waiting to retry {}ms. {}", self.rate_limiting_ms, error_message);
                    tokio::time::sleep(Duration::from_millis(self.rate_limiting_ms as u64 * 10)).await;
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