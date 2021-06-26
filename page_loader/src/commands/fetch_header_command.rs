use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use hyper::{StatusCode, Uri};

use crate::page_request::PageRequest;

#[async_trait]
pub trait FetchHeaderCommand: Sync + Send {
    async fn fetch_header(&self, page_request: Arc<Mutex<PageRequest>>) -> Result<FetchHeaderResponse, String>;
}

struct DefaultFetchHeaderCommand {}

#[async_trait]
impl FetchHeaderCommand for DefaultFetchHeaderCommand {
    async fn fetch_header(&self, page_request: Arc<Mutex<PageRequest>>) -> Result<FetchHeaderResponse, String> {
        let result = FetchHeaderResponse { redirects: vec![] };
        Ok(result)
    }
}

pub struct Redirect {
    source: Uri,
    destination: Uri,
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

#[cfg(test)]
mod tests {
    use std::fmt::{Debug, Formatter, Result};
    use std::time::Duration;

    use mockall::*;
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
        let result = command.fetch_header(Arc::new(Mutex::new(page_request))).await;

        // then: simple response is returned, with no redirects
        assert_eq!(result.is_ok(), true, "Expecting a simple Response");
        assert_eq!(result.unwrap().redirects.len(), 0, "Should not have any redirects");
    }
}