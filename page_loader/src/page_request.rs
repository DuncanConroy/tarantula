use chrono::{DateTime, Utc};

#[derive(Clone, Debug)]
pub struct PageRequest {
    pub url: String,
    pub last_crawled_timestamp: Option<DateTime<Utc>>,
}

impl PageRequest {
    pub fn new(url: String, last_crawled_timestamp: Option<DateTime<Utc>>) -> PageRequest {
        PageRequest {
            url,
            last_crawled_timestamp,
        }
    }
}