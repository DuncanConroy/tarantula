use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use responses::get_response::GetResponse;
use responses::response_timings::ResponseTimings;
use tracing::trace;

use crate::http::http_client::HttpClient;
use crate::http::http_utils;

#[async_trait]
pub trait PageDownloadCommand: Sync + Send {
    async fn download_page(&self, uri: String, http_client: Arc<dyn HttpClient>, robots_txt_info_url: Option<String>) -> Result<GetResponse, String>;
}

pub struct DefaultPageDownloadCommand {}

#[async_trait]
impl PageDownloadCommand for DefaultPageDownloadCommand {
    async fn download_page(&self, uri: String, http_client: Arc<dyn HttpClient>, robots_txt_info_url: Option<String>) -> Result<GetResponse, String> {
        let start_time = DateTime::from(Utc::now());

        let response = http_client.get(uri.clone(), robots_txt_info_url).await.unwrap();
        trace!("GET for {}: {:?}", uri, response.headers());
        let headers: HashMap<String, String> = http_utils::response_headers_to_map(&response);
        let http_response_code = http_utils::map_status_code(response.status());
        let body: String = String::from_utf8_lossy(hyper::body::to_bytes(response.into_body()).await.unwrap().as_ref())
            .to_string();
        let result = GetResponse {
            http_response_code,
            headers,
            requested_url: uri.clone(),
            response_timings: ResponseTimings::from(uri.clone(), start_time, DateTime::from(Utc::now())),
            body: Some(body),
        };
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use hyper::{Body, Response};
    use mockall::*;

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
    async fn returns_simple_result_on_simple_request() {
        // given: simple download command
        let command = DefaultPageDownloadCommand {};
        let mut mock_http_client = MockMyHttpClient::new();
        mock_http_client.expect_get().returning(|_, _| Ok(Response::builder()
            .status(200)
            .body(Body::from("Hello World"))
            .unwrap()));
        let mock_http_client = Arc::new(mock_http_client);

        // when: fetch is invoked
        let result = command.download_page("https://example.com".into(), mock_http_client, None).await;

        // then: simple response is returned, with no redirects
        assert_eq!(result.is_ok(), true, "Expecting a simple Response");
        assert_eq!(result.as_ref().unwrap().body.is_some(), true, "Should have body");
        assert_eq!(result.as_ref().unwrap().body.as_ref().unwrap(), "Hello World", "Should have body");
        assert_eq!(result.as_ref().unwrap().response_timings.end_time.is_some(), true, "Should have updated end_time after successful run");
    }
}
