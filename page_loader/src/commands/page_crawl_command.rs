use std::io::Error;
use std::sync::Arc;

use async_trait::async_trait;

use crate::page_request::PageRequest;
use crate::page_response::PageResponse;
use crate::task_context::{DefaultTaskContext, FullTaskContext, KnownLinks, TaskContext};

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
}

#[async_trait]
impl CrawlCommand for PageCrawlCommand {
    fn get_url_clone(&self) -> String { self.request_object.url.clone() }

    async fn crawl(&self) -> Result<Option<PageResponse>, String> {
        if self.request_object.task_context.get_config_ref().maximum_depth > 0 &&
            self.current_depth > self.request_object.task_context.get_config_ref().maximum_depth {
            return Ok(None);
        }

        if self.request_object.task_context.get_all_known_links().lock().unwrap().contains(&self.request_object.url) {
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
    use std::sync::Arc;

    use crate::commands::page_crawl_command::{CrawlCommand, PageCrawlCommand};
    use crate::task_context::{DefaultTaskContext, KnownLinks, TaskContext, TaskContextInit};

    #[tokio::test]
    async fn will_not_crawl_if_max_depth_reached() {
        // given: a task context with maximum_depth > 0
        let mut task_context = DefaultTaskContext::init(String::from("https://example.com"));
        task_context.get_config_mut().maximum_depth = 1;

        // when: invoked with a current_depth > 0 && > maximum_depth
        let page_crawl_command = PageCrawlCommand::new(String::from("https://example.com"), Arc::new(task_context), 2);
        let crawl_result = page_crawl_command.crawl().await;

        // then: expect none
        assert_eq!(crawl_result.unwrap().is_none(), true, "Should not crawl, if max depth reached")
    }

    #[tokio::test]
    async fn will_crawl_if_max_depth_is_zero() {
        // given: a task context with maximum_depth = 0
        let mut task_context = DefaultTaskContext::init(String::from("https://example.com"));
        task_context.get_config_mut().maximum_depth = 0;

        // when: invoked with a current_depth > 0
        let page_crawl_command = PageCrawlCommand::new(String::from("https://example.com"), Arc::new(task_context), 9000);
        let crawl_result = page_crawl_command.crawl().await;

        // then: expect some
        assert_eq!(crawl_result.unwrap().is_some(), true, "Should crawl, if max depth not reached, yet")
    }

    #[tokio::test]
    async fn will_not_crawl_if_url_is_known() {
        // given: a task context with a known link
        let mut task_context = DefaultTaskContext::init(String::from("https://example.com"));
        task_context.add_known_link("https://example.com".into());

        // when: invoked with a known link
        let page_crawl_command = PageCrawlCommand::new(String::from("https://example.com"), Arc::new(task_context), 1);
        let crawl_result = page_crawl_command.crawl().await;

        // then: expect none
        assert_eq!(crawl_result.unwrap().is_none(), true, "Should not crawl, if url is known")
    }

    #[tokio::test]
    async fn will_crawl_if_url_is_unknown() {
        // given: a task context without the link known
        let mut task_context = DefaultTaskContext::init(String::from("https://example.com"));

        // when: invoked with a known link
        let page_crawl_command = PageCrawlCommand::new(String::from("https://example.com"), Arc::new(task_context), 1);
        let crawl_result = page_crawl_command.crawl().await;

        // then: expect some
        assert_eq!(crawl_result.unwrap().is_some(), true, "Should crawl, if url is unknown")
    }
}