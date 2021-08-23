use std::fmt::{Debug, Formatter};
use std::fmt;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use hyper::{Body, Client, Request, StatusCode, Uri};
use hyper::header::USER_AGENT;
use hyper_tls::HttpsConnector;
use log::{debug, info, warn};
use robotstxt_with_cache::{DefaultCachingMatcher, DefaultMatcher};

#[async_trait]
pub trait RobotsTxtInit {
    async fn init(&mut self, uri: Uri);
}

pub trait RobotsTxt: Sync + Send {
    fn can_access(&self, item_uri: &str) -> bool;
}

pub struct RobotsService {
    robot_file_parser: Arc<Mutex<DefaultCachingMatcher>>,
    uri: Option<Uri>,
    user_agent: String,
    disallow_all: AtomicBool,
    allow_all: AtomicBool,
    is_initialized: AtomicBool,
}

impl RobotsService {
    pub fn new(user_agent: String) -> RobotsService {
        let instance = RobotsService {
            robot_file_parser: Arc::new(Mutex::new(DefaultCachingMatcher::new(DefaultMatcher::default()))),
            uri: None,
            user_agent,
            disallow_all: AtomicBool::new(false),
            allow_all: AtomicBool::new(false),
            is_initialized: AtomicBool::new(false),
        };

        instance
    }
}

impl RobotsTxt for RobotsService {
    fn can_access(&self, item_uri: &str) -> bool {
        !self.disallow_all.load(Ordering::Acquire) &&
            (self.allow_all.load(Ordering::Acquire)
                || self.robot_file_parser.clone().lock().unwrap().one_agent_allowed_by_robots(&self.user_agent, item_uri))
    }
}

#[async_trait]
impl RobotsTxtInit for RobotsService {
    async fn init(&mut self, uri: Uri) {
        if self.is_initialized.load(Ordering::Acquire) {
            panic!("RobotService is already initialized.");
        }

        self.uri = Some(uri);

        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);

        let request = Request::builder()
            .method("GET")
            .uri(self.uri.clone().unwrap().clone())
            .header(USER_AGENT, self.user_agent.clone())
            .body(Body::from(""))
            .expect("GET request builder");

        async {
            let response = match client.request(request).await {
                Ok(res) => res,
                Err(_) => {
                    let uri = self.uri.clone().unwrap().to_string();
                    warn!("Couldn't fetch robots.txt for {}", uri);
                    return;
                }
            };

            let status = response.status();
            match status {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                    self.disallow_all.store(true, Ordering::Release);
                    let uri = self.uri.clone().unwrap().to_string();
                    info!("Got status {} for {}, setting DISALLOW_ALL: true", status, uri);
                }
                status if status >= StatusCode::BAD_REQUEST && status < StatusCode::INTERNAL_SERVER_ERROR => {
                    self.allow_all.store(true, Ordering::Release);
                    let uri = self.uri.clone().unwrap().to_string();
                    info!("Got status {} for {}, setting ALLOW_ALL: true", status, uri);
                }
                StatusCode::OK => {
                    let body = response.into_body();
                    let result = String::from_utf8_lossy(hyper::body::to_bytes(body).await.unwrap().as_ref())
                        .to_string();
                    let uri = self.uri.clone().unwrap().to_string();
                    let uri_clone = uri.clone();
                    debug!("Received robots.txt for {}, parsing...", uri);
                    self.robot_file_parser.clone().lock().unwrap().parse(&result);
                    info!("Parsed robots.txt for {},", uri_clone);
                }
                _ => {}
            }

            self.is_initialized.store(true, Ordering::SeqCst);
        }.await
    }
}

impl Debug for RobotsService {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_tuple("RobotsService")
            .field(&self.uri)
            .field(&self.user_agent)
            .field(&self.disallow_all)
            .field(&self.allow_all)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_not_access_on_disallow_all() {
        // given: a robots.txt with disallow_all = true
        let service = RobotsService::new("tarantula".into());
        service.disallow_all.store(true, Ordering::Release);

        // when: can_access is invoked
        let can_access = service.can_access("https://example.com");

        // then: result is false
        assert_eq!(can_access, false, "Should not crawl anything with disallow_all=true")
    }

    #[test]
    fn can_access_on_allow_all(){
        // given: a robots.txt with allow_all = true
        let service = RobotsService::new("tarantula".into());
        service.disallow_all.store(false, Ordering::Release);
        service.allow_all.store(true, Ordering::Release);

        // when: can_access is invoked
        let can_access = service.can_access("https://example.com");

        // then: result is false
        assert_eq!(can_access, true, "Should not crawl anything with disallow_all=true")
    }

    #[test]
    fn disallow_all_precedes_allow_all(){
        // given: a robots.txt with disallow_all = true and allow_all = true
        let service = RobotsService::new("tarantula".into());
        service.allow_all.store(true, Ordering::Release);
        service.disallow_all.store(true, Ordering::Release);

        // when: can_access is invoked
        let can_access = service.can_access("https://example.com");

        // then: result is false
        assert_eq!(can_access, false, "Should not crawl anything with disallow_all=true")
    }

    #[test]
    fn can_access_if_only_robots_txt_permits(){
        // given: a robots.txt which allows us access
        let service = RobotsService::new("tarantula".into());
        let robots_body = "user-agent: tarantula\n\
                           disallow: /\n";
        service.robot_file_parser.lock().unwrap().parse(robots_body);
        service.allow_all.store(true, Ordering::Release);
        service.disallow_all.store(true, Ordering::Release);

        // when: can_access is invoked
        let can_access = service.can_access("https://example.com/some-otherwise-forbidden-deeplink");

        // then: result is false
        assert_eq!(can_access, false, "Should not crawl anything with disallow_all=true")
    }
}
