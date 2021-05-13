use chrono::{DateTime, Utc};

pub struct PageRequest {
    url: String,
    last_crawled_timestamp: Option<DateTime<Utc>>,
}