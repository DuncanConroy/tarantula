use serde::Serialize;
use uuid::Uuid;

use crate::get_response::GetResponse;
use crate::head_response::HeadResponse;
use crate::link::Link;
use crate::response_timings::ResponseTimings;

#[derive(Debug, Clone, Serialize)]
pub struct PageResponse {
    pub original_requested_url: String,
    pub original_requested_url_raw: String,
    pub final_url_after_redirects: Option<String>,
    pub head: Option<HeadResponse>,
    pub get: Option<GetResponse>,
    pub links: Option<Vec<Link>>,
    pub response_timings: ResponseTimings,
    pub uuid: Uuid,
}

impl PageResponse {
    pub fn new(original_requested_url: String, original_requested_url_raw: String, uuid: Uuid) -> PageResponse {
        let response_timings_name = format!("PageResponse.{}", original_requested_url);
        let response_timings = ResponseTimings::new(response_timings_name);
        PageResponse {
            original_requested_url,
            original_requested_url_raw,
            final_url_after_redirects: None,
            head: None,
            get: None,
            links: None,
            response_timings,
            uuid,
        }
    }
}
