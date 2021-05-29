use crate::page_request::PageRequest;
use crate::page_response::PageResponse;
use crate::task_context::TaskContext;
use std::sync::Arc;

pub trait CrawlCommand: Send {
    fn get_url_clone(&self) -> String;
    fn crawl(&self) -> PageResponse;
    fn get_task_context(&self) -> Arc<dyn TaskContext>;
}

#[derive(Clone, Debug)]
pub struct PageCrawlCommand {
    pub request_object: PageRequest,
}

impl PageCrawlCommand {
    pub fn new(url: String, task_context: Arc<dyn TaskContext>) -> PageCrawlCommand {
        PageCrawlCommand { request_object: PageRequest::new(url, None, task_context) }
    }
}

impl CrawlCommand for PageCrawlCommand {
    fn get_url_clone(&self) -> String { self.request_object.url.clone() }

    fn crawl(&self) -> PageResponse {
        PageResponse::new(self.request_object.url.clone())
    }

    fn get_task_context(&self) -> Arc<dyn TaskContext> {
        self.request_object.task_context.clone()
    }
}