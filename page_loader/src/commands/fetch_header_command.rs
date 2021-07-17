use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hyper::{Body, Response, StatusCode, Uri};
use hyper::header::HeaderValue;
use log::{debug, info, trace};

use crate::http::http_client::HttpClient;
use crate::http::http_utils;
use crate::page_request::PageRequest;
use crate::response_timings::ResponseTimings;

#[async_trait]
pub trait FetchHeaderCommand: Sync + Send {
    async fn fetch_header(&self, page_request: Arc<Mutex<PageRequest>>, http_client: Arc<dyn HttpClient>, redirects: Option<Vec<Redirect>>) -> Result<(FetchHeaderResponse, Arc<dyn HttpClient>), String>;
}

pub struct DefaultFetchHeaderCommand {}

#[async_trait]
impl FetchHeaderCommand for DefaultFetchHeaderCommand {
    async fn fetch_header(&self, page_request: Arc<Mutex<PageRequest>>, http_client: Arc<dyn HttpClient>, redirects: Option<Vec<Redirect>>) -> Result<(FetchHeaderResponse, Arc<dyn HttpClient>), String> {
        let start_time = DateTime::from(Utc::now());
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
        let headers: HashMap<String, String> = http_utils::response_headers_to_map(&response);
        if num_redirects < maximum_redirects && response.status().is_redirection() {
            if let Some(location_header) = response.headers().get("location") {
                let redirects_for_next = DefaultFetchHeaderCommand::append_redirect(&page_request, redirects, uri, &response, &headers, location_header, start_time);
                let response = self.fetch_header(page_request.clone(), http_client, Some(redirects_for_next)).await;
                return response;
            }
            let error_message = format!("No valid location found in redirect header {:?}", response);
            info!("{}", &error_message);
        }

        let redirects_result = redirects.unwrap_or(vec![]);
        let result = FetchHeaderResponse {
            redirects: redirects_result,
            http_response_code: response.status(),
            headers,
            requested_url: uri.clone(),
            response_timings: ResponseTimings::from(format!("FetchHeaderResponse.{}", uri.clone()), start_time, DateTime::from(Utc::now())),
        };
        Ok((result, http_client))
    }
}

impl DefaultFetchHeaderCommand {
    fn append_redirect(page_request: &Arc<Mutex<PageRequest>>, redirects: Option<Vec<Redirect>>, uri: String, response: &Response<Body>, headers: &HashMap<String, String>, location_header: &HeaderValue, redirect_start_time: DateTime<Utc>) -> Vec<Redirect> {
        let uri_service = page_request.lock().unwrap().task_context.lock().unwrap().get_uri_service();
        let uri_object = Uri::from_str(&uri).unwrap();
        let adjusted_uri = uri_service.form_full_url(uri_object.scheme_str().unwrap(), location_header.to_str().unwrap(), uri_object.host().unwrap(), &Some(uri.clone()));
        let redirect = Redirect {
            source: uri.clone(),
            destination: adjusted_uri.to_string(),
            http_response_code: response.status(),
            headers: headers.clone(),
            response_timings: ResponseTimings::from(format!("Redirect.{}", uri.clone()), redirect_start_time, DateTime::from(Utc::now())),
        };
        debug!("Following redirect {}", adjusted_uri);
        let mut redirects_for_next = vec![];
        if redirects.is_some() {
            redirects_for_next.append(&mut redirects.unwrap());
        }
        redirects_for_next.push(redirect);
        redirects_for_next
    }
}

#[derive(Debug, Clone)]
pub struct Redirect {
    source: String,
    destination: String,
    http_response_code: StatusCode,
    headers: HashMap<String, String>,
    response_timings: ResponseTimings,
}

#[cfg(test)]
impl Redirect {
    pub fn from(source: String, destination: String) -> Redirect {
        Redirect {
            source: source.clone(),
            destination,
            http_response_code: StatusCode::OK,
            headers: HashMap::new(),
            response_timings: ResponseTimings::new(format!("Redirects.{}", source)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FetchHeaderResponse {
    pub requested_url: String,
    pub redirects: Vec<Redirect>,
    pub http_response_code: StatusCode,
    pub headers: HashMap<String, String>,
    pub response_timings: ResponseTimings,
}

impl FetchHeaderResponse {
    pub fn new(requested_url: String, http_response_code: StatusCode) -> FetchHeaderResponse {
        FetchHeaderResponse {
            requested_url: requested_url.clone(),
            redirects: vec![],
            http_response_code,
            headers: HashMap::new(),
            response_timings: ResponseTimings::new(format!("FetchHeaderResponse.{}", requested_url.clone())),
        }
    }

    pub fn get_final_uri(&self) -> String {
        if self.redirects.is_empty() {
            return self.requested_url.clone();
        }

        self.redirects.last().unwrap().destination.clone()
    }
}

#[cfg(test)]
mod tests {
    use mockall::*;
    use mockall::predicate::eq;
    use tokio::time::Instant;
    use uuid::Uuid;

    use dom_parser::DomParser;
    use linkresult::LinkTypeChecker;
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
            fn get_dom_parser(&self) ->Arc<dyn DomParser>;
            fn get_http_client(&self) -> Arc<dyn HttpClient>;
        }
        impl KnownLinks for MyTaskContext{
            fn get_all_known_links(&self) -> Arc<Mutex<Vec<String>>>;
            fn add_known_link(&self, link: String);
        }
        impl RobotsTxt for MyTaskContext{
            fn can_access(&self, item_uri: &str) -> bool;
        }
        impl FullTaskContext for MyTaskContext{}
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
        let mock_http_client = Arc::new(mock_http_client);

        // when: fetch is invoked
        let result = command.fetch_header(
            Arc::new(Mutex::new(page_request)),
            mock_http_client,
            None,
        ).await;

        // then: simple response is returned, with no redirects
        assert_eq!(result.is_ok(), true, "Expecting a simple Response");
        assert_eq!(result.as_ref().unwrap().0.redirects.len(), 0, "Should not have any redirects");
        assert_eq!(result.as_ref().unwrap().0.response_timings.end_time.is_some(), true, "Should have updated end_time after successful run");
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
        let mock_http_client = Arc::new(mock_http_client);
        let page_request = PageRequest::new("https://example.com".into(), None, Arc::new(Mutex::new(mock_task_context)));

        // when: fetch is invoked
        let result = command.fetch_header(Arc::new(Mutex::new(page_request)), mock_http_client, None).await;

        // then: simple response is returned, with maximum_redirects redirects
        assert_eq!(result.is_ok(), true, "Expecting a simple Response");
        let result_unwrapped = result.unwrap().0;
        assert_eq!(result_unwrapped.redirects.len(), 2, "Should have two redirects");
        assert_eq!(result_unwrapped.headers.get("x-custom").unwrap(), &String::from("Final destination"), "Should have headers embedded");
        assert_eq!(result_unwrapped.response_timings.end_time.is_some(), true, "Should have updated end_time after successful run");

        assert_eq!(result_unwrapped.redirects[0].source, String::from("https://example.com"), "Source should match");
        assert_eq!(result_unwrapped.redirects[0].destination, String::from("https://first-redirect.example.com/"), "Destination should match");
        assert_eq!(result_unwrapped.redirects[0].headers.get("location").unwrap(), &String::from("https://first-redirect.example.com/"), "Should have headers embedded");
        assert_eq!(result_unwrapped.redirects[0].response_timings.end_time.is_some(), true, "Should have updated end_time after successful run - redirect[0]");
        assert_eq!(result_unwrapped.redirects[1].source, String::from("https://first-redirect.example.com/"), "Source should match");
        assert_eq!(result_unwrapped.redirects[1].destination, String::from("https://second-redirect.example.com/"), "Destination should match");
        assert_eq!(result_unwrapped.redirects[1].headers.get("x-custom").unwrap(), &String::from("Hello World"), "Should have headers embedded");
        assert_eq!(result_unwrapped.redirects[1].response_timings.end_time.is_some(), true, "Should have updated end_time after successful run - redirect[1]");
    }

    // todo: test with ignore redirect
}
