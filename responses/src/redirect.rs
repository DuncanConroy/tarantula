use std::collections::HashMap;

use serde::Serialize;

use crate::response_timings::ResponseTimings;
use crate::status_code::StatusCode;

#[derive(Debug, Clone, Serialize)]
pub struct Redirect {
    pub source: String,
    pub destination: String,
    pub http_response_code: StatusCode,
    pub headers: HashMap<String, String>,
    pub response_timings: ResponseTimings,
}

impl Redirect {
    pub fn from(source: String, destination: String) -> Redirect {
        Redirect {
            source: source.clone(),
            destination,
            http_response_code: StatusCode { code: 200, label: "OK".into() },
            headers: HashMap::new(),
            response_timings: ResponseTimings::new(format!("Redirects.{}", source)),
        }
    }
}
