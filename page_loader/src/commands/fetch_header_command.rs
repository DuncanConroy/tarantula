use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hyper::{Body, Response, Uri};
use hyper::header::HeaderValue;
use log::{debug, info, trace};

use linkresult::uri_service::UriService;
use responses::head_response::HeadResponse;
use responses::redirect::Redirect;
use responses::response_timings::ResponseTimings;
use responses::status_code::StatusCode;

use crate::http::http_client::HttpClient;
use crate::http::http_utils;

#[async_trait]
pub trait FetchHeaderCommand: Sync + Send {
    async fn fetch_header(&self, url: String, ignore_redirects: bool, maximum_redirects: u8, uri_service: Arc<UriService>, http_client: Arc<dyn HttpClient>, redirects: Option<Vec<Redirect>>, robots_txt_info_url: Option<String>) -> Result<(HeadResponse, Arc<dyn HttpClient>), String>;
}

pub struct DefaultFetchHeaderCommand {}

#[async_trait]
impl FetchHeaderCommand for DefaultFetchHeaderCommand {
    async fn fetch_header(&self, url: String, ignore_redirects: bool, maximum_redirects: u8, uri_service: Arc<UriService>, http_client: Arc<dyn HttpClient>, redirects: Option<Vec<Redirect>>, robots_txt_info_url: Option<String>) -> Result<(HeadResponse, Arc<dyn HttpClient>), String> {
        let start_time = DateTime::from(Utc::now());
        let mut uri = url.clone();

        let mut num_redirects = 0;
        if redirects.is_some() {
            let redirects_unwrapped = redirects.as_ref().unwrap();
            num_redirects = redirects_unwrapped.len() as u8;
            uri = redirects_unwrapped.last().unwrap().destination.clone();
        }

        let response = http_client.head(uri.clone(), robots_txt_info_url.clone()).await.unwrap();
        trace!("HEAD for {}: {:?}", uri, response.headers());
        let headers: HashMap<String, String> = http_utils::response_headers_to_map(&response);
        let can_process_redirects = !ignore_redirects && num_redirects < maximum_redirects && response.status().is_redirection();
        if can_process_redirects {
            if let Some(location_header) = response.headers().get("location") {
                let redirects_for_next = DefaultFetchHeaderCommand::append_redirect(uri_service.clone(), redirects, uri, &response, &headers, location_header, start_time);
                let response = self.fetch_header(url.clone(), false, maximum_redirects, uri_service.clone(), http_client.clone(), Some(redirects_for_next), robots_txt_info_url.clone()).await;
                return response;
            }
            let error_message = format!("No valid location found in redirect header {:?}", response);
            info!("{}", &error_message);
        }

        let redirects_result = redirects.unwrap_or(vec![]);
        let result = HeadResponse {
            redirects: redirects_result,
            http_response_code: http_utils::map_status_code(response.status()),
            headers,
            requested_url: uri.clone(),
            response_timings: ResponseTimings::from(format!("HeadResponse.{}", uri.clone()), start_time, DateTime::from(Utc::now())),
        };
        Ok((result, http_client))
    }
}

impl DefaultFetchHeaderCommand {
    fn append_redirect(uri_service: Arc<UriService>, redirects: Option<Vec<Redirect>>, uri: String, response: &Response<Body>, headers: &HashMap<String, String>, location_header: &HeaderValue, redirect_start_time: DateTime<Utc>) -> Vec<Redirect> {
        let uri_object = Uri::from_str(&uri).unwrap();
        let adjusted_uri = uri_service.form_full_url(uri_object.scheme_str().unwrap(), location_header.to_str().unwrap(), uri_object.host().unwrap(), &Some(uri.clone()));
        let redirect = Redirect {
            source: uri.clone(),
            destination: adjusted_uri.to_string(),
            http_response_code: StatusCode { code: response.status().as_u16(), label: response.status().canonical_reason().unwrap().into() },
            headers: headers.clone(),
            response_timings: ResponseTimings::from(format!("Redirect.{}", uri.clone()), redirect_start_time, DateTime::from(Utc::now())),
        };
        debug!("Following redirect {}", adjusted_uri);
        let mut redirects_for_next = vec![];
        if redirects.is_some() {
            redirects_for_next.append(&mut redirects.unwrap());
        }
        redirects_for_next.push(redirect);
        redirects_for_next
    }
}

#[cfg(test)]
mod tests {
    use mockall::*;
    use mockall::predicate::eq;

    use linkresult::link_type_checker::LinkTypeChecker;
    use linkresult::uri_service::UriService;

    use super::*;

    mock! {
        MyHttpClient {}
        #[async_trait]
        impl HttpClient for MyHttpClient{
            async fn head(&self, uri: String, robots_txt_info_url: Option<String>) -> hyper::Result<Response<Body>>;
            async fn get(&self, uri: String, robots_txt_info_url: Option<String>) -> hyper::Result<Response<Body>>;
        }
    }

    #[tokio::test]
    async fn returns_simple_result_on_simple_request_without_redirect_following() {
        // given: simple fetch command
        let command = DefaultFetchHeaderCommand {};
        let uri_service = Arc::new(UriService::new(Arc::new(LinkTypeChecker::new("example.com"))));
        let mut mock_http_client = MockMyHttpClient::new();
        mock_http_client.expect_head().returning(|_, _| Ok(Response::builder()
            .status(200)
            .body(Body::from(""))
            .unwrap()));
        let mock_http_client = Arc::new(mock_http_client);

        // when: fetch is invoked
        let result = command.fetch_header("https://example.com".into(), false, 10, uri_service, mock_http_client, None, None).await;

        // then: simple response is returned, with no redirects
        assert_eq!(result.is_ok(), true, "Expecting a simple Response");
        assert_eq!(result.as_ref().unwrap().0.redirects.len(), 0, "Should not have any redirects");
        assert_eq!(result.as_ref().unwrap().0.response_timings.end_time.is_some(), true, "Should have updated end_time after successful run");
    }

    #[tokio::test]
    async fn should_return_redirect_list_up_to_max_redirects() {
        // given: simple fetch command
        let target_domain = "example.com";
        let target_url = String::from(format!("https://{}", target_domain));
        let command = DefaultFetchHeaderCommand {};
        let uri_service = Arc::new(UriService::new(Arc::new(LinkTypeChecker::new(target_domain))));

        let mut mock_http_client = MockMyHttpClient::new();
        let mut sequence = Sequence::new();
        mock_http_client.expect_head()
            .with(eq(target_url.clone()), eq(None))
            .times(1)
            .in_sequence(&mut sequence)
            .returning(|_, _x: Option<String>| Ok(Response::builder()
                .status(308)
                .header("location", "https://first-redirect.example.com/")
                .body(Body::from(""))
                .unwrap()));
        mock_http_client.expect_head()
            .with(eq(String::from("https://first-redirect.example.com/")), eq(None))
            .times(1)
            .in_sequence(&mut sequence)
            .returning(|_, _x: Option<String>| Ok(Response::builder()
                .status(308)
                .header("location", "https://second-redirect.example.com")
                .header("x-custom", "Hello World")
                .body(Body::from(""))
                .unwrap()));
        mock_http_client.expect_head().returning(|_, _| Ok(Response::builder()
            .status(200)
            .header("x-custom", "Final destination")
            .body(Body::from(""))
            .unwrap()));
        let mock_http_client = Arc::new(mock_http_client);

        // when: fetch is invoked
        let result = command.fetch_header(target_url.clone(), false, 2, uri_service, mock_http_client, None, None).await;

        // then: simple response is returned, with maximum_redirects redirects
        assert_eq!(result.is_ok(), true, "Expecting a Response with redirects");
        let result_unwrapped = result.unwrap().0;
        assert_eq!(result_unwrapped.redirects.len(), 2, "Should have two redirects");
        assert_eq!(result_unwrapped.headers.get("x-custom").unwrap(), &String::from("Final destination"), "Should have headers embedded");
        assert_eq!(result_unwrapped.response_timings.end_time.is_some(), true, "Should have updated end_time after successful run");

        assert_eq!(result_unwrapped.redirects[0].source, target_url, "Source should match");
        assert_eq!(result_unwrapped.redirects[0].destination, String::from("https://first-redirect.example.com/"), "Destination should match");
        assert_eq!(result_unwrapped.redirects[0].headers.get("location").unwrap(), &String::from("https://first-redirect.example.com/"), "Should have headers embedded");
        assert_eq!(result_unwrapped.redirects[0].response_timings.end_time.is_some(), true, "Should have updated end_time after successful run - redirect[0]");
        assert_eq!(result_unwrapped.redirects[1].source, String::from("https://first-redirect.example.com/"), "Source should match");
        assert_eq!(result_unwrapped.redirects[1].destination, String::from("https://second-redirect.example.com/"), "Destination should match");
        assert_eq!(result_unwrapped.redirects[1].headers.get("x-custom").unwrap(), &String::from("Hello World"), "Should have headers embedded");
        assert_eq!(result_unwrapped.redirects[1].response_timings.end_time.is_some(), true, "Should have updated end_time after successful run - redirect[1]");
    }

    #[tokio::test]
    async fn should_return_no_redirect_if_ignore_redirects_is_true() {
        // given: simple fetch command
        let target_domain = "example.com";
        let target_url = String::from(format!("https://{}", target_domain));
        let command = DefaultFetchHeaderCommand {};
        let uri_service = Arc::new(UriService::new(Arc::new(LinkTypeChecker::new(target_domain))));

        let mut mock_http_client = MockMyHttpClient::new();
        let mut sequence = Sequence::new();
        mock_http_client.expect_head()
            .with(eq(target_url.clone()), eq(None))
            .times(1)
            .in_sequence(&mut sequence)
            .returning(|_, _x: Option<String>| Ok(Response::builder()
                .status(308)
                .header("location", "https://ignorable-redirect.example.com/")
                .body(Body::from(""))
                .unwrap()));
        let mock_http_client = Arc::new(mock_http_client);

        // when: fetch is invoked
        let result = command.fetch_header(target_url.clone(), true, 0, uri_service, mock_http_client, None, None).await;

        // then: simple response is returned, with no redirects
        assert_eq!(result.is_ok(), true, "Expecting a simple Response");
        let result_unwrapped = result.unwrap().0;
        assert_eq!(result_unwrapped.redirects.len(), 0, "Should have no redirects");
        assert_eq!(result_unwrapped.response_timings.end_time.is_some(), true, "Should have updated end_time after successful run");
    }

    #[tokio::test]
    // ignore_redirects takes precedence
    async fn should_return_no_redirect_if_ignore_redirects_is_true_and_maximum_redirect_is_set() {
        // given: simple fetch command
        let target_domain = "example.com";
        let target_url = String::from(format!("https://{}", target_domain));
        let command = DefaultFetchHeaderCommand {};
        let uri_service = Arc::new(UriService::new(Arc::new(LinkTypeChecker::new(target_domain))));

        let mut mock_http_client = MockMyHttpClient::new();
        let mut sequence = Sequence::new();
        mock_http_client.expect_head()
            .with(eq(target_url.clone()), eq(None))
            .times(1)
            .in_sequence(&mut sequence)
            .returning(|_, _x: Option<String>| Ok(Response::builder()
                .status(308)
                .header("location", "https://ignorable-redirect.example.com/")
                .body(Body::from(""))
                .unwrap()));
        let mock_http_client = Arc::new(mock_http_client);

        // when: fetch is invoked
        let result = command.fetch_header(target_url.clone(), true, 2, uri_service, mock_http_client, None, None).await;

        // then: simple response is returned, with no redirects
        assert_eq!(result.is_ok(), true, "Expecting a simple Response");
        let result_unwrapped = result.unwrap().0;
        assert_eq!(result_unwrapped.redirects.len(), 0, "Should have no redirects");
        assert_eq!(result_unwrapped.response_timings.end_time.is_some(), true, "Should have updated end_time after successful run");
    }
}
