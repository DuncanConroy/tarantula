use std::collections::HashMap;

use hyper::{Body, Response};

use responses::status_code::StatusCode;

pub fn response_headers_to_map(response: &Response<Body>) -> HashMap<String, String> {
    response.headers().iter()
        .map(|(key, value)| {
            (key.to_string().to_lowercase(), String::from(value.to_str().unwrap()))
        }).collect()
}

pub fn map_status_code(status: hyper::StatusCode) -> StatusCode {
    let unofficial_codes: HashMap<u16, &str> = HashMap::from([
        (520u16, "[CLOUDFLARE] Web Server Returned an Unknown Error"),
        (521u16, "[CLOUDFLARE] Web Server Is Down"),
        (522u16, "[CLOUDFLARE] Connection Timed Out"),
        (523u16, "[CLOUDFLARE] Origin Is Unreachable"),
        (524u16, "[CLOUDFLARE] A Timeout Occurred"),
        (525u16, "[CLOUDFLARE] SSL Handshake Failed"),
        (526u16, "[CLOUDFLARE] Invalid SSL Certificate"),
        (527u16, "[CLOUDFLARE] Railgun Error"),
    ]);

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