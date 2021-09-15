use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hyper::Uri;
use tokio::sync::mpsc::Sender;
use tokio::time::Instant;
use uuid::Uuid;

use dom_parser::{DomParser, DomParserService};
use linkresult::LinkTypeChecker;
use linkresult::uri_service::UriService;
use responses::run_config::RunConfig;

use crate::events::crawler_event::CrawlerEvent;
use crate::http::http_client::{HttpClient, HttpClientImpl};
use crate::task_context::robots_service::{RobotsService, RobotsTxt};

pub trait TaskContextInit {
    fn init(run_config: RunConfig, uuid: Uuid, response_channel: Sender<CrawlerEvent>) -> Self;
}

pub trait TaskContext: Sync + Send {
    fn get_uuid_clone(&self) -> Uuid;
    fn get_config(&self) -> Arc<Mutex<TaskConfig>>;
    fn get_url(&self) -> String;
    fn get_last_command_received(&self) -> Instant;
    fn set_last_command_received(&mut self, instant: Instant);
    fn can_be_garbage_collected(&self, gc_timeout_ms: u64) -> bool;
    fn get_response_channel(&self) -> Sender<CrawlerEvent>;
}

pub trait TaskContextServices: Sync + Send {
    fn get_uri_service(&self) -> Arc<UriService>;
    fn get_dom_parser(&self) -> Arc<dyn DomParser>;
    fn get_http_client(&self) -> Arc<dyn HttpClient>;
}

pub trait KnownLinks: Sync + Send {
    fn get_all_known_links(&self) -> Arc<Mutex<Vec<String>>>;
    fn add_known_link(&self, link: String);
}

pub trait FullTaskContext: TaskContext + TaskContextServices + KnownLinks + RobotsTxt {}

#[derive(Clone)]
pub struct DefaultTaskContext {
    task_config: Arc<Mutex<TaskConfig>>,
    dom_parser: Arc<dyn DomParser>,
    uri_service: Arc<UriService>,
    robots_service: Arc<dyn RobotsTxt>,
    http_client: Arc<dyn HttpClient>,
    uuid: Uuid,
    last_command_received: Instant,
    all_known_links: Arc<Mutex<Vec<String>>>,
    response_channel: Sender<CrawlerEvent>,
}

impl TaskContextInit for DefaultTaskContext {
    fn init(run_config: RunConfig, uuid: Uuid, response_channel: Sender<CrawlerEvent>) -> DefaultTaskContext {
        let hyper_uri = run_config.url.parse::<hyper::Uri>().unwrap();
        let task_config = Arc::new(Mutex::new(TaskConfig::new(run_config)));
        let user_agent = task_config.lock().unwrap().user_agent.clone();
        let crawl_delay_ms = task_config.lock().unwrap().crawl_delay_ms.clone();
        let link_type_checker = Arc::new(LinkTypeChecker::new(hyper_uri.host().unwrap()));
        let dom_parser = Arc::new(DomParserService::new(link_type_checker.clone()));
        let uri_service = Arc::new(UriService::new(link_type_checker.clone()));
        let robots_service = Arc::new(RobotsService::new(user_agent.clone()));
        let http_client = Arc::new(HttpClientImpl::new(user_agent.clone(), crawl_delay_ms.clone()));
        let all_known_links = Arc::new(Mutex::new(vec![]));
        let last_command_received = Instant::now();
        DefaultTaskContext {
            task_config,
            dom_parser,
            uri_service,
            robots_service,
            http_client,
            uuid,
            last_command_received,
            all_known_links,
            response_channel,
        }
    }
}

impl TaskContext for DefaultTaskContext {
    fn get_uuid_clone(&self) -> Uuid {
        self.uuid.clone()
    }

    fn get_config(&self) -> Arc<Mutex<TaskConfig>> { self.task_config.clone() }

    fn get_url(&self) -> String { self.task_config.lock().unwrap().uri.to_string() }

    fn get_last_command_received(&self) -> Instant {
        self.last_command_received
    }

    fn set_last_command_received(&mut self, instant: Instant) {
        self.last_command_received = instant;
    }

    fn can_be_garbage_collected(&self, gc_timeout_ms: u64) -> bool {
        return if Instant::now() - self.last_command_received > Duration::from_millis(self.task_config.lock().unwrap().crawl_delay_ms as u64 + gc_timeout_ms) {
            true
        } else {
            false
        };
    }

    fn get_response_channel(&self) -> Sender<CrawlerEvent> {
        self.response_channel.clone()
    }
}

impl TaskContextServices for DefaultTaskContext {
    fn get_uri_service(&self) -> Arc<UriService> {
        self.uri_service.clone()
    }
    fn get_dom_parser(&self) -> Arc<dyn DomParser> { self.dom_parser.clone() }
    fn get_http_client(&self) -> Arc<dyn HttpClient> { self.http_client.clone() }
}

impl KnownLinks for DefaultTaskContext {
    fn get_all_known_links(&self) -> Arc<Mutex<Vec<String>>> {
        self.all_known_links.clone()
    }

    fn add_known_link(&self, link: String) {
        self.all_known_links.lock().unwrap().push(link);
    }
}

impl RobotsTxt for DefaultTaskContext {
    fn can_access(&self, item_uri: &str) -> bool {
        self.robots_service.clone().can_access(item_uri)
    }
}

impl FullTaskContext for DefaultTaskContext {}

#[derive(Clone, Debug)]
pub struct TaskConfig {
    pub uri: Uri,
    pub ignore_redirects: bool,
    pub maximum_redirects: u8,
    pub maximum_depth: u16,
    pub ignore_robots_txt: bool,
    pub keep_html_in_memory: bool,
    pub user_agent: String,
    pub crawl_delay_ms: usize,
}

impl TaskConfig {
    pub fn new(run_config: RunConfig) -> TaskConfig {
        TaskConfig {
            uri: run_config.url.parse::<hyper::Uri>().unwrap(),
            ignore_redirects: run_config.ignore_redirects.unwrap_or_else(|| false),
            maximum_redirects: run_config.maximum_redirects.unwrap_or_else(|| 10),
            maximum_depth: run_config.maximum_depth.unwrap_or_else(|| 16),
            ignore_robots_txt: run_config.ignore_robots_txt.unwrap_or_else(|| false),
            keep_html_in_memory: run_config.keep_html_in_memory.unwrap_or_else(|| false),
            user_agent: run_config.user_agent.unwrap_or_else(|| String::from("tarantula")),
            crawl_delay_ms: run_config.crawl_delay_ms.unwrap_or_else(|| 10_000),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use tokio::sync::mpsc;

    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn can_be_garbage_collected_false_by_default() {
        // given: a usual task context
        let gc_timeout_ms = 10;
        let (resp_tx, _) = mpsc::channel(2);
        let context = DefaultTaskContext::init(RunConfig::new("https://example.com".into(), None), Uuid::new_v4(), resp_tx);

        // when: can_be_garbage_collected is invoked
        let result = context.can_be_garbage_collected(gc_timeout_ms);

        // then: expect false
        assert_eq!(result, false, "TaskContext should not be garbage collectable at this point");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn can_be_garbage_collected_true_on_timeout() {
        // given: a usual task context
        let (resp_tx, _) = mpsc::channel(2);
        let mut run_config = RunConfig::new("https://example.com".into(), None);
        run_config.crawl_delay_ms = Some(20);
        let context = DefaultTaskContext::init(run_config.clone(), Uuid::new_v4(), resp_tx);
        let gc_timeout_ms = 10u64;

        // when: can_be_garbage_collected is invoked after crawl_delay_ms + gc_timeout_ms * 2
        thread::sleep(Duration::from_millis(run_config.crawl_delay_ms.unwrap() as u64 + gc_timeout_ms * 2u64));
        let result = context.can_be_garbage_collected(gc_timeout_ms);

        // then: expect true
        assert_eq!(result, true, "TaskContext should be garbage collectable at this point");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn can_be_garbage_collected_false_if_diff_lower_than_crawl_delay_ms() {
        // given: a usual task context
        let (resp_tx, _) = mpsc::channel(2);
        let mut run_config = RunConfig::new("https://example.com".into(), None);
        run_config.crawl_delay_ms = Some(40);
        let context = DefaultTaskContext::init(run_config, Uuid::new_v4(), resp_tx);
        let gc_timeout_ms = 10u64;

        // when: can_be_garbage_collected is invoked after gc_timeout_ms * 2
        thread::sleep(Duration::from_millis(gc_timeout_ms * 2u64));
        let result = context.can_be_garbage_collected(gc_timeout_ms);

        // then: expect true
        assert_eq!(result, false, "TaskContext should not be garbage collectable at this point");
    }
}