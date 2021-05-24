use crate::page_request::PageRequest;
use crate::page_response::PageResponse;

pub trait CrawlCommand: Send {
    fn get_url_clone(&self) -> String;
    fn crawl(&self) -> PageResponse;
}

#[derive(Clone, Debug)]
pub struct PageCrawlCommand {
    pub request_object: PageRequest,
}

impl PageCrawlCommand {
    pub fn new(url: String) -> PageCrawlCommand {
        PageCrawlCommand { request_object: PageRequest::new(url, None) }
    }
}

impl CrawlCommand for PageCrawlCommand {
    fn get_url_clone(&self) -> String { self.request_object.url.clone() }

    fn crawl(&self) -> PageResponse {
        PageResponse::new(self.request_object.url.clone())
    }
}