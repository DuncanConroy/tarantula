use std::collections::HashMap;

use hyper::{Body, Response};

use responses::status_code::StatusCode;

pub fn response_headers_to_map(response: &Response<Body>) -> HashMap<String, String> {
    response.headers().iter()
        .map(|(key, value)| {
            (key.to_string().to_lowercase(), String::from(value.to_str().unwrap()))
        }).collect()
}

fn build_status_codes() -> HashMap<u16, &'static str> {
    let mut status_codes = HashMap::new();
    status_codes.insert(520u16, "[CLOUDFLARE] Web Server Returned an Unknown Error");
    status_codes.insert(521u16, "[CLOUDFLARE] Web Server Is Down");
    status_codes.insert(522u16, "[CLOUDFLARE] Connection Timed Out");
    status_codes.insert(523u16, "[CLOUDFLARE] Origin Is Unreachable");
    status_codes.insert(524u16, "[CLOUDFLARE] A Timeout Occurred");
    status_codes.insert(525u16, "[CLOUDFLARE] SSL Handshake Failed");
    status_codes.insert(526u16, "[CLOUDFLARE] Invalid SSL Certificate");
    status_codes.insert(527u16, "[CLOUDFLARE] Railgun Error");

    status_codes
}

pub fn map_status_code(status: hyper::StatusCode) -> StatusCode {
    let unofficial_codes: HashMap<u16, &str> = build_status_codes();

    let code = status.as_u16();
    let label = if let Some(reason) = status.canonical_reason() {
        reason
    } else {
        unofficial_codes.get(&code)
            .unwrap_or(&"Unknown Status Code")
    };
    StatusCode {
        code,
        label: String::from(label),
    }
}