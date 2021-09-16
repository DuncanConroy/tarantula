use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use log::trace;

use responses::get_response::GetResponse;
use responses::response_timings::ResponseTimings;
use responses::status_code::StatusCode;

use crate::http::http_client::HttpClient;
use crate::http::http_utils;

#[async_trait]
pub trait PageDownloadCommand: Sync + Send {
    async fn download_page(&self, uri: String, http_client: Arc<dyn HttpClient>) -> Result<GetResponse, String>;
}

pub struct DefaultPageDownloadCommand {}

#[async_trait]
impl PageDownloadCommand for DefaultPageDownloadCommand {
    async fn download_page(&self, uri: String, http_client: Arc<dyn HttpClient>) -> Result<GetResponse, String> {
        let start_time = DateTime::from(Utc::now());

        let response = http_client.get(uri.clone()).await.unwrap();
        trace!("GET for {}: {:?}", uri, response.headers());
        let headers: HashMap<String, String> = http_utils::response_headers_to_map(&response);
        let http_response_code = http_utils::map_status_code(response.status());
        let body: String = String::from_utf8_lossy(hyper::body::to_bytes(response.into_body()).await.unwrap().as_ref())
            .to_string();
        let result = GetResponse {
            http_response_code,
            headers,
            requested_url: uri.clone(),
            response_timings: ResponseTimings::from(uri.clone(), start_time, DateTime::from(Utc::now())),
            body: Some(body),
        };
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use hyper::{Body, Response};
    use mockall::*;
    use tokio::sync::mpsc::Sender;
    use tokio::time::Instant;
    use uuid::Uuid;

    use dom_parser::DomParser;
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
            fn get_response_channel(&self) -> Sender<CrawlerEvent>;
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
    async fn returns_simple_result_on_simple_request() {
        // given: simple download command
        let command = DefaultPageDownloadCommand {};
        let mut mock_task_context = MockMyTaskContext::new();
        let task_config = TaskConfig::new(RunConfig::new("https://example.com".into(), None));
        mock_task_context.expect_get_config().return_const(Arc::new(Mutex::new(task_config)));
        let mut mock_http_client = MockMyHttpClient::new();
        mock_http_client.expect_get().returning(|_| Ok(Response::builder()
            .status(200)
            .body(Body::from("Hello World"))
            .unwrap()));
        let mock_http_client = Arc::new(mock_http_client);

        // when: fetch is invoked
        let result = command.download_page("https://example.com".into(), mock_http_client).await;

        // then: simple response is returned, with no redirects
        assert_eq!(result.is_ok(), true, "Expecting a simple Response");
        assert_eq!(result.as_ref().unwrap().body.is_some(), true, "Should have body");
        assert_eq!(result.as_ref().unwrap().body.as_ref().unwrap(), "Hello World", "Should have body");
        assert_eq!(result.as_ref().unwrap().response_timings.end_time.is_some(), true, "Should have updated end_time after successful run");
    }
}
