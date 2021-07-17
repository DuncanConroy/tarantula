use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use hyper::Uri;
use log::trace;

use crate::task_context::task_context::FullTaskContext;
use std::fmt::Debug;
use std::fmt;

#[derive(Clone)]
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
        trace!("get protocol: {}", uri);
        uri.scheme_str().unwrap().to_owned()
    }

    pub fn get_host(&self) -> String {
        let uri = self.get_uri();
        trace!("get host: {}", uri);
        uri.host().unwrap().to_string()
    }

    pub fn get_uri(&self) -> Uri {
        self.url.parse::<hyper::Uri>().unwrap()
    }
}

impl Debug for PageRequest {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("PageRequest")
            .field("url", &self.url)
            .field("last_crawled_timestamp", &self.last_crawled_timestamp)
            .finish()
    }
}