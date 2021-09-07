use serde::Serialize;

use linkresult::Link;

use crate::commands::fetch_header_command::{FetchHeaderResponse, StatusCode};
use crate::response_timings::ResponseTimings;

#[derive(Debug, Clone, Serialize)]
pub struct PageResponse {
    pub original_requested_url: String,
    pub original_requested_url_raw: String,
    pub final_url_after_redirects: Option<String>,
    pub status_code: Option<StatusCode>,
    pub headers: Option<FetchHeaderResponse>,
    pub body: Option<String>,
    pub links: Option<Vec<Link>>,
    pub response_timings: ResponseTimings
}

impl PageResponse {
    pub fn new(original_requested_url: String, original_requested_url_raw: String) -> PageResponse {
        let response_timings_name = format!("PageResponse.{}", original_requested_url);
        let response_timings = ResponseTimings::new(response_timings_name);
        PageResponse {
            original_requested_url,
            original_requested_url_raw,
            final_url_after_redirects: None,
            status_code: None,
            headers: None,
            body: None,
            links: None,
            response_timings,
        }
    }
}
