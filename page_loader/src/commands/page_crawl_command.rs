use crate::page_request::PageRequest;
use crate::page_response::PageResponse;

use mockall::automock;

pub struct PageCrawlCommand {
    pub request_object: PageRequest,
}

#[automock]
impl PageCrawlCommand {
    pub fn new(url: String) -> PageCrawlCommand {
        PageCrawlCommand { request_object: PageRequest::new(url, None) }
    }

    pub fn crawl(&self)->PageResponse{
        PageResponse::new(self.request_object.url.clone())
    }
}