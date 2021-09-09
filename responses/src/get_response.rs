use std::collections::HashMap;

use serde::Serialize;

use crate::response_timings::ResponseTimings;
use crate::status_code::StatusCode;

#[derive(Debug, Clone, Serialize)]
pub struct GetResponse {
    pub requested_url: String,
    pub http_response_code: StatusCode,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
    pub response_timings: ResponseTimings,
}

impl GetResponse {
    pub fn new(requested_url: String, http_response_code: StatusCode) -> GetResponse {
        GetResponse {
            requested_url: requested_url.clone(),
            http_response_code,
            headers: HashMap::new(),
            body: None,
            response_timings: ResponseTimings::new(format!("GETResponse.{}", requested_url.clone())),
        }
    }
}