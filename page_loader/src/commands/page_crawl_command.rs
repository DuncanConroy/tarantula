use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hyper::header::CONTENT_TYPE;
use log::debug;
use uuid::Uuid;

use responses::link::Link;
use responses::page_response::PageResponse;
use responses::status_code::StatusCode;

use crate::commands::fetch_header_command::FetchHeaderCommand;
use crate::commands::page_download_command::PageDownloadCommand;
use crate::http::http_client::HttpClient;
use crate::page_request::PageRequest;
use crate::task_context::task_context::FullTaskContext;

#[async_trait]
pub trait CrawlCommand: Sync + Send {
    fn get_url_clone(&self) -> String;
    fn get_page_request(&self) -> Arc<Mutex<PageRequest>>;
    async fn crawl(&self, http_client: Arc<dyn HttpClient>, task_context_uuid: Uuid, robots_txt_info_url: Option<String>) -> Result<Option<PageResponse>, String>;
    fn get_task_context(&self) -> Arc<Mutex<dyn FullTaskContext>>;
    fn get_current_depth(&self) -> u16;
}

pub struct PageCrawlCommand {
    pub request_object: Arc<Mutex<PageRequest>>,
    pub current_depth: u16,
    fetch_header_command: Box<dyn FetchHeaderCommand>,
    page_download_command: Box<dyn PageDownloadCommand>,
}

impl PageCrawlCommand {
    pub fn new(url: String, raw_url: String, task_context: Arc<Mutex<dyn FullTaskContext>>, current_depth: u16, fetch_header_command: Box<dyn FetchHeaderCommand>, page_download_command: Box<dyn PageDownloadCommand>) -> PageCrawlCommand {
        PageCrawlCommand {
            request_object: Arc::new(Mutex::new(PageRequest::new(url, raw_url, None, task_context))),
            current_depth,
            fetch_header_command,
            page_download_command,
        }
    }

    fn verify_crawlability(&self) -> bool {
        let request_object = self.request_object.clone();
        let request_object_locked = request_object.lock().unwrap();
        let task_context = request_object_locked.task_context.clone();
        let config = task_context.lock().unwrap().get_config().clone();
        let config_locked = config.lock().unwrap();
        if config_locked.maximum_depth > 0 &&
            self.current_depth > config_locked.maximum_depth {
            debug!("Dropping requested url: {} -> maximum_depth reached: {}", &request_object_locked.url, config_locked.maximum_depth);
            return false;
        }
        // at this point, the config isn't required anymore and can therefore be dropped
        drop(config_locked);
        drop(config);

        let task_context_locked = task_context.lock().unwrap();
        if task_context_locked.get_all_known_links().lock().unwrap().contains(&request_object_locked.url) {
            debug!("Dropping requested url: {} -> already known", &request_object_locked.url);
            return false;
        }

        if !task_context_locked.can_access(&request_object_locked.url) {
            debug!("Dropping requested url: {} -> can't access (robots.txt)", &request_object_locked.url);
            return false;
        }

        true
    }

    async fn perform_crawl_internal(&self, http_client: Arc<dyn HttpClient>, task_context_uuid: Uuid, robots_txt_info_url: Option<String>) -> Result<Option<PageResponse>, String> {
        let request_object_cloned = self.request_object.clone();
        let url = request_object_cloned.lock().unwrap().url.clone();
        let raw_url = request_object_cloned.lock().unwrap().raw_url.clone();
        let mut page_response = PageResponse::new(url, raw_url, task_context_uuid.clone());
        let fetch_header_response = self.fetch_header_command.fetch_header(request_object_cloned, http_client, None, robots_txt_info_url.clone()).await;
        if let Ok(result) = fetch_header_response {
            let http_client = result.1;
            let fetch_header_response = result.0;
            page_response.head = Some(fetch_header_response);
            let final_uri = page_response.head.as_ref().unwrap().get_final_uri();
            page_response.final_url_after_redirects = Some(final_uri.clone());

            let headers = &page_response.head.as_ref().unwrap().headers;
            if self.should_download_page(headers, &page_response.head.as_ref().unwrap().http_response_code) {
                let page_download_response = self.page_download_command.download_page(final_uri.clone(), http_client, robots_txt_info_url.clone()).await;
                if let Ok(download_result) = page_download_response {
                    page_response.get = Some(download_result);
                    if self.is_html(&page_response.get.as_ref().unwrap().headers) {
                        page_response.links = self.extract_links(page_response.get.as_ref().unwrap().body.as_ref());
                    }
                } else {
                    panic!("proper error handling needed")
                }
            }

            // todo work with dynamic filtering and mapping classes, like spring routing, etc.
        }
        page_response.response_timings.end_time = Some(DateTime::from(Utc::now()));
        Ok(Some(page_response))
    }

    fn should_download_page(&self, headers: &HashMap<String, String>, status_code: &StatusCode) -> bool {
        (hyper::StatusCode::from_u16(status_code.code).unwrap().is_success()
            || headers.contains_key("x-cache") && headers.get("x-cache").unwrap().contains("cloudfront")
        ) && self.is_html(headers)
    }

    fn is_html(&self, headers: &HashMap<String, String>) -> bool {
        headers.contains_key(CONTENT_TYPE.as_str()) &&
            headers.get(CONTENT_TYPE.as_str()).unwrap().contains("text/html")
    }

    fn extract_links(&self, body: Option<&String>) -> Option<Vec<Link>> {
        // todo!("TEST")
        if let Some(body_content) = body {
            let request_object = self.request_object.lock().unwrap();
            let dom_parser = request_object.task_context.lock().unwrap().get_dom_parser();
            let links = dom_parser.get_links(
                &request_object.get_protocol(),
                &request_object.get_host(),
                body_content);

            return match links {
                None => { None }
                Some(links) => Some(links.links)
            };
        }
        return None;
    }
}

#[async_trait]
impl CrawlCommand for PageCrawlCommand {
    fn get_url_clone(&self) -> String { self.request_object.lock().unwrap().url.clone() }

    fn get_page_request(&self) -> Arc<Mutex<PageRequest>> { self.request_object.clone() }

    async fn crawl(&self, http_client: Arc<dyn HttpClient>, task_context_uuid: Uuid, robots_txt_info_url: Option<String>) -> Result<Option<PageResponse>, String> {
        if !self.verify_crawlability() {
            return Ok(None);
        }

        self.perform_crawl_internal(http_client, task_context_uuid, robots_txt_info_url).await
    }

    fn get_task_context(&self) -> Arc<Mutex<dyn FullTaskContext>> {
        self.request_object.lock().unwrap().task_context.clone()
    }

    fn get_current_depth(&self) -> u16 { self.current_depth }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;
    use std::sync::{Arc, Mutex};

    use hyper::{Body, Response};
    use hyper::header::CONTENT_TYPE;
    use mockall::*;
    use tokio::sync::mpsc::Sender;
    use tokio::time::Instant;
    use uuid::Uuid;

    use dom_parser::DomParser;
    use linkresult::uri_result::UriResult;
    use linkresult::uri_service::UriService;
    use responses::get_response::GetResponse;
    use responses::head_response::HeadResponse;
    use responses::redirect::Redirect;

    use crate::commands::page_crawl_command::{CrawlCommand, PageCrawlCommand};
    use crate::events::crawler_event::CrawlerEvent;
    use crate::task_context::robots_service::RobotsTxt;
    use crate::task_context::task_context::{KnownLinks, TaskConfig, TaskContext, TaskContextServices};

    use super::*;

    mock! {
        MyTaskContext {}
        impl TaskContext for MyTaskContext {
            fn get_uuid_clone(&self) -> Uuid;
            fn get_config(&self) -> Arc<Mutex<TaskConfig>>;
            fn get_url(&self)->String;
            fn get_last_command_received(&self) -> Instant;
            fn set_last_command_received(&mut self, instant: Instant);
            fn can_be_garbage_collected(&self, gc_timeout_ms: u64) -> bool;
            fn get_response_channel(&self) -> &Sender<CrawlerEvent>;
        }
        impl TaskContextServices for MyTaskContext{
            fn get_uri_service(&self) -> Arc<UriService>;
            fn get_dom_parser(&self) ->Arc<dyn DomParser>;
            fn get_http_client(&self) -> Arc<dyn HttpClient>;
        }
        impl KnownLinks for MyTaskContext{
            fn get_all_known_links(&self) -> Arc<Mutex<Vec<String>>>;
            fn add_known_link(&self, link: String);
        }
        impl RobotsTxt for MyTaskContext{
            fn can_access(&self, item_uri: &str) -> bool;
        }
        impl FullTaskContext for MyTaskContext{}
    }
    mock! {
        MyDomParser {}
        impl DomParser for MyDomParser {
            fn get_links(&self, parent_protocol: &str, source_domain:&str, body: &String) -> Option<UriResult>;
        }
    }
    mock! {
        #[async_trait]
        MyHttpClient {}
        #[async_trait]
        impl HttpClient for MyHttpClient{
            async fn head(&self, uri: String, robots_txt_info_url: Option<String>) -> hyper::Result<Response<Body>>;
            async fn get(&self, uri: String, robots_txt_info_url: Option<String>) -> hyper::Result<Response<Body>>;
        }
    }
    mock! {
        #[async_trait]
        MyFetchHeaderCommand {}
        #[async_trait]
        impl FetchHeaderCommand for MyFetchHeaderCommand{
            async fn fetch_header(&self, page_request: Arc<Mutex<PageRequest>>, http_client: Arc<dyn HttpClient>, redirects: Option<Vec<Redirect>>, robots_txt_info_url: Option<String>) -> std::result::Result<(HeadResponse, Arc<dyn HttpClient>), String>;
        }
    }
    mock! {
        #[async_trait]
        MyPageDownloadCommand {}
        #[async_trait]
        impl PageDownloadCommand for MyPageDownloadCommand{
                async fn download_page(&self, uri: String, http_client: Arc<dyn HttpClient>, robots_txt_info_url: Option<String>) -> Result<GetResponse, String>;
        }
    }

    fn get_default_task_config() -> Arc<Mutex<TaskConfig>> {
        Arc::new(Mutex::new(TaskConfig {
            uri: Default::default(),
            ignore_redirects: false,
            maximum_redirects: 0,
            maximum_depth: 16,
            ignore_robots_txt: false,
            keep_html_in_memory: false,
            user_agent: "".to_string(),
            robots_txt_info_url: None,
            crawl_delay_ms: 1,
        }))
    }

    fn get_mock_http_client() -> Arc<MockMyHttpClient> {
        Arc::new(MockMyHttpClient::new())
    }

    #[tokio::test]
    async fn will_not_crawl_if_max_depth_reached() {
        // given: a task context with maximum_depth > 0
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        let url_clone = url.clone();
        mock_task_context.expect_get_url().return_const(url_clone);
        let config = get_default_task_config();
        config.lock().unwrap().maximum_depth = 1;
        mock_task_context.expect_get_config().return_const(config.clone());
        let mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        let mock_page_download_command = Box::new(MockMyPageDownloadCommand::new());

        // when: invoked with a current_depth > 0 && > maximum_depth
        let page_crawl_command = PageCrawlCommand::new(
            String::from("https://example.com"),
            String::from("https://example.com"),
            Arc::new(Mutex::new(mock_task_context)),
            2,
            mock_fetch_header_command,
            mock_page_download_command,
        );
        let mock_http_client = get_mock_http_client();
        let crawl_result = page_crawl_command.crawl(mock_http_client, Uuid::new_v4(), None).await;

        // then: expect none
        assert_eq!(crawl_result.as_ref().unwrap().is_none(), true, "Should not crawl, if max depth reached");
    }

    #[tokio::test]
    async fn will_crawl_if_max_depth_is_zero() {
        // given: a task context with maximum_depth = 0
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());
        mock_task_context.expect_get_all_known_links().return_const(Arc::new(Mutex::new(vec![])));
        let config = get_default_task_config();
        config.lock().unwrap().maximum_depth = 0;
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_can_access().returning(|_| true);
        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|_, _, _, _| Ok((HeadResponse::new(String::from("https://example.com"), StatusCode { code: hyper::StatusCode::IM_A_TEAPOT.as_u16(), label: hyper::StatusCode::IM_A_TEAPOT.canonical_reason().unwrap().into() }), get_mock_http_client())));
        let mock_page_download_command = Box::new(MockMyPageDownloadCommand::new());
        let mut mock_http_client = MockMyHttpClient::new();
        mock_http_client.expect_head().returning(|_, _| Ok(Response::builder()
            .status(200)
            .body(Body::from(""))
            .unwrap()));
        let mock_http_client = Arc::new(mock_http_client);

        // when: invoked with a current_depth > 0
        let page_crawl_command = PageCrawlCommand::new(
            String::from("https://example.com"),
            String::from("https://example.com"),
            Arc::new(Mutex::new(mock_task_context)),
            9000,
            mock_fetch_header_command,
            mock_page_download_command);
        let crawl_result = page_crawl_command.crawl(mock_http_client, Uuid::new_v4(), None).await;

        // then: expect some
        assert_eq!(crawl_result.as_ref().unwrap().is_some(), true, "Should crawl, if max depth not reached, yet");
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().response_timings.end_time.is_some(), true, "Should have end_time, regardless of status code");
    }

    #[tokio::test]
    async fn will_not_crawl_if_url_is_known() {
        // given: a task context with a known link
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_known_links().return_const(Arc::new(Mutex::new(vec![url.clone()])));
        let mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        let mock_page_download_command = Box::new(MockMyPageDownloadCommand::new());

        // when: invoked with a known link
        let page_crawl_command = PageCrawlCommand::new(
            url.clone(),
            url.clone(),
            Arc::new(Mutex::new(mock_task_context)),
            1,
            mock_fetch_header_command,
            mock_page_download_command);
        let mock_http_client = get_mock_http_client();
        let crawl_result = page_crawl_command.crawl(mock_http_client, Uuid::new_v4(), None).await;

        // then: expect none
        assert_eq!(crawl_result.as_ref().unwrap().is_none(), true, "Should not crawl, if url is known");
    }

    #[tokio::test]
    async fn will_crawl_if_url_is_unknown() {
        // given: a task context without the link known
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_known_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| true);
        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|_, _, _, _| Ok((HeadResponse::new(String::from("https://example.com"), StatusCode { code: hyper::StatusCode::IM_A_TEAPOT.as_u16(), label: hyper::StatusCode::IM_A_TEAPOT.canonical_reason().unwrap().into() }), get_mock_http_client())));
        let mock_page_download_command = Box::new(MockMyPageDownloadCommand::new());
        let mut mock_http_client = MockMyHttpClient::new();
        mock_http_client.expect_head().returning(|_, _| Ok(Response::builder()
            .status(200)
            .body(Body::from(""))
            .unwrap()));
        let mock_http_client = Arc::new(mock_http_client);

        // when: invoked with a known link
        let page_crawl_command = PageCrawlCommand::new(
            String::from("https://example.com"),
            String::from("https://example.com"),
            Arc::new(Mutex::new(mock_task_context)),
            1,
            mock_fetch_header_command,
            mock_page_download_command);
        let crawl_result = page_crawl_command.crawl(mock_http_client, Uuid::new_v4(), None).await;

        // then: expect some
        assert_eq!(crawl_result.as_ref().unwrap().is_some(), true, "Should crawl, if url is unknown");
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().response_timings.end_time.is_some(), true, "Should have end_time, regardless of status code");
    }

    #[tokio::test]
    async fn will_not_crawl_if_url_is_forbidden_by_robots_txt() {
        // given: a task context with robots_txt disallowing crawling
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_known_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| false);
        let mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        let mock_page_download_command = Box::new(MockMyPageDownloadCommand::new());

        // when: invoked with a restricted link
        let page_crawl_command = PageCrawlCommand::new(
            String::from("https://example.com"),
            String::from("https://example.com"),
            Arc::new(Mutex::new(mock_task_context)),
            1,
            mock_fetch_header_command,
            mock_page_download_command,
        );
        let mock_http_client = get_mock_http_client();
        let crawl_result = page_crawl_command.crawl(mock_http_client, Uuid::new_v4(), None).await;

        // then: expect none
        assert_eq!(crawl_result.as_ref().unwrap().is_none(), true, "Should not crawl urls forbidden by robots.txt");
    }

    #[tokio::test]
    async fn returns_proper_page_response_on_successful_crawl() {
        // given: a task context that allows crawl
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_known_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| true);
        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|_, _, _, _| Ok((HeadResponse::new(String::from("https://example.com"), StatusCode { code: hyper::StatusCode::IM_A_TEAPOT.as_u16(), label: hyper::StatusCode::IM_A_TEAPOT.canonical_reason().unwrap().into() }), get_mock_http_client())));
        let mock_page_download_command = Box::new(MockMyPageDownloadCommand::new());

        // when: invoked with a regular link
        let page_crawl_command = PageCrawlCommand::new(
            String::from("https://example.com"),
            String::from("https://example.com"),
            Arc::new(Mutex::new(mock_task_context)),
            1,
            mock_fetch_header_command,
            mock_page_download_command,
        );
        let mock_http_client = get_mock_http_client();
        let crawl_result = page_crawl_command.crawl(mock_http_client, Uuid::new_v4(), None).await;

        // then: expect some PageResponse with Teapot status code
        assert_eq!(crawl_result.as_ref().unwrap().is_some(), true, "Should crawl urls if allowed");
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().head.is_some(), true, "Should have head, regardless of status code");
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().head.as_ref().unwrap().http_response_code.code, hyper::StatusCode::IM_A_TEAPOT.as_u16());
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().response_timings.end_time.is_some(), true, "Should have end_time, regardless of status code");
    }

    #[tokio::test]
    async fn returned_page_response_does_not_include_body_if_head_status_is_not_200() {
        // given: a task context that allows crawl
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_known_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| true);
        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|_, _, _, _| Ok((HeadResponse::new(String::from("https://example.com"), StatusCode { code: hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16(), label: hyper::StatusCode::INTERNAL_SERVER_ERROR.canonical_reason().unwrap().into() }), get_mock_http_client())));
        let mock_page_download_command = Box::new(MockMyPageDownloadCommand::new());

        // when: invoked with a regular link
        let page_crawl_command = PageCrawlCommand::new(
            String::from("https://example.com"),
            String::from("https://example.com"),
            Arc::new(Mutex::new(mock_task_context)),
            1,
            mock_fetch_header_command,
            mock_page_download_command,
        );
        let mock_http_client = get_mock_http_client();
        let crawl_result = page_crawl_command.crawl(mock_http_client, Uuid::new_v4(), None).await;

        // then: expect some PageResponse with InternalServerError status code and no body
        assert_eq!(crawl_result.as_ref().unwrap().is_some(), true, "Should crawl urls if allowed");
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().get.is_none(), true, "Should not have get response, if status is not ok");
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().head.is_some(), true, "Should have head, regardless of status code");
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().head.as_ref().unwrap().http_response_code.code, hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16());
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().response_timings.end_time.is_some(), true, "Should have end_time, regardless of status code");
        let is_page_response_before_fetch_header_response = crawl_result.as_ref().unwrap().as_ref().unwrap()
            .response_timings.start_time.as_ref().unwrap()
            .cmp(crawl_result.as_ref().unwrap().as_ref().unwrap()
                .head.as_ref().unwrap()
                .response_timings.start_time.as_ref().unwrap());
        assert_eq!(is_page_response_before_fetch_header_response, Ordering::Less, "PageResponse start_time should be before HeadResponse start_time");
    }

    #[tokio::test]
    async fn does_not_download_page_if_content_type_is_not_text_html() {
        // given: a task context that allows crawl
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_known_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| true);
        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|_, _, _, _| {
            let mut header_response = HeadResponse::new(String::from("https://example.com"), StatusCode { code: hyper::StatusCode::OK.as_u16(), label: hyper::StatusCode::OK.canonical_reason().unwrap().into() });
            header_response.headers.insert(CONTENT_TYPE.as_str().into(), "application/json; charset=UTF-8".into());

            Ok((header_response, get_mock_http_client()))
        });
        let mock_page_download_command = Box::new(MockMyPageDownloadCommand::new());

        // when: invoked with a regular link
        let page_crawl_command = PageCrawlCommand::new(
            String::from("https://example.com"),
            String::from("https://example.com"),
            Arc::new(Mutex::new(mock_task_context)),
            1,
            mock_fetch_header_command,
            mock_page_download_command,
        );
        let mock_http_client = get_mock_http_client();
        let crawl_result = page_crawl_command.crawl(mock_http_client, Uuid::new_v4(), None).await;

        // then: expect some PageResponse without body
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().get.is_none(), true, "Should not have get response, if status content-type is not text/html");
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().head.is_some(), true, "Should have head, regardless of status code");
    }

    #[tokio::test]
    async fn downloads_page_if_content_type_is_text_html() {
        // given: a task context that allows crawl
        let url = String::from("https://example.com");

        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());

        let config = get_default_task_config();

        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_known_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| true);
        mock_task_context.expect_get_dom_parser().returning(|| {
            let mut dom_parser = MockMyDomParser::new();
            dom_parser.expect_get_links().returning(|_, _, _| None);
            Arc::new(dom_parser)
        });

        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|_, _, _, _| {
            let mut header_response = HeadResponse::new(String::from("https://example.com"), StatusCode { code: hyper::StatusCode::OK.as_u16(), label: hyper::StatusCode::OK.canonical_reason().unwrap().into() });
            header_response.headers.insert(CONTENT_TYPE.as_str().into(), "text/html; charset=UTF-8".into());
            header_response.redirects.push(Redirect::from(
                String::from("https://example.com"),
                String::from("https://initial-redirection.example.com"),
            ));
            header_response.redirects.push(Redirect::from(
                String::from("https://initial-redirection.example.com"),
                String::from("https://final-redirection.example.com"),
            ));
            Ok((header_response, get_mock_http_client()))
        });

        let mut mock_page_download_command = Box::new(MockMyPageDownloadCommand::new());
        mock_page_download_command.expect_download_page()
            .returning(|uri, _, _| {
                if uri == "https://final-redirection.example.com" {
                    let mut download_response = GetResponse::new(uri.clone(), StatusCode { code: hyper::StatusCode::OK.as_u16(), label: hyper::StatusCode::OK.canonical_reason().unwrap().into() });
                    download_response.headers = HashMap::new();
                    download_response.headers.insert("content-type".into(), "text/html".into());
                    download_response.body = Some("<html><p>Hello World!</p></html>".into());
                    return Ok(download_response);
                }
                Err(String::from("Wrong URL received in test"))
            });

        // when: invoked with a regular link
        let page_crawl_command = PageCrawlCommand::new(
            String::from("https://example.com"),
            String::from("https://example.com"),
            Arc::new(Mutex::new(mock_task_context)),
            1,
            mock_fetch_header_command,
            mock_page_download_command,
        );
        let mock_http_client = get_mock_http_client();
        let crawl_result = page_crawl_command.crawl(mock_http_client, Uuid::new_v4(), None).await;

        // then: expect some PageResponse with body
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().get.as_ref().unwrap().body.is_some(), true, "Should have body, if status content-type is text/html");
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().get.as_ref().unwrap().body.as_ref().unwrap(), &String::from("<html><p>Hello World!</p></html>"), "Should have body, if status content-type is text/html");
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().head.is_some(), true, "Should have head, regardless of status code");
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().final_url_after_redirects.is_some(), true, "Should have final_url_after_redirects updated");
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().final_url_after_redirects.as_ref().unwrap(), "https://final-redirection.example.com", "Should have final_url_after_redirects set to requested url");
    }
}