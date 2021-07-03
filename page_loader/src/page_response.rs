use std::collections::HashMap;
use std::iter::Map;

use linkresult::Link;

use crate::commands::fetch_header_command::FetchHeaderResponse;
use crate::response_timings::ResponseTimings;

#[derive(Debug, Clone)]
pub struct PageResponse {
    pub original_requested_url: String,
    pub final_url_after_redirects: Option<String>,
    pub status_code: Option<u16>,
    pub headers: Option<FetchHeaderResponse>,
    pub body: Option<String>,
    pub links: Option<Vec<Link>>,
    pub response_timings: ResponseTimings
}

impl PageResponse {
    pub fn new(original_requested_url: String) -> PageResponse {
        let response_timings_name = format!("PageResponse.{}", original_requested_url);
        let response_timings = ResponseTimings::new(response_timings_name);
        PageResponse {
            original_requested_url,
            final_url_after_redirects: None,
            status_code: None,
            headers: None,
            body: None,
            links: None,
            response_timings,
        }
    }

    fn is_final_destination(&self) -> bool {
        if let Some(status_code) = self.status_code {
            return match status_code {
                300u16..=399u16 => false,
                _ => true,
            };
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use crate::page_response::*;

    #[test]
    fn is_final_destination_on_smaller_greater_300eds() {
        let statuses_expectations = vec![
            (200u16, true),
            (299u16, true),
            (300u16, false),
            (399u16, false),
            (400u16, true),
        ];

        statuses_expectations.iter()
            .for_each(|(status_code, expected_result)| {
                let mut response = PageResponse::new("http://example.com".into());
                response.status_code = Some(*status_code);
                let actual_result = response.is_final_destination();
                assert_eq!(*expected_result, actual_result);
            });
    }
}