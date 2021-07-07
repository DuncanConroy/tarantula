use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use hyper::Uri;
use log::debug;

use crate::task_context::task_context::FullTaskContext;

#[derive(Clone, Debug)]
pub struct PageRequest {
    pub url: String,
    pub last_crawled_timestamp: Option<DateTime<Utc>>,
    pub task_context: Arc<Mutex<dyn FullTaskContext>>,
}

impl PageRequest {
    pub fn new(url: String, last_crawled_timestamp: Option<DateTime<Utc>>, task_context: Arc<Mutex<dyn FullTaskContext>>) -> PageRequest {
        PageRequest {
            url,
            last_crawled_timestamp,
            task_context,
        }
    }

    pub fn get_protocol(&self) -> String {
        let uri = self.get_uri();
        debug!("get protocol: {}", uri);
        uri.scheme_str().unwrap().to_owned()
    }

    pub fn get_host(&self) -> String {
        let uri = self.get_uri();
        debug!("get host: {}", uri);
        uri.host().unwrap().to_string()
    }

    pub fn get_uri(&self) -> Uri {
        self.url.parse::<hyper::Uri>().unwrap()
    }
}