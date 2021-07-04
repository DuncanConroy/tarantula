use std::collections::HashMap;

use hyper::{Body, Response};

pub fn response_headers_to_map(response: &Response<Body>) -> HashMap<String, String> {
    response.headers().iter()
        .map(|(key, value)| {
            (key.to_string(), String::from(value.to_str().unwrap()))
        }).collect()
}