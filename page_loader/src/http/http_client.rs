use std::ops::Sub;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use hyper::{Body, Client, Request, Response};
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;
use rand::random;
use tracing::debug;

#[async_trait]
pub trait HttpClient: Sync + Send {
    async fn head(&self, uri: String, robots_txt_info_url: Option<String>) -> hyper::Result<Response<Body>>;
    async fn get(&self, uri: String, robots_txt_info_url: Option<String>) -> hyper::Result<Response<Body>>;
}

pub struct HttpClientImpl {
    user_agent: String,
    client: Client<HttpsConnector<HttpConnector>>,
    rate_limiting_ms: usize,
    last_request_timestamp: Arc<Mutex<Option<Instant>>>,
}

impl HttpClientImpl {
    pub fn new(user_agent: String, rate_limiting_ms: usize) -> HttpClientImpl {
        let connector = HttpsConnector::new();
        HttpClientImpl::new_(connector, user_agent, rate_limiting_ms)
    }

    #[cfg(test)]
    pub fn new_with_timeout(user_agent: String, rate_limiting_ms: usize, timeout_ms: usize) -> HttpClientImpl {
        let mut http_connector = HttpConnector::new();
        http_connector.set_connect_timeout(Some(Duration::from_millis(timeout_ms as u64)));
        let https_connector = HttpsConnector::new_with_connector(http_connector);
        HttpClientImpl::new_(https_connector, user_agent, rate_limiting_ms)
    }

    fn new_(connector: HttpsConnector<HttpConnector>, user_agent: String, rate_limiting_ms: usize) -> HttpClientImpl {
        HttpClientImpl {
            user_agent,
            client: Client::builder().build::<_, hyper::Body>(connector),
            rate_limiting_ms,
            last_request_timestamp: Arc::new(Mutex::new(Some(Instant::now().sub(Duration::from_millis(rate_limiting_ms as u64))))),
        }
    }

    async fn send_request(&self, method: &str, uri: String, robots_txt_info_url: Option<String>) -> hyper::Result<Response<Body>> {
        while self.is_blocked() {
            let sleep_duration = (random::<f64>() * self.rate_limiting_ms as f64) as u64 + self.rate_limiting_ms as u64;
            debug!("Rate limiting request {}. Random limit: {}ms; Config Setting: {}ms", uri, sleep_duration, self.rate_limiting_ms);
            // tokio::time::sleep(Duration::from_millis(sleep_duration)).await;
            tokio::task::yield_now().await;
        }

        let user_agent_string = format!("{}{}", self.user_agent,
                                        robots_txt_info_url
                                            .map_or("".into(),
                                                    |it| format!(" +{}", it)));

        let req = Request::builder()
            .header("user-agent", user_agent_string)
            .method(method)
            .uri(uri.clone())
            .body(Body::from(""))
            .expect(&format!("{} request builder", method));

        debug!("request {}", uri);
        let result = self.client.request(req).await;
        let instant = self.last_request_timestamp.lock().unwrap().unwrap();
        debug!("request end {}, last_request_timestamp {:?}", uri,instant);
        self.last_request_timestamp.lock().unwrap().replace(Instant::now());
        let instant = self.last_request_timestamp.lock().unwrap().unwrap();
        debug!("request end {}, last_request_timestamp {:?}", uri,instant);

        result
    }

    fn is_blocked(&self) -> bool {
        debug!("is_blocked: elapsed {}", self.last_request_timestamp.lock().unwrap().unwrap().elapsed().as_millis());
        self.last_request_timestamp.lock().unwrap().unwrap()
            .elapsed().as_millis() <= self.rate_limiting_ms as u128
    }
}

#[async_trait]
impl HttpClient for HttpClientImpl {
    async fn head(&self, uri: String, robots_txt_info_url: Option<String>) -> hyper::Result<Response<Body>> {
        self.send_request("HEAD", uri, robots_txt_info_url).await
    }

    async fn get(&self, uri: String, robots_txt_info_url: Option<String>) -> hyper::Result<Response<Body>> {
        self.send_request("GET", uri, robots_txt_info_url).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rate_limit_is_respected_properly() {
        // given: a client
        let rate_limit = 11; // the rate limit must be high enough to include http timeout, set below HttpClientImpl::new_with_timeout
        let client = Arc::new(HttpClientImpl::new_with_timeout("test-client".into(), rate_limit, 10));
        let client_clone_1 = client.clone();
        let client_clone_2 = client.clone();
        let client_clone_3 = client.clone();
        let first_timestamp = Arc::new(Mutex::new(Some(Instant::now())));
        let second_timestamp = Arc::new(Mutex::new(Some(Instant::now())));
        let third_timestamp = Arc::new(Mutex::new(Some(Instant::now())));
        let first_timestamp_clone = first_timestamp.clone();
        let second_timestamp_clone = second_timestamp.clone();
        let third_timestamp_clone = third_timestamp.clone();

        // when: client is invoked several times within rate_limiting_ms
        let _ = tokio::join!(
            tokio::spawn(async move {
                let _ = client_clone_1.send_request("GET", String::from("https://localhost:12345"), None).await;
                first_timestamp.lock().unwrap().replace(Instant::now());
            }),
            tokio::spawn(async move {
                let _ = client_clone_2.send_request("GET", String::from("https://localhost:12345"), None).await;
                second_timestamp.lock().unwrap().replace(Instant::now());
            }),
            tokio::spawn(async move {
                let _ = client_clone_3.send_request("GET", String::from("https://localhost:12345"), None).await;
                third_timestamp.lock().unwrap().replace(Instant::now());
            })
        );

        // then: rate is limited appropriately
        // note that first, second and third are probably unordered, due to threading. That's why we need to bring them in order first
        let mut ordered_times = vec![first_timestamp_clone, second_timestamp_clone, third_timestamp_clone];
        ordered_times.sort_by(|a, b| a.lock().unwrap().unwrap().cmp(&b.lock().unwrap().unwrap()));
        let second_first_diff = ordered_times[1].lock().unwrap().unwrap().duration_since(ordered_times[0].lock().unwrap().unwrap()).as_millis();
        let third_second_diff = ordered_times[2].lock().unwrap().unwrap().duration_since(ordered_times[1].lock().unwrap().unwrap()).as_millis();
        assert_eq!(second_first_diff >= rate_limit as u128, true);
        assert_eq!(third_second_diff >= rate_limit as u128, true);
    }
}