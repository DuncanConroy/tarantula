use chrono::{DateTime, Utc};
use crate::task_context::TaskContext;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct PageRequest {
    pub url: String,
    pub last_crawled_timestamp: Option<DateTime<Utc>>,
    pub task_context: Arc<dyn TaskContext>,
}

impl PageRequest {
    pub fn new(url: String, last_crawled_timestamp: Option<DateTime<Utc>>, task_context: Arc<dyn TaskContext>) -> PageRequest {
        PageRequest {
            url,
            last_crawled_timestamp,
            task_context
        }
    }
}