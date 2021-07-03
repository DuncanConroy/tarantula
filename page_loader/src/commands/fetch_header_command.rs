use std::collections::HashMap;
use std::iter::Map;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use hyper::{Body, Response, StatusCode, Uri};
use log::{debug, info, trace};
#[cfg(test)]
use mockall::automock;

use crate::http::http_client::HttpClient;
use crate::page_request::PageRequest;

#[async_trait]
pub trait FetchHeaderCommand: Sync + Send {
    async fn fetch_header(&self, page_request: Arc<Mutex<PageRequest>>, http_client: Box<dyn HttpClient>, redirects: Option<Vec<Redirect>>) -> Result<FetchHeaderResponse, String>;
}

pub struct DefaultFetchHeaderCommand {}

#[async_trait]
impl FetchHeaderCommand for DefaultFetchHeaderCommand {
    async fn fetch_header(&self, page_request: Arc<Mutex<PageRequest>>, http_client: Box<dyn HttpClient>, redirects: Option<Vec<Redirect>>) -> Result<FetchHeaderResponse, String> {
        let mut uri = page_request.lock().unwrap().url.clone();
        let maximum_redirects = page_request.lock().unwrap().task_context.lock().unwrap().get_config().lock().unwrap().maximum_redirects;

        let mut num_redirects = 0;
        if redirects.is_some() {
            let redirects_unwrapped = redirects.as_ref().unwrap();
            num_redirects = redirects_unwrapped.len() as u16;
            uri = redirects_unwrapped.last().unwrap().destination.clone();
        }

        let response = http_client.head(uri.clone()).await.unwrap();
        trace!("HEAD for {}: {:?}", uri, response.headers());
        let headers: HashMap<String, String> = response.headers().iter().map(|(key, value)| { (key.to_string(), String::from(value.to_str().unwrap())) }).collect();
        if num_redirects < maximum_redirects && response.status().is_redirection() {
            if let Some(location_header) = response.headers().get("location") {
                let uri_service = page_request.lock().unwrap().task_context.lock().unwrap().get_uri_service();
                let uri_object = Uri::from_str(&uri).unwrap();
                let adjusted_uri = uri_service.form_full_url(uri_object.scheme_str().unwrap(), location_header.to_str().unwrap(), uri_object.host().unwrap(), &Some(uri.clone()));
                let redirect = Redirect { source: uri, destination: adjusted_uri.to_string(), http_response_code: response.status(), headers: headers.clone() };
                debug!("Following redirect {}", adjusted_uri);
                let mut redirects_for_next = vec![];
                if redirects.is_some() {
                    redirects_for_next.append(&mut redirects.unwrap());
                }
                redirects_for_next.push(redirect);
                let response = self.fetch_header(page_request.clone(), http_client, Some(redirects_for_next)).await;
                return response;
            }
            let error_message = format!("No valid location found in redirect header {:?}", response);
            info!("{}", &error_message);
        }

        let redirects_result = redirects.unwrap_or(vec![]);
        let result = FetchHeaderResponse { redirects: redirects_result, http_response_code: response.status(), headers };
        Ok(result)
    }
}

#[derive(Debug, Clone)]
pub struct Redirect {
    source: String,
    destination: String,
    http_response_code: StatusCode,
    headers: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct FetchHeaderResponse {
    pub redirects: Vec<Redirect>,
    pub http_response_code: StatusCode,
    pub headers: HashMap<String, String>,
}

impl FetchHeaderResponse {
    pub fn new(http_response_code: StatusCode) -> FetchHeaderResponse {
        FetchHeaderResponse {
            redirects: vec![],
            http_response_code,
            headers: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::{Debug, Formatter, Result};
    use std::str::FromStr;
    use std::time::Duration;

    use mockall::*;
    use mockall::predicate::eq;
    use tokio::test;
    use tokio::time::Instant;
    use uuid::Uuid;

    use linkresult::LinkTypeChecker;
    use linkresult::uri_service::UriService;

    use crate::task_context::robots_service::RobotsTxt;
    use crate::task_context::task_context::{DefaultTaskContext, FullTaskContext, KnownLinks, TaskConfig, TaskContext, TaskContextInit, TaskContextServices};

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
        }
    }

    #[tokio::test]
    async fn returns_simple_result_on_simple_request_without_redirect_following() {
        // given: simple fetch command
        let command = DefaultFetchHeaderCommand {};
        let mut mock_task_context = MockMyTaskContext::new();
        let task_config = TaskConfig::new("https://example.com".into());
        mock_task_context.expect_get_config().return_const(Arc::new(Mutex::new(task_config)));
        let page_request = PageRequest::new("https://example.com".into(), None, Arc::new(Mutex::new(mock_task_context)));
        let mut mock_http_client = MockMyHttpClient::new();
        mock_http_client.expect_head().returning(|_| Ok(Response::builder()
            .status(200)
            .body(Body::from(""))
            .unwrap()));

        // when: fetch is invoked
        let result = command.fetch_header(Arc::new(Mutex::new(page_request)), Box::new(mock_http_client), None).await;

        // then: simple response is returned, with no redirects
        assert_eq!(result.is_ok(), true, "Expecting a simple Response");
        assert_eq!(result.unwrap().redirects.len(), 0, "Should not have any redirects");
    }

    #[tokio::test]
    async fn should_return_redirect_list_up_to_max_redirects() {
        // given: simple fetch command
        let command = DefaultFetchHeaderCommand {};
        let mut mock_task_context = MockMyTaskContext::new();
        let uri_service = Arc::new(UriService::new(Arc::new(LinkTypeChecker::new("example.com"))));
        mock_task_context.expect_get_uri_service().return_const(uri_service.clone());
        let mut task_config = TaskConfig::new("https://example.com".into());
        task_config.maximum_redirects = 2;
        mock_task_context.expect_get_config().return_const(Arc::new(Mutex::new(task_config)));
        let mut mock_http_client = MockMyHttpClient::new();
        let mut sequence = Sequence::new();
        mock_http_client.expect_head()
            .with(eq(String::from("https://example.com")))
            .times(1)
            .in_sequence(&mut sequence)
            .returning(|_| Ok(Response::builder()
                .status(308)
                .header("location", "https://first-redirect.example.com/")
                .body(Body::from(""))
                .unwrap()));
        mock_http_client.expect_head()
            .with(eq(String::from("https://first-redirect.example.com/")))
            .times(1)
            .in_sequence(&mut sequence)
            .returning(|_| Ok(Response::builder()
                .status(308)
                .header("location", "https://second-redirect.example.com")
                .header("x-custom", "Hello World")
                .body(Body::from(""))
                .unwrap()));
        mock_http_client.expect_head().returning(|_| Ok(Response::builder()
            .status(200)
            .header("x-custom", "Final destination")
            .body(Body::from(""))
            .unwrap()));
        let page_request = PageRequest::new("https://example.com".into(), None, Arc::new(Mutex::new(mock_task_context)));

        // when: fetch is invoked
        let result = command.fetch_header(Arc::new(Mutex::new(page_request)), Box::new(mock_http_client), None).await;

        // then: simple response is returned, with maximum_redirects redirects
        assert_eq!(result.is_ok(), true, "Expecting a simple Response");
        let result_unwrapped = result.unwrap();
        assert_eq!(result_unwrapped.redirects.len(), 2, "Should have two redirects");
        assert_eq!(result_unwrapped.headers.get("x-custom").unwrap(), &String::from("Final destination"), "Should have headers embedded");
        assert_eq!(result_unwrapped.redirects[0].source, String::from("https://example.com"), "Source should match");
        assert_eq!(result_unwrapped.redirects[0].destination, String::from("https://first-redirect.example.com/"), "Destination should match");
        assert_eq!(result_unwrapped.redirects[0].headers.get("location").unwrap(), &String::from("https://first-redirect.example.com/"), "Should have headers embedded");
        assert_eq!(result_unwrapped.redirects[1].source, String::from("https://first-redirect.example.com/"), "Source should match");
        assert_eq!(result_unwrapped.redirects[1].destination, String::from("https://second-redirect.example.com/"), "Destination should match");
        assert_eq!(result_unwrapped.redirects[1].headers.get("x-custom").unwrap(), &String::from("Hello World"), "Should have headers embedded");
    }

    // todo: test with ignore redirect
}