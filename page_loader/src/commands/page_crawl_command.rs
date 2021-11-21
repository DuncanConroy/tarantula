use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hyper::Error;
use hyper::header::CONTENT_TYPE;
use log::debug;
use uuid::Uuid;

use dom_parser::DomParser;
use responses::crawl_status::CrawlStatus;
use responses::get_response::GetResponse;
use responses::link::Link;
use responses::page_response::PageResponse;
use responses::status_code::StatusCode;

use crate::commands::fetch_header_command::{FetchHeaderCommand, HeadResponseResult};
use crate::commands::page_download_command::PageDownloadCommand;
use crate::http::http_client::HttpClient;
use crate::page_request::PageRequest;
use crate::task_context::task_context::FullTaskContext;

#[async_trait]
pub trait CrawlCommand: Sync + Send {
    fn get_url_clone(&self) -> String;
    fn get_page_request(&self) -> Arc<Mutex<PageRequest>>;
    async fn crawl(&self, http_client: Arc<dyn HttpClient>, task_context_uuid: Uuid, robots_txt_info_url: Option<String>) -> Result<Option<PageResponse>, Error>;
    fn get_task_context(&self) -> Arc<Mutex<dyn FullTaskContext>>;
    fn get_current_depth(&self) -> u16;
    fn get_uuid_clone(&self) -> Uuid;
}

#[derive(Debug, PartialEq)]
enum Crawlability {
    AlreadyKnown,
    AlreadyTasked,
    Crawlable,
    RestrictedByRobotsTxt,
    MaxDepthReached,
}

pub struct PageCrawlCommand {
    pub request_object: Arc<Mutex<PageRequest>>,
    pub current_depth: u16,
    fetch_header_command: Box<dyn FetchHeaderCommand>,
    page_download_command: Box<dyn PageDownloadCommand>,
    uuid: Uuid,
}

impl PageCrawlCommand {
    pub fn new(url: String, raw_url: String, task_context: Arc<Mutex<dyn FullTaskContext>>, current_depth: u16, fetch_header_command: Box<dyn FetchHeaderCommand>, page_download_command: Box<dyn PageDownloadCommand>) -> PageCrawlCommand {
        PageCrawlCommand {
            request_object: Arc::new(Mutex::new(PageRequest::new(url, raw_url, None, task_context))),
            current_depth,
            fetch_header_command,
            page_download_command,
            uuid: Uuid::new_v4(),
        }
    }

    fn verify_crawlability(&self) -> Crawlability {
        let request_object = self.request_object.clone();
        let request_object_locked = request_object.lock().unwrap();
        let task_context = request_object_locked.task_context.clone();
        let task_context_locked = task_context.lock().unwrap();
        let config = task_context_locked.get_config().clone();
        let config_locked = config.lock().unwrap();
        if config_locked.maximum_depth > 0 &&
            self.current_depth >= config_locked.maximum_depth {
            debug!("Dropping requested url: {} -> maximum_depth reached: {}", &request_object_locked.url, config_locked.maximum_depth);
            return Crawlability::MaxDepthReached;
        }
        // at this point, the config isn't required anymore and can therefore be dropped
        drop(config_locked);
        drop(config);

        if task_context_locked.get_all_crawled_links().lock().unwrap().contains(&request_object_locked.url) {
            debug!("Dropping requested url: {} -> already known", &request_object_locked.url);
            return Crawlability::AlreadyKnown;
        }

        if task_context_locked.get_all_tasked_links().lock().unwrap().contains(&request_object_locked.url) {
            debug!("Dropping requested url: {} -> already tasked", &request_object_locked.url);
            return Crawlability::AlreadyTasked;
        }

        if !task_context_locked.can_access(&request_object_locked.url) {
            debug!("Dropping requested url: {} -> can't access (robots.txt)", &request_object_locked.url);
            return Crawlability::RestrictedByRobotsTxt;
        }

        Crawlability::Crawlable
    }

    async fn perform_crawl_internal(&self, http_client: Arc<dyn HttpClient>, task_context_uuid: Uuid, robots_txt_info_url: Option<String>) -> Result<Option<PageResponse>, Error> {
        let request_object_cloned = self.request_object.clone();
        let url = request_object_cloned.lock().unwrap().url.clone();
        request_object_cloned.lock().unwrap().task_context.lock().unwrap().get_all_tasked_links().lock().unwrap().push(url.clone());
        let raw_url = request_object_cloned.lock().unwrap().raw_url.clone();
        let mut page_response = PageResponse::new(url.clone(), raw_url, task_context_uuid.clone());
        let maximum_redirects = request_object_cloned.lock().unwrap().task_context.lock().unwrap().get_config().lock().unwrap().maximum_redirects;
        let ignore_redirects = request_object_cloned.lock().unwrap().task_context.lock().unwrap().get_config().lock().unwrap().ignore_redirects;
        let uri_service = request_object_cloned.lock().unwrap().task_context.lock().unwrap().get_uri_service();
        let fetch_header_response = self.fetch_header_command.fetch_header(url.clone(), ignore_redirects, maximum_redirects, uri_service, http_client, None, robots_txt_info_url.clone()).await;
        page_response = self.consume_fetch_header_response(robots_txt_info_url, request_object_cloned, page_response, fetch_header_response).await;

        page_response.response_timings.end_time = Some(DateTime::from(Utc::now()));
        Ok(Some(page_response))
    }

    async fn consume_fetch_header_response(&self, robots_txt_info_url: Option<String>, request_object: Arc<Mutex<PageRequest>>, mut page_response: PageResponse, fetch_header_response: HeadResponseResult) -> PageResponse {
        if let Ok(result) = fetch_header_response {
            let http_client = result.1;
            let fetch_header_response = result.0;
            let final_uri = fetch_header_response.get_final_uri();
            page_response.final_url_after_redirects = Some(final_uri.clone());

            let headers = &fetch_header_response.headers;
            let should_download = self.should_download_page(headers, &fetch_header_response.http_response_code);
            page_response.head = Some(fetch_header_response);

            if !should_download { return page_response; }

            let page_download_response = self.page_download_command.download_page(final_uri.clone(), http_client, robots_txt_info_url.clone()).await;
            page_response = self.consume_page_download_response(request_object, page_response, page_download_response);
        } else {
            page_response.crawl_status = Some(CrawlStatus::ConnectionError(fetch_header_response.err().unwrap().to_string()));
        }

        page_response
    }

    fn consume_page_download_response(&self, request_object: Arc<Mutex<PageRequest>>, mut page_response: PageResponse, page_download_response: Result<GetResponse, String>) -> PageResponse {
        if let Ok(download_result) = page_download_response {
            if self.is_html(&download_result.headers) {
                let request_object_locked = request_object.lock().unwrap();
                page_response.links = Self::extract_links(
                    request_object_locked.get_protocol(),
                    request_object_locked.get_host(),
                    download_result.body.as_ref(),
                    request_object_locked.task_context.lock().unwrap().get_dom_parser(),
                );
            }

            page_response.get = Some(download_result);
        } else {
            panic!("proper error handling needed")
        }

        page_response
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

    fn extract_links(protocol: String, host: String, body: Option<&String>, dom_parser: Arc<dyn DomParser>) -> Option<Vec<Link>> {
        if let Some(body_content) = body {
            let links = dom_parser.get_links(
                &protocol,
                &host,
                body_content);

            return match links {
                None => None,
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

    async fn crawl(&self, http_client: Arc<dyn HttpClient>, task_context_uuid: Uuid, robots_txt_info_url: Option<String>) -> Result<Option<PageResponse>, Error> {
        let status: Option<CrawlStatus>;

        match self.verify_crawlability() {
            Crawlability::AlreadyKnown | Crawlability::AlreadyTasked => return Ok(None),
            Crawlability::Crawlable => return self.perform_crawl_internal(http_client, task_context_uuid, robots_txt_info_url).await,
            Crawlability::RestrictedByRobotsTxt => status = Some(CrawlStatus::RestrictedByRobotsTxt),
            Crawlability::MaxDepthReached => status = Some(CrawlStatus::MaximumCrawlDepthReached),
        }

        let request_object_locked = self.request_object.lock().unwrap();
        let requested_url = request_object_locked.url.clone();
        let raw_url = request_object_locked.raw_url.clone();
        let mut response = PageResponse::new(requested_url, raw_url, task_context_uuid);
        response.crawl_status = status;
        response.response_timings.end_time = Some(DateTime::from(Utc::now()));
        return Ok(Some(response));
    }

    fn get_task_context(&self) -> Arc<Mutex<dyn FullTaskContext>> {
        self.request_object.lock().unwrap().task_context.clone()
    }

    fn get_current_depth(&self) -> u16 { self.current_depth }

    fn get_uuid_clone(&self) -> Uuid { self.uuid.clone() }
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

    use dom_parser::{DomParser, DomParserService};
    use linkresult::link_type_checker::LinkTypeChecker;
    use linkresult::uri_result::UriResult;
    use linkresult::uri_service::UriService;
    use responses::get_response::GetResponse;
    use responses::head_response::HeadResponse;
    use responses::redirect::Redirect;

    use crate::commands::page_crawl_command::{CrawlCommand, HeadResponseResult, PageCrawlCommand};
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
            fn get_all_crawled_links(&self) -> Arc<Mutex<Vec<String>>>;
            fn get_all_tasked_links(&self) -> Arc<Mutex<Vec<String>>>;
            fn add_crawled_link(&self, link: String);
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
            async fn fetch_header(&self, url: String, ignore_redirects:bool, maximum_redirects: u8, uri_service: Arc<UriService>, http_client: Arc<dyn HttpClient>, redirects: Option<Vec<Redirect>>, robots_txt_info_url: Option<String>) -> HeadResponseResult;
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
        assert_eq!(crawl_result.as_ref().unwrap().is_some(), true, "Should have result, if max depth reached");
        let crawl_result_unwrapped = crawl_result.unwrap().unwrap();
        assert_eq!(crawl_result_unwrapped.crawl_status.is_some(), true, "Should have crawl status, if max depth reached");
        assert_eq!(crawl_result_unwrapped.crawl_status.unwrap(), CrawlStatus::MaximumCrawlDepthReached, "Should have crawl status MaximumCrawlDepthReached, if max depth reached");
    }

    #[tokio::test]
    async fn will_crawl_if_max_depth_is_zero() {
        // given: a task context with maximum_depth = 0
        let url = String::from("https://example.com");
        let uri_service = Arc::new(UriService::new(Arc::new(LinkTypeChecker::new("example.com"))));
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_uri_service().return_const(uri_service.clone());
        mock_task_context.expect_get_url().return_const(url.clone());
        mock_task_context.expect_get_all_crawled_links().return_const(Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_get_all_tasked_links().return_const(Arc::new(Mutex::new(vec![])));
        let config = get_default_task_config();
        config.lock().unwrap().maximum_depth = 0;
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_can_access().returning(|_| true);
        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|_, _, _, _, _, _, _| Ok((HeadResponse::new(String::from("https://example.com"), StatusCode { code: hyper::StatusCode::IM_A_TEAPOT.as_u16(), label: hyper::StatusCode::IM_A_TEAPOT.canonical_reason().unwrap().into() }), get_mock_http_client())));
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
    async fn will_not_crawl_if_url_is_crawled() {
        // given: a task context with a known link
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_crawled_links().return_const(Arc::new(Mutex::new(vec![url.clone()])));
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
        assert_eq!(crawl_result.as_ref().unwrap().is_none(), true, "Should have no result, if url is known");
    }

    #[tokio::test]
    async fn will_crawl_if_url_is_uncrawled() {
        // given: a task context without the link known
        let url = String::from("https://example.com");
        let uri_service = Arc::new(UriService::new(Arc::new(LinkTypeChecker::new("example.com"))));
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_uri_service().return_const(uri_service.clone());
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_crawled_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_get_all_tasked_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| true);
        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|_, _, _, _, _, _, _| Ok((HeadResponse::new(String::from("https://example.com"), StatusCode { code: hyper::StatusCode::IM_A_TEAPOT.as_u16(), label: hyper::StatusCode::IM_A_TEAPOT.canonical_reason().unwrap().into() }), get_mock_http_client())));
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
    async fn will_not_crawl_if_url_is_tasked() {
        // given: a task context with a known link
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_crawled_links().return_const(Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_get_all_tasked_links().return_const(Arc::new(Mutex::new(vec![url.clone()])));
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
        assert_eq!(crawl_result.as_ref().unwrap().is_none(), true, "Should have no result, if url is tasked");
    }

    #[tokio::test]
    async fn will_crawl_if_url_is_untasked() {
        // given: a task context without the link known
        let url = String::from("https://example.com");
        let uri_service = Arc::new(UriService::new(Arc::new(LinkTypeChecker::new("example.com"))));
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_uri_service().return_const(uri_service.clone());
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_crawled_links().returning(|| Arc::new(Mutex::new(vec![])));
        let all_tasked_links = Arc::new(Mutex::new(vec![]));
        mock_task_context.expect_get_all_tasked_links().return_const(all_tasked_links.clone());
        mock_task_context.expect_can_access().returning(|_| true);
        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|_, _, _, _, _, _, _| Ok((HeadResponse::new(String::from("https://example.com"), StatusCode { code: hyper::StatusCode::IM_A_TEAPOT.as_u16(), label: hyper::StatusCode::IM_A_TEAPOT.canonical_reason().unwrap().into() }), get_mock_http_client())));
        let mock_page_download_command = Box::new(MockMyPageDownloadCommand::new());
        let mut mock_http_client = MockMyHttpClient::new();
        mock_http_client.expect_head().returning(|_, _| Ok(Response::builder()
            .status(200)
            .body(Body::from(""))
            .unwrap()));
        let mock_http_client = Arc::new(mock_http_client);

        // when: invoked with an untasked link
        let page_crawl_command = PageCrawlCommand::new(
            String::from("https://example.com"),
            String::from("https://example.com"),
            Arc::new(Mutex::new(mock_task_context)),
            1,
            mock_fetch_header_command,
            mock_page_download_command);
        let crawl_result = page_crawl_command.crawl(mock_http_client, Uuid::new_v4(), None).await;

        // then: expect some
        assert_eq!(crawl_result.as_ref().unwrap().is_some(), true, "Should crawl, if url is untasked");
        assert_eq!(crawl_result.as_ref().unwrap().as_ref().unwrap().response_timings.end_time.is_some(), true, "Should have end_time, regardless of status code");
        assert_eq!(page_crawl_command.get_task_context().as_ref().lock().unwrap()
                       .get_all_tasked_links().as_ref().lock().unwrap()
                       .contains(&String::from("https://example.com")), true, "Url should now be tasked");
    }

    #[tokio::test]
    async fn will_not_crawl_if_url_is_forbidden_by_robots_txt() {
        // given: a task context with robots_txt disallowing crawling
        let url = String::from("https://example.com");
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_crawled_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_get_all_tasked_links().returning(|| Arc::new(Mutex::new(vec![])));
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

        // then: expect CrawlStatus::RestrictedByRobotsTxt
        assert_eq!(crawl_result.as_ref().unwrap().is_some(), true, "Should have result for urls forbidden by robots.txt");
        let crawl_result_unwrapped = crawl_result.unwrap().unwrap();
        assert_eq!(crawl_result_unwrapped.crawl_status.is_some(), true, "Should have crawl_status for urls forbidden by robots.txt");
        assert_eq!(crawl_result_unwrapped.crawl_status.unwrap(), CrawlStatus::RestrictedByRobotsTxt, "Should have RestrictedByRobotsTxt for urls forbidden by robots.txt");
    }

    #[tokio::test]
    async fn returns_proper_page_response_on_successful_crawl() {
        // given: a task context that allows crawl
        let url = String::from("https://example.com");
        let uri_service = Arc::new(UriService::new(Arc::new(LinkTypeChecker::new("example.com"))));
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_uri_service().return_const(uri_service.clone());
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_crawled_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_get_all_tasked_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| true);
        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|_, _, _, _, _, _, _| Ok((HeadResponse::new(String::from("https://example.com"), StatusCode { code: hyper::StatusCode::IM_A_TEAPOT.as_u16(), label: hyper::StatusCode::IM_A_TEAPOT.canonical_reason().unwrap().into() }), get_mock_http_client())));
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
        let crawl_result_unwrapped = crawl_result.unwrap().unwrap();
        assert_eq!(crawl_result_unwrapped.head.is_some(), true, "Should have head, regardless of status code");
        assert_eq!(crawl_result_unwrapped.head.as_ref().unwrap().http_response_code.code, hyper::StatusCode::IM_A_TEAPOT.as_u16());
        assert_eq!(crawl_result_unwrapped.response_timings.end_time.is_some(), true, "Should have end_time, regardless of status code");
    }

    #[tokio::test]
    async fn returned_page_response_does_not_include_body_if_head_status_is_not_200() {
        // given: a task context that allows crawl
        let url = String::from("https://example.com");
        let uri_service = Arc::new(UriService::new(Arc::new(LinkTypeChecker::new("example.com"))));
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_uri_service().return_const(uri_service.clone());
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_crawled_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_get_all_tasked_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| true);
        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|_, _, _, _, _, _, _| Ok((HeadResponse::new(String::from("https://example.com"), StatusCode { code: hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16(), label: hyper::StatusCode::INTERNAL_SERVER_ERROR.canonical_reason().unwrap().into() }), get_mock_http_client())));
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
        let crawl_result_unwrapped = crawl_result.unwrap().unwrap();
        assert_eq!(crawl_result_unwrapped.get.is_none(), true, "Should not have get response, if status is not ok");
        assert_eq!(crawl_result_unwrapped.head.is_some(), true, "Should have head, regardless of status code");
        assert_eq!(crawl_result_unwrapped.head.as_ref().unwrap().http_response_code.code, hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16());
        assert_eq!(crawl_result_unwrapped.response_timings.end_time.is_some(), true, "Should have end_time, regardless of status code");
        let is_page_response_before_fetch_header_response = crawl_result_unwrapped
            .response_timings.start_time.as_ref().unwrap()
            .cmp(crawl_result_unwrapped
                .head.as_ref().unwrap()
                .response_timings.start_time.as_ref().unwrap());
        assert_eq!(is_page_response_before_fetch_header_response, Ordering::Less, "PageResponse start_time should be before HeadResponse start_time");
    }

    #[tokio::test]
    async fn does_not_download_page_if_content_type_is_not_text_html() {
        // given: a task context that allows crawl
        let url = String::from("https://example.com");
        let uri_service = Arc::new(UriService::new(Arc::new(LinkTypeChecker::new("example.com"))));
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_uri_service().return_const(uri_service.clone());
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_crawled_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_get_all_tasked_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| true);
        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|_, _, _, _, _, _, _| {
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
        let crawl_result_unwrapped = crawl_result.unwrap().unwrap();
        assert_eq!(crawl_result_unwrapped.get.is_none(), true, "Should not have get response, if status content-type is not text/html");
        assert_eq!(crawl_result_unwrapped.head.is_some(), true, "Should have head, regardless of status code");
    }

    #[tokio::test]
    async fn downloads_page_if_content_type_is_text_html() {
        // given: a task context that allows crawl
        let url = String::from("https://example.com");

        let uri_service = Arc::new(UriService::new(Arc::new(LinkTypeChecker::new("example.com"))));
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_uri_service().return_const(uri_service.clone());
        mock_task_context.expect_get_url().return_const(url.clone());

        let config = get_default_task_config();

        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_crawled_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_get_all_tasked_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| true);
        mock_task_context.expect_get_dom_parser().returning(|| {
            let mut dom_parser = MockMyDomParser::new();
            dom_parser.expect_get_links().returning(|_, _, _| None);
            Arc::new(dom_parser)
        });

        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|_, _, _, _, _, _, _| {
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
        let crawl_result_unwrapped = crawl_result.unwrap().unwrap();
        assert_eq!(crawl_result_unwrapped.get.as_ref().unwrap().body.is_some(), true, "Should have body, if status content-type is text/html");
        assert_eq!(crawl_result_unwrapped.get.as_ref().unwrap().body.as_ref().unwrap(), &String::from("<html><p>Hello World!</p></html>"), "Should have body, if status content-type is text/html");
        assert_eq!(crawl_result_unwrapped.head.is_some(), true, "Should have head, regardless of status code");
        assert_eq!(crawl_result_unwrapped.final_url_after_redirects.is_some(), true, "Should have final_url_after_redirects updated");
        assert_eq!(crawl_result_unwrapped.final_url_after_redirects.as_ref().unwrap(), "https://final-redirection.example.com", "Should have final_url_after_redirects set to requested url");
    }

    #[test]
    fn extract_links_invokes_dom_parser() {
        // given: a test body
        let body = String::from("<a href=\"https://www.example.com\">");
        let dom_parser = Arc::new(DomParserService::new(Arc::new(LinkTypeChecker::new("example.com"))));

        // when: extract_links is invoked
        let result = PageCrawlCommand::extract_links("https".into(), "example.com".into(), Some(&body), dom_parser);

        // then: result contains 1 link
        assert_eq!(result.is_some(), true, "Should contain a result");
        assert_eq!(result.unwrap().len(), 1, "Should contain exactly one link");
    }

    #[tokio::test]
    async fn returned_page_response_contains_correct_response_timings_on_max_depth_reached() {
        // given: a task context that allows crawl
        let url = String::from("https://example.com");
        let uri_service = Arc::new(UriService::new(Arc::new(LinkTypeChecker::new("example.com"))));
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_uri_service().return_const(uri_service.clone());
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        config.lock().unwrap().maximum_depth = 1;
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_crawled_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| true);
        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|_, _, _, _, _, _, _| Ok((HeadResponse::new(String::from("https://example.com"), StatusCode { code: hyper::StatusCode::OK.as_u16(), label: hyper::StatusCode::OK.canonical_reason().unwrap().into() }), get_mock_http_client())));
        let mock_page_download_command = Box::new(MockMyPageDownloadCommand::new());

        // when: invoked with a regular link
        let mut page_crawl_command = PageCrawlCommand::new(
            String::from("https://example.com"),
            String::from("https://example.com"),
            Arc::new(Mutex::new(mock_task_context)),
            1,
            mock_fetch_header_command,
            mock_page_download_command,
        );
        page_crawl_command.current_depth = 1;
        assert_eq!(page_crawl_command.verify_crawlability(), Crawlability::MaxDepthReached, "verify_crawlability should return MaxDepthReached");
        let mock_http_client = get_mock_http_client();
        let crawl_result = page_crawl_command.crawl(mock_http_client, Uuid::new_v4(), None).await;

        // then: expect some PageResponse with valid ResponseTimings
        assert_eq!(crawl_result.as_ref().unwrap().is_some(), true, "Should have a crawl result");
        let crawl_result_unwrapped = crawl_result.unwrap().unwrap();
        assert_eq!(crawl_result_unwrapped.get.is_none(), true, "Should not have get response, if maxDepth is reached");
        assert_eq!(crawl_result_unwrapped.head.is_none(), true, "Should not have head, if maxDepth is reached");
        assert_eq!(crawl_result_unwrapped.response_timings.start_time.is_some(), true, "Should have start_time, if maxDepth is reached");
        assert_eq!(crawl_result_unwrapped.response_timings.end_time.is_some(), true, "Should have end_time, if maxDepth is reached");
    }

    #[tokio::test]
    async fn unreachable_domains_return_error_message_in_proper_response() {
        // given: a task context that allows crawl
        let domain = "unreachable-domain.no";
        let url = format!("https://{}", domain);
        let uri_service = Arc::new(UriService::new(Arc::new(LinkTypeChecker::new(domain))));
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_get_uri_service().return_const(uri_service.clone());
        mock_task_context.expect_get_url().return_const(url.clone());
        let config = get_default_task_config();
        mock_task_context.expect_get_config().return_const(config.clone());
        mock_task_context.expect_get_all_crawled_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_get_all_tasked_links().returning(|| Arc::new(Mutex::new(vec![])));
        mock_task_context.expect_can_access().returning(|_| true);
        let mut mock_fetch_header_command = Box::new(MockMyFetchHeaderCommand::new());
        mock_fetch_header_command.expect_fetch_header().returning(|_, _, _, _, _, _, _| Err(String::from("Some nasty shit happened.")));
        let mock_page_download_command = Box::new(MockMyPageDownloadCommand::new());

        // when: invoked with a regular link
        let page_crawl_command = PageCrawlCommand::new(
            url.clone(),
            url.clone(),
            Arc::new(Mutex::new(mock_task_context)),
            0,
            mock_fetch_header_command,
            mock_page_download_command,
        );
        assert_eq!(page_crawl_command.verify_crawlability(), Crawlability::Crawlable, "verify_crawlability should return Crawlable");
        let mock_http_client = get_mock_http_client();
        let crawl_result = page_crawl_command.crawl(mock_http_client, Uuid::new_v4(), None).await;
        println!("{:?}", crawl_result);

        // then: expect some PageResponse with valid ResponseTimings
        assert_eq!(crawl_result.as_ref().unwrap().is_some(), true, "Should have a crawl result");
        let crawl_result_unwrapped = crawl_result.unwrap().unwrap();
        assert_eq!(crawl_result_unwrapped.get.is_none(), true, "Should not have get response, even if error occurred");
        assert_eq!(crawl_result_unwrapped.head.is_none(), true, "Should not have head, even if error occurred");
        assert_eq!(crawl_result_unwrapped.response_timings.start_time.is_some(), true, "Should have start_time, even if error occurred");
        assert_eq!(crawl_result_unwrapped.response_timings.end_time.is_some(), true, "Should have end_time, even if error occurred");
        assert_eq!(crawl_result_unwrapped.crawl_status.is_some(), true, "Should have crawl_status, if error occurred");
        assert_eq!(crawl_result_unwrapped.crawl_status.unwrap(), CrawlStatus::ConnectionError(String::from("Some nasty shit happened.")), "Should have crawl_status == ConnectionError, if error occurred");
    }
}