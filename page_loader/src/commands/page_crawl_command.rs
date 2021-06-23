use std::sync::Arc;

use async_trait::async_trait;

use crate::page_request::PageRequest;
use crate::page_response::PageResponse;
use crate::task_context::task_context::FullTaskContext;

#[async_trait]
pub trait CrawlCommand: Sync + Send {
    fn get_url_clone(&self) -> String;
    async fn crawl(&self) -> Result<Option<PageResponse>, String>;
    fn get_task_context(&self) -> Arc<dyn FullTaskContext>;
    fn get_current_depth(&self) -> u16;
}

#[derive(Clone, Debug)]
pub struct PageCrawlCommand {
    pub request_object: PageRequest,
    pub current_depth: u16,
}

impl PageCrawlCommand {
    pub fn new(url: String, task_context: Arc<dyn FullTaskContext>, current_depth: u16) -> PageCrawlCommand {
        PageCrawlCommand { request_object: PageRequest::new(url, None, task_context), current_depth }
    }

    fn verify_crawlability(&self) -> bool {
        let config = self.request_object.task_context.get_config().clone();
        let config_locked = config.lock().unwrap();
        if config_locked.maximum_depth > 0 &&
            self.current_depth > config_locked.maximum_depth {
            return false;
        }
        // at this point, the config isn't required anymore and can therefore be dropped
        drop(config_locked);
        drop(config);

        if self.request_object.task_context.get_all_known_links().lock().unwrap().contains(&self.request_object.url) {
            return false;
        }

        if !self.request_object.task_context.can_access(&self.request_object.url) {
            return false;
        }

        true
    }
}

#[async_trait]
impl CrawlCommand for PageCrawlCommand {
    fn get_url_clone(&self) -> String { self.request_object.url.clone() }

    async fn crawl(&self) -> Result<Option<PageResponse>, String> {
        if !self.verify_crawlability() {
            return Ok(None);
        }

        Ok(Some(PageResponse::new(self.request_object.url.clone())))
    }

    fn get_task_context(&self) -> Arc<dyn FullTaskContext> {
        self.request_object.task_context.clone()
    }

    fn get_current_depth(&self) -> u16 { self.current_depth }
}

#[cfg(test)]
mod tests {
    use std::fmt::{Debug, Formatter, Result};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use hyper::Uri;
    use mockall::*;
    use tokio::time::Instant;
    use uuid::Uuid;

    use crate::commands::page_crawl_command::{CrawlCommand, PageCrawlCommand};
    use crate::task_context::robots_service::RobotsTxt;
    use crate::task_context::task_context::{DefaultTaskContext, KnownLinks, TaskConfig, TaskContext, TaskContextInit};

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

        // when: invoked with a current_depth > 0 && > maximum_depth
        let page_crawl_command = PageCrawlCommand::new(String::from("https://example.com"), Arc::new(mock_task_context), 2);
        let crawl_result = page_crawl_command.crawl().await;

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

        // when: invoked with a current_depth > 0
        let page_crawl_command = PageCrawlCommand::new(String::from("https://example.com"), Arc::new(mock_task_context), 9000);
        let crawl_result = page_crawl_command.crawl().await;

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

        // when: invoked with a known link
        let page_crawl_command = PageCrawlCommand::new(url.clone(), Arc::new(mock_task_context), 1);
        let crawl_result = page_crawl_command.crawl().await;

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

        // when: invoked with a known link
        let page_crawl_command = PageCrawlCommand::new(String::from("https://example.com"), Arc::new(mock_task_context), 1);
        let crawl_result = page_crawl_command.crawl().await;

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

        // when: invoked with a restricted link
        let page_crawl_command = PageCrawlCommand::new(String::from("https://example.com"), Arc::new(mock_task_context), 1);
        let crawl_result = page_crawl_command.crawl().await;

        // then: expect none
        assert_eq!(crawl_result.unwrap().is_none(), true, "Should not crawl urls forbidden by robots.txt")
    }
}