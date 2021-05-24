use linkresult::Link;

#[derive(Debug, Clone)]
pub struct PageResponse {
    pub original_requested_url: String,
    pub final_url_after_redirects: Option<String>,
    pub redirected_from: Option<Box<PageResponse>>,
    pub status_code: Option<u16>,
    pub head: Option<String>,
    pub body: Option<String>,
    pub links: Option<Vec<Link>>,
}

impl PageResponse {
    pub fn new(original_requested_url: String) -> PageResponse {
        PageResponse {
            original_requested_url,
            final_url_after_redirects: None,
            redirected_from: None,
            status_code: None,
            head: None,
            body: None,
            links: None,
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