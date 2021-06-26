use std::str::FromStr;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use hyper::{Body, Response, StatusCode, Uri};
use log::{debug, info, trace};
use mockall::automock;

use crate::page_request::PageRequest;

#[async_trait]
pub trait FetchHeaderCommand: Sync + Send {
    async fn fetch_header(&self, page_request: Arc<Mutex<PageRequest>>, http_client: Box<dyn HttpClient>, redirects: Option<Vec<Redirect>>) -> Result<FetchHeaderResponse, String>;
}

struct DefaultFetchHeaderCommand {}

#[async_trait]
impl FetchHeaderCommand for DefaultFetchHeaderCommand {
    async fn fetch_header(&self, page_request: Arc<Mutex<PageRequest>>, http_client: Box<dyn HttpClient>, redirects: Option<Vec<Redirect>>) -> Result<FetchHeaderResponse, String> {
        let uri = page_request.lock().unwrap().url.clone();
        let maximum_redirects = page_request.lock().unwrap().task_context.lock().unwrap().get_config().lock().unwrap().maximum_redirects;
        // let https = HttpsConnector::new();
        // let client = Client::builder().build::<_, hyper::Body>(https);
        //
        // let req = Request::builder()
        //     .method("HEAD")
        //     .uri(uri.clone())
        //     .body(Body::from(""))
        //     .expect("HEAD request builder");
        //
        // if ignore_redirects {
        //     Ok((uri, client.request(req).await.unwrap()))
        // } else {
        let num_redirects = if redirects.is_some() { redirects.unwrap().len() as u16 } else { 0 };

        let response = http_client.head(uri).await.unwrap();
        trace!("HEAD for {}: {:?}", uri, response.headers());
        if num_redirects < maximum_redirects && response.status().is_redirection() {
            if let Some(location_header) = response.headers().get("location") {
                let uri_service = page_request.lock().unwrap().task_context.lock().unwrap().get_uri_service();
                let uri_object = Uri::from_str(&url);
                let adjusted_uri = uri_service.form_full_url(uri_object.scheme_str().unwrap(), location_header.to_str().unwrap(), uri_object.host().unwrap(), parent_uri);
                debug!("Following redirect {}", adjusted_uri);
                let response = fetch_head(adjusted_uri, ignore_redirects, current_redirect + 1, maximum_redirects, parent_uri, uri_service.clone()).await;
                return response;
            }
            info!("No valid location found in redirect header {:?}", response);
        }
        // Ok((uri, response))
        // }

        let result = FetchHeaderResponse { redirects: vec![] };
        Ok(result)
    }
}

pub struct Redirect {
    source: String,
    destination: String,
    http_response_code: StatusCode,
}

pub struct FetchHeaderResponse {
    pub redirects: Vec<Redirect>,
}

impl FetchHeaderResponse {
    pub fn new() -> FetchHeaderResponse {
        FetchHeaderResponse {
            redirects: vec![],
        }
    }
}

#[automock]
#[async_trait]
pub trait HttpClient: Sync + Send {
    async fn head(&self, uri: String) -> Result<Response<Body>, String>;
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

    use crate::task_context::robots_service::RobotsTxt;
    use crate::task_context::task_context::{DefaultTaskContext, FullTaskContext, KnownLinks, TaskConfig, TaskContext, TaskContextInit};

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

    #[tokio::test]
    async fn returns_simple_result_on_simple_request_without_redirect_following() {
        // given: simple fetch command
        let command = DefaultFetchHeaderCommand {};
        let mock_task_context = MockMyTaskContext::new();
        let page_request = PageRequest::new("https://example.com".into(), None, Arc::new(Mutex::new(mock_task_context)));

        // when: fetch is invoked
        let result = command.fetch_header(Arc::new(Mutex::new(page_request)), Box::new(MockHttpClient::new())).await;

        // then: simple response is returned, with no redirects
        assert_eq!(result.is_ok(), true, "Expecting a simple Response");
        assert_eq!(result.unwrap().redirects.len(), 0, "Should not have any redirects");
    }

    #[tokio::test]
    async fn should_return_redirect_list_up_to_max_redirects() {
        // given: simple fetch command
        let command = DefaultFetchHeaderCommand {};
        let mut mock_task_context = MockMyTaskContext::new();
        let mut task_config = TaskConfig::new("https://example.com".into());
        task_config.maximum_redirects = 2;
        mock_task_context.expect_get_config().return_const(Arc::new(Mutex::new(task_config)));
        let mut mock_http_client = MockHttpClient::new();
        let mut sequence = Sequence::new();
        mock_http_client.expect_head()
            .with(eq(String::from("https://example.com")))
            .in_sequence(&mut sequence)
            .returning(|_| Ok(Response::builder()
                .status(308)
                .header("location", "https://first-redirect.example.com")
                .body(Body::from(""))
                .unwrap()));
        mock_http_client.expect_head()
            .with(eq(String::from("https://first-redirect.example.com")))
            .in_sequence(&mut sequence)
            .returning(|_| Ok(Response::builder()
                .status(308)
                .header("location", "https://second-redirect.example.com")
                .body(Body::from(""))
                .unwrap()));
        let page_request = PageRequest::new("https://example.com".into(), None, Arc::new(Mutex::new(mock_task_context)));

        // when: fetch is invoked
        let result = command.fetch_header(Arc::new(Mutex::new(page_request)), Box::new(mock_http_client)).await;

        // then: simple response is returned, with maximum_redirects redirects
        assert_eq!(result.is_ok(), true, "Expecting a simple Response");
        let result_unwrapped = result.unwrap();
        assert_eq!(result_unwrapped.redirects.len(), 2, "Should have two redirects");
        assert_eq!(result_unwrapped.redirects[0].source, String::from("https://example.com"), "Should have two redirects");
        assert_eq!(result_unwrapped.redirects[0].destination, String::from("https://first-redirect.example.com"), "Should have two redirects");
        assert_eq!(result_unwrapped.redirects[1].source, String::from("https://first-redirect.example.com"), "Should have two redirects");
        assert_eq!(result_unwrapped.redirects[1].destination, String::from("https://second-redirect.example.com"), "Should have two redirects");
    }

    // todo: test with ignore redirect
}