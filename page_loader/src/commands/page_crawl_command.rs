use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::commands::fetch_header_command::{DefaultFetchHeaderCommand, FetchHeaderCommand};
use crate::http::http_client::{HttpClient, HttpClientImpl};
use crate::page_request::PageRequest;
use crate::page_response::PageResponse;
use crate::task_context::task_context::FullTaskContext;

#[async_trait]
pub trait CrawlCommand: Sync + Send {
    fn get_url_clone(&self) -> String;
    async fn crawl(&self, http_client: Box<dyn HttpClient>) -> Result<Option<PageResponse>, String>;
    fn get_task_context(&self) -> Arc<Mutex<dyn FullTaskContext>>;
    fn get_current_depth(&self) -> u16;
}

pub struct PageCrawlCommand {
    pub request_object: Arc<Mutex<PageRequest>>,
    pub current_depth: u16,
    fetch_header_command: Box<dyn FetchHeaderCommand>,
}

impl PageCrawlCommand {
    pub fn new(url: String, task_context: Arc<Mutex<dyn FullTaskContext>>, current_depth: u16, fetch_header_command: Box<dyn FetchHeaderCommand>) -> PageCrawlCommand {
        PageCrawlCommand {
            request_object: Arc::new(Mutex::new(PageRequest::new(url, None, task_context))),
            current_depth,
            fetch_header_command,
        }
    }

    fn verify_crawlability(&self) -> bool {
        let request_object_locked = self.request_object.lock().unwrap();
        let config = request_object_locked.task_context.lock().unwrap().get_config().clone();
        let config_locked = config.lock().unwrap();
        if config_locked.maximum_depth > 0 &&
            self.current_depth > config_locked.maximum_depth {
            return false;
        }
        // at this point, the config isn't required anymore and can therefore be dropped
        drop(config_locked);
        drop(config);

        let task_context_locked = request_object_locked.task_context.lock().unwrap();
        if task_context_locked.get_all_known_links().lock().unwrap().contains(&request_object_locked.url) {
            return false;
        }

        if !task_context_locked.can_access(&request_object_locked.url) {
            return false;
        }

        true
    }

    async fn perform_crawl_internal(&self, http_client: Box<dyn HttpClient>) -> Result<Option<PageResponse>, String> {
        /// OLD /////
        // let mut page_response = PageResponse::new(self.request_object.url.clone());

        let fetch_header_response = self.fetch_header_command.fetch_header(self.request_object.clone(), http_client, None).await;
        let mut page_response = PageResponse::new(self.request_object.lock().unwrap().url.clone());
        page_response.status_code = Some(fetch_header_response.unwrap().http_response_code.as_u16());

        // todo!("TDD approach to retrieve head, redirect, final content, parse and return found links");
        // work with dynamic filtering and mapping classes, like spring routing, etc.


        Ok(Some(page_response))
    }
}

#[async_trait]
impl CrawlCommand for PageCrawlCommand {
    fn get_url_clone(&self) -> String { self.request_object.lock().unwrap().url.clone() }

    async fn crawl(&self, http_client: Box<dyn HttpClient>) -> Result<Option<PageResponse>, String> {
        if !self.verify_crawlability() {
            return Ok(None);
        }

        self.perform_crawl_internal(http_client).await
    }

    fn get_task_context(&self) -> Arc<Mutex<dyn FullTaskContext>> {
        self.request_object.lock().unwrap().task_context.clone()
    }

    fn get_current_depth(&self) -> u16 { self.current_depth }
}

#[cfg(test)]
mod tests {
    use std::fmt::{Debug, Formatter, Result};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use hyper::{Body, Response, StatusCode, Uri};
    use mockall::*;
    use tokio::time::Instant;
    use uuid::Uuid;

    use linkresult::uri_service::UriService;

    use crate::commands::fetch_header_command::{FetchHeaderResponse, Redirect};
    use crate::commands::page_crawl_command::{CrawlCommand, PageCrawlCommand};
    use crate::task_context::robots_service::RobotsTxt;
    use crate::task_context::task_context::{DefaultTaskContext, KnownLinks, TaskConfig, TaskContext, TaskContextInit, TaskContextServices};

    use super::*;
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
            fn fmt<'a>(&self, f: &mut Formatter<'a>) -> std::fmt::Result;
        }
    }
    mock! {
        #[async_trait]
        MyHttpClient {}
        #[async_trait]
        impl HttpClient for MyHttpClient{
            async fn head(&self, uri: String) -> std::result::Result<Response<Body>, String>;
        }
    }
    mock! {
        #[async_trait]
        MyFetchHeaderCommand {}
        #[async_trait]
        impl FetchHeaderCommand for MyFetchHeaderCommand{
            async fn fetch_header(&self, page_request: Arc<Mutex<PageRequest>>, http_client: Box<dyn HttpClient>, redirects: Option<Vec<Redirect>>) -> std::result::Result<FetchHeaderResponse, String>;
        }
    }

    fn get_default_task_config() -> Arc<Mutex<TaskConfig>> {
        Arc::new(Mutex::new(TaskConfig {
            uri: Default::default(),
            ignore_redirects: false,
            maximum_redirects: 0,
            maximum_depth: 16,
            ignore_robots_txt: false,
            keep_html_in_memory: false,
            user_agent: "".to_string(),
        }))
    }

    #[tokio::test]
    async fn will_not_crawl_if_max_depth_reached() {
        // given: a task context with maximum_depth > 0
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        let url_clone = url.clone();
        mock_task_context.expect_get_url().return_const(url_clone);
        let config = get_default_task_config();
        config.lock().unwrap().maximum_depth = 1;
        mock_task_context.expect_get_config().return_const(config.clone());
        let mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());

        // when: invoked with a current_depth > 0 && > maximum_depth
        let mut page_crawl_command = PageCrawlCommand::new(
            String::from("https://example.com"),
            Arc::new(Mutex::new(mock_task_context)),
            2,
            mock_fetch_header_command);
        let mock_http_client = Box::new(MockMyHttpClient::new());
        let crawl_result = page_crawl_command.crawl(mock_http_client).await;

        // then: expect none
        assert_eq!(crawl_result.unwrap().is_none(), true, "Should not crawl, if max depth reached")
    }

    #[tokio::test]
    async fn will_crawl_if_max_depth_is_zero() {
        // given: a task context with maximum_depth = 0
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());
        mock_task_context.expect_get_all_known_links().return_const(Arc::new(Mutex::new(vec![])));
        let config = get_default_task_config();
        config.lock().unwrap().maximum_depth = 0;
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_can_access().returning(|_| true);
        let mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        let mut mock_http_client = Box::new(MockMyHttpClient::new());
        mock_http_client.expect_head().returning(|_| Ok(Response::builder()
            .status(200)
            .body(Body::from(""))
            .unwrap()));

        // when: invoked with a current_depth > 0
        let page_crawl_command = PageCrawlCommand::new(
            String::from("https://example.com"),
            Arc::new(Mutex::new(mock_task_context)),
            9000,
            mock_fetch_header_command);
        let crawl_result = page_crawl_command.crawl(mock_http_client).await;

        // then: expect some
        assert_eq!(crawl_result.unwrap().is_some(), true, "Should crawl, if max depth not reached, yet")
    }

    #[tokio::test]
    async fn will_not_crawl_if_url_is_known() {
        // given: a task context with a known link
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_known_links().return_const(Arc::new(Mutex::new(vec![url.clone()])));
        let mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());

        // when: invoked with a known link
        let page_crawl_command = PageCrawlCommand::new(
            url.clone(),
            Arc::new(Mutex::new(mock_task_context)),
            1,
            mock_fetch_header_command);
        let mock_http_client = Box::new(MockMyHttpClient::new());
        let crawl_result = page_crawl_command.crawl(mock_http_client).await;

        // then: expect none
        assert_eq!(crawl_result.unwrap().is_none(), true, "Should not crawl, if url is known")
    }

    #[tokio::test]
    async fn will_crawl_if_url_is_unknown() {
        // given: a task context without the link known
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_known_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| true);
        let mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        let mut mock_http_client = Box::new(MockMyHttpClient::new());
        mock_http_client.expect_head().returning(|_| Ok(Response::builder()
            .status(200)
            .body(Body::from(""))
            .unwrap()));

        // when: invoked with a known link
        let page_crawl_command = PageCrawlCommand::new(
            String::from("https://example.com"),
            Arc::new(Mutex::new(mock_task_context)),
            1,
            mock_fetch_header_command);
        let crawl_result = page_crawl_command.crawl(mock_http_client).await;

        // then: expect some
        assert_eq!(crawl_result.unwrap().is_some(), true, "Should crawl, if url is unknown")
    }

    #[tokio::test]
    async fn will_not_crawl_if_url_is_forbidden_by_robots_txt() {
        // given: a task context with robots_txt disallowing crawling
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_known_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| false);
        let mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());

        // when: invoked with a restricted link
        let page_crawl_command = PageCrawlCommand::new(
            String::from("https://example.com"),
            Arc::new(Mutex::new(mock_task_context)),
            1,
            mock_fetch_header_command,
        );
        let mock_http_client = Box::new(MockMyHttpClient::new());
        let crawl_result = page_crawl_command.crawl(mock_http_client).await;

        // then: expect none
        assert_eq!(crawl_result.unwrap().is_none(), true, "Should not crawl urls forbidden by robots.txt")
    }

    #[tokio::test]
    async fn returns_proper_page_response_on_successful_crawl() {
        // given: a task context with robots_txt disallowing crawling
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_known_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| true);
        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|a, b, c| Ok(FetchHeaderResponse::new(StatusCode::IM_A_TEAPOT)));

        // when: invoked with a restricted link
        let page_crawl_command = PageCrawlCommand::new(
            String::from("https://example.com"),
            Arc::new(Mutex::new(mock_task_context)),
            1,
            mock_fetch_header_command,
        );
        let mock_http_client = Box::new(MockMyHttpClient::new());
        let crawl_result = page_crawl_command.crawl(mock_http_client).await;

        // then: expect some PageResponse with Teapot status code
        assert_eq!(crawl_result.as_ref().unwrap().is_some(), true, "Should not crawl urls forbidden by robots.txt");
        assert_eq!(crawl_result.unwrap().unwrap().status_code.unwrap(), StatusCode::IM_A_TEAPOT);

        // TODO: add more tests, once PageResponse structure is clear
    }
}