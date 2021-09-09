use std::collections::HashMap;

use serde::Serialize;

use crate::redirect::Redirect;
use crate::response_timings::ResponseTimings;
use crate::status_code::StatusCode;

#[derive(Debug, Clone, Serialize)]
pub struct HeadResponse {
    pub requested_url: String,
    pub redirects: Vec<Redirect>,
    pub http_response_code: StatusCode,
    pub headers: HashMap<String, String>,
    pub response_timings: ResponseTimings,
}

impl HeadResponse {
    pub fn new(requested_url: String, http_response_code: StatusCode) -> HeadResponse {
        HeadResponse {
            requested_url: requested_url.clone(),
            redirects: vec![],
            http_response_code,
            headers: HashMap::new(),
            response_timings: ResponseTimings::new(format!("HEADResponse.{}", requested_url.clone())),
        }
    }

    pub fn get_final_uri(&self) -> String {
        if self.redirects.is_empty() {
            return self.requested_url.clone();
        }

        self.redirects.last().unwrap().destination.clone()
    }
}
