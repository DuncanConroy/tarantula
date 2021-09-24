use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hyper::{Body, Response, Uri};
use hyper::header::HeaderValue;
use log::{debug, info, trace};

use responses::head_response::HeadResponse;
use responses::redirect::Redirect;
use responses::response_timings::ResponseTimings;
use responses::status_code::StatusCode;

use crate::http::http_client::HttpClient;
use crate::http::http_utils;
use crate::page_request::PageRequest;

#[async_trait]
pub trait FetchHeaderCommand: Sync + Send {
    async fn fetch_header(&self, page_request: Arc<Mutex<PageRequest>>, http_client: Arc<dyn HttpClient>, redirects: Option<Vec<Redirect>>, robots_txt_info_url: Option<String>) -> Result<(HeadResponse, Arc<dyn HttpClient>), String>;
}

pub struct DefaultFetchHeaderCommand {}

#[async_trait]
impl FetchHeaderCommand for DefaultFetchHeaderCommand {
    async fn fetch_header(&self, page_request: Arc<Mutex<PageRequest>>, http_client: Arc<dyn HttpClient>, redirects: Option<Vec<Redirect>>, robots_txt_info_url: Option<String>) -> Result<(HeadResponse, Arc<dyn HttpClient>), String> {
        let start_time = DateTime::from(Utc::now());
        let mut uri = page_request.lock().unwrap().url.clone();
        let maximum_redirects = page_request.lock().unwrap().task_context.lock().unwrap().get_config().lock().unwrap().maximum_redirects;

        let mut num_redirects = 0;
        if redirects.is_some() {
            let redirects_unwrapped = redirects.as_ref().unwrap();
            num_redirects = redirects_unwrapped.len() as u8;
            uri = redirects_unwrapped.last().unwrap().destination.clone();
        }

        let response = http_client.head(uri.clone(), robots_txt_info_url.clone()).await.unwrap();
        trace!("HEAD for {}: {:?}", uri, response.headers());
        let headers: HashMap<String, String> = http_utils::response_headers_to_map(&response);
        if num_redirects < maximum_redirects && response.status().is_redirection() {
            if let Some(location_header) = response.headers().get("location") {
                let redirects_for_next = DefaultFetchHeaderCommand::append_redirect(&page_request, redirects, uri, &response, &headers, location_header, start_time);
                let response = self.fetch_header(page_request.clone(), http_client.clone(), Some(redirects_for_next), robots_txt_info_url.clone()).await;
                return response;
            }
            let error_message = format!("No valid location found in redirect header {:?}", response);
            info!("{}", &error_message);
        }

        let redirects_result = redirects.unwrap_or(vec![]);
        let result = HeadResponse {
            redirects: redirects_result,
            http_response_code: http_utils::map_status_code(response.status()),
            headers,
            requested_url: uri.clone(),
            response_timings: ResponseTimings::from(format!("HeadResponse.{}", uri.clone()), start_time, DateTime::from(Utc::now())),
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
            http_response_code: StatusCode { code: response.status().as_u16(), label: response.status().canonical_reason().unwrap().into() },
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

#[cfg(test)]
mod tests {
    use mockall::*;
    use mockall::predicate::eq;
    use tokio::sync::mpsc::Sender;
    use tokio::time::Instant;
    use uuid::Uuid;

    use dom_parser::DomParser;
    use linkresult::LinkTypeChecker;
    use linkresult::uri_service::UriService;
    use responses::run_config::RunConfig;

    use crate::events::crawler_event::CrawlerEvent;
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
            fn get_response_channel(&self) -> &Sender<CrawlerEvent>;
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
            async fn head(&self, uri: String, robots_txt_info_url: Option<String>) -> hyper::Result<Response<Body>>;
            async fn get(&self, uri: String, robots_txt_info_url: Option<String>) -> hyper::Result<Response<Body>>;
        }
    }

    #[tokio::test]
    async fn returns_simple_result_on_simple_request_without_redirect_following() {
        // given: simple fetch command
        let command = DefaultFetchHeaderCommand {};
        let mut mock_task_context = MockMyTaskContext::new();
        let task_config = TaskConfig::new(RunConfig::new("https://example.com".into(), None));
        mock_task_context.expect_get_config().return_const(Arc::new(Mutex::new(task_config)));
        let page_request = PageRequest::new("https://example.com".into(), "/".into(), None, Arc::new(Mutex::new(mock_task_context)));
        let mut mock_http_client = MockMyHttpClient::new();
        mock_http_client.expect_head().returning(|_, _| Ok(Response::builder()
            .status(200)
            .body(Body::from(""))
            .unwrap()));
        let mock_http_client = Arc::new(mock_http_client);

        // when: fetch is invoked
        let result = command.fetch_header(
            Arc::new(Mutex::new(page_request)),
            mock_http_client,
            None,
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
        let mut task_config = TaskConfig::new(RunConfig::new("https://example.com".into(), None));
        task_config.maximum_redirects = 2;
        mock_task_context.expect_get_config().return_const(Arc::new(Mutex::new(task_config)));
        let mut mock_http_client = MockMyHttpClient::new();
        let mut sequence = Sequence::new();
        mock_http_client.expect_head()
            .with(eq(String::from("https://example.com")), eq(None))
            .times(1)
            .in_sequence(&mut sequence)
            .returning(|_, _x: Option<String>| Ok(Response::builder()
                .status(308)
                .header("location", "https://first-redirect.example.com/")
                .body(Body::from(""))
                .unwrap()));
        mock_http_client.expect_head()
            .with(eq(String::from("https://first-redirect.example.com/")), eq(None))
            .times(1)
            .in_sequence(&mut sequence)
            .returning(|_, _x: Option<String>| Ok(Response::builder()
                .status(308)
                .header("location", "https://second-redirect.example.com")
                .header("x-custom", "Hello World")
                .body(Body::from(""))
                .unwrap()));
        mock_http_client.expect_head().returning(|_, _| Ok(Response::builder()
            .status(200)
            .header("x-custom", "Final destination")
            .body(Body::from(""))
            .unwrap()));
        let mock_http_client = Arc::new(mock_http_client);
        let page_request = PageRequest::new("https://example.com".into(), "/".into(), None, Arc::new(Mutex::new(mock_task_context)));

        // when: fetch is invoked
        let result = command.fetch_header(Arc::new(Mutex::new(page_request)), mock_http_client, None, None).await;

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
