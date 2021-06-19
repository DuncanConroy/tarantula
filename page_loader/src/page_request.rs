use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::task_context::task_context::FullTaskContext;

#[derive(Clone, Debug)]
pub struct PageRequest {
    pub url: String,
    pub last_crawled_timestamp: Option<DateTime<Utc>>,
    pub task_context: Arc<dyn FullTaskContext>,
}

impl PageRequest {
    pub fn new(url: String, last_crawled_timestamp: Option<DateTime<Utc>>, task_context: Arc<dyn FullTaskContext>) -> PageRequest {
        PageRequest {
            url,
            last_crawled_timestamp,
            task_context,
        }
    }
}