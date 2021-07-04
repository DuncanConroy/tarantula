use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hyper::StatusCode;
use log::trace;

use crate::http::http_client::HttpClient;
use crate::http::http_utils;
use crate::page_request::PageRequest;
use crate::response_timings::ResponseTimings;

#[async_trait]
pub trait PageDownloadCommand: Sync + Send {
    async fn download_page(&self, page_request: Arc<Mutex<PageRequest>>, http_client: Box<dyn HttpClient>) -> Result<PageDownloadResponse, String>;
}

pub struct DefaultPageDownloadCommand {}

#[async_trait]
impl PageDownloadCommand for DefaultPageDownloadCommand {
    async fn download_page(&self, page_request: Arc<Mutex<PageRequest>>, http_client: Box<dyn HttpClient>) -> Result<PageDownloadResponse, String> {
        let start_time = DateTime::from(Utc::now());
        let uri = page_request.lock().unwrap().url.clone();

        let response = http_client.get(uri.clone()).await.unwrap();
        trace!("GET for {}: {:?}", uri, response.headers());
        let headers: HashMap<String, String> = http_utils::response_headers_to_map(&response);

        let status = response.status();
        let body: String = String::from_utf8_lossy(hyper::body::to_bytes(response.into_body()).await.unwrap().as_ref())
            .to_string();
        let result = PageDownloadResponse {
            http_response_code: status,
            headers,
            requested_url: uri.clone(),
            response_timings: ResponseTimings::from(uri.clone(), start_time, DateTime::from(Utc::now())),
            body: Some(body),
        };
        Ok(result)
    }
}

#[derive(Debug, Clone)]
pub struct PageDownloadResponse {
    pub requested_url: String,
    pub body: Option<String>,
    pub http_response_code: StatusCode,
    pub headers: HashMap<String, String>,
    pub response_timings: ResponseTimings,
}

impl PageDownloadResponse {
    pub fn new(requested_url: String, http_response_code: StatusCode) -> PageDownloadResponse {
        PageDownloadResponse {
            requested_url: requested_url.clone(),
            body: None,
            http_response_code,
            headers: HashMap::new(),
            response_timings: ResponseTimings::new(format!("FetchHeaderResponse.{}", requested_url.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::{Debug, Formatter, Result};
    use std::time::Duration;

    use hyper::{Body, Response};
    use mockall::*;
    use tokio::time::Instant;
    use uuid::Uuid;

    use linkresult::uri_service::UriService;

    use crate::task_context::robots_service::RobotsTxt;
    use crate::task_context::task_context::{FullTaskContext, KnownLinks, TaskConfig, TaskContext, TaskContextServices};

    use super::*;

    mock! {
        MyTaskContext {}
        impl TaskContext for MyTaskContext {
            fn get_uuid_clone(&self) -> Uuid;
            fn get_config(&self) -> Arc<Mutex<TaskConfig>>;
            fn get_url(&self)->String;
            fn get_last_command_received(&self) -> Instant;
            fn set_last_command_received(&mut self, instant: Instant);
            fn can_be_garbage_collected(&self, gc_timeout_ms: u64) -> bool;
        }
        impl TaskContextServices for MyTaskContext{
            fn get_uri_service(&self) -> Arc<UriService>;
        }
        impl KnownLinks for MyTaskContext{
            fn get_all_known_links(&self) -> Arc<Mutex<Vec<String>>>;
            fn add_known_link(&self, link: String);
        }
        impl RobotsTxt for MyTaskContext{
            fn can_access(&self, item_uri: &str) -> bool;
            fn get_crawl_delay(&self) -> Option<Duration>;
        }
        impl FullTaskContext for MyTaskContext{}

        impl Debug for MyTaskContext {
            fn fmt<'a>(&self, f: &mut Formatter<'a>) -> Result;
        }
    }
    mock! {
        MyHttpClient {}
        #[async_trait]
        impl HttpClient for MyHttpClient{
            async fn head(&self, uri: String) -> std::result::Result<Response<Body>, String>;
            async fn get(&self, uri: String) -> std::result::Result<Response<Body>, String>;
        }
    }

    #[tokio::test]
    async fn returns_simple_result_on_simple_request() {
        // given: simple download command
        let command = DefaultPageDownloadCommand {};
        let mut mock_task_context = MockMyTaskContext::new();
        let task_config = TaskConfig::new("https://example.com".into());
        mock_task_context.expect_get_config().return_const(Arc::new(Mutex::new(task_config)));
        let page_request = PageRequest::new("https://example.com".into(), None, Arc::new(Mutex::new(mock_task_context)));
        let mut mock_http_client = Box::new(MockMyHttpClient::new());
        mock_http_client.expect_get().returning(|_| Ok(Response::builder()
            .status(200)
            .body(Body::from("Hello World"))
            .unwrap()));

        // when: fetch is invoked
        let result = command.download_page(Arc::new(Mutex::new(page_request)), mock_http_client).await;

        // then: simple response is returned, with no redirects
        assert_eq!(result.is_ok(), true, "Expecting a simple Response");
        assert_eq!(result.as_ref().unwrap().body.is_some(), true, "Should have body");
        assert_eq!(result.as_ref().unwrap().body.as_ref().unwrap(), "Hello World", "Should have body");
        assert_eq!(result.as_ref().unwrap().response_timings.end_time.is_some(), true, "Should have updated end_time after successful run");
    }
}