use uuid::Uuid;

use responses::page_response::PageResponse;

#[derive(Debug)]
pub enum CrawlerEvent {
    CompleteEvent {
        uuid: Uuid,
    },
    PageEvent {
        page_response: PageResponse,
    }
}