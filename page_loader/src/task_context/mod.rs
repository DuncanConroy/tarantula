use std::fmt::Debug;
use std::sync::{Arc, Mutex};

use hyper::Uri;
use tokio::time::Instant;
use uuid::Uuid;

use dom_parser::DomParser;
use linkresult::LinkTypeChecker;
use linkresult::uri_service::UriService;

pub trait TaskContextInit {
    fn init(uri: String) -> Self;
}

pub trait TaskContext: Sync + Send + Debug {
    fn get_uuid_clone(&self) -> Uuid;
    fn get_config_clone(&self) -> TaskConfig;
    fn get_config_ref(&self) -> &TaskConfig;
    fn get_config_mut(&mut self) -> &mut TaskConfig;
    fn get_url(&self) -> String;
    fn get_last_load_page_command_received_instant(&self) -> Option<Instant>;
    fn can_be_garbage_collected(&self) -> bool;
}

pub trait KnownLinks: Sync + Send + Debug {
    fn get_all_known_links(&self) -> Arc<Mutex<Vec<String>>>;
    fn add_known_link(&self, link: String);
}

pub trait FullTaskContext: TaskContext + KnownLinks {}

#[derive(Clone, Debug)]
pub struct DefaultTaskContext {
    task_config: TaskConfig,
    dom_parser: Arc<DomParser>,
    link_type_checker: Arc<LinkTypeChecker>,
    uri_service: Arc<UriService>,
    uuid: Uuid,
    last_load_page_command_received_instant: Option<Instant>,
    all_known_links: Arc<Mutex<Vec<String>>>,
}

impl TaskContextInit for DefaultTaskContext {
    fn init(uri: String) -> DefaultTaskContext {
        let hyper_uri = uri.parse::<hyper::Uri>().unwrap();
        let link_type_checker = Arc::new(LinkTypeChecker::new(hyper_uri.host().unwrap()));
        let dom_parser = Arc::new(DomParser::new(link_type_checker.clone()));
        let uri_service = Arc::new(UriService::new(link_type_checker.clone()));
        let uuid = Uuid::new_v4();
        let all_known_links = Arc::new(Mutex::new(vec![]));
        DefaultTaskContext {
            task_config: TaskConfig::new(uri),
            dom_parser,
            link_type_checker,
            uri_service,
            uuid,
            last_load_page_command_received_instant: None,
            all_known_links,
        }
    }
}

impl TaskContext for DefaultTaskContext {
    fn get_uuid_clone(&self) -> Uuid {
        self.uuid.clone()
    }

    fn get_config_clone(&self) -> TaskConfig {
        self.task_config.clone()
    }

    fn get_config_ref(&self) -> &TaskConfig { &self.task_config }

    fn get_config_mut(&mut self) -> &mut TaskConfig { &mut self.task_config }

    fn get_url(&self) -> String { self.task_config.uri.to_string() }

    fn get_last_load_page_command_received_instant(&self) -> Option<Instant> {
        self.last_load_page_command_received_instant
    }

    fn can_be_garbage_collected(&self) -> bool {
        todo!()
    }
}

impl KnownLinks for DefaultTaskContext {
    fn get_all_known_links(&self) -> Arc<Mutex<Vec<String>>> {
        self.all_known_links.clone()
    }

    fn add_known_link(&self, link: String) {
        self.all_known_links.lock().unwrap().push(link);
    }
}

impl FullTaskContext for DefaultTaskContext {}

#[derive(Clone, Debug)]
pub struct TaskConfig {
    pub uri: Uri,
    pub ignore_redirects: bool,
    pub maximum_redirects: u16,
    pub maximum_depth: u16,
    pub ignore_robots_txt: bool,
    pub keep_html_in_memory: bool,
    pub user_agent: String,
}

impl TaskConfig {
    fn new(uri: String) -> TaskConfig {
        TaskConfig {
            uri: uri.parse::<hyper::Uri>().unwrap(),
            ignore_redirects: false,
            maximum_redirects: 10,
            maximum_depth: 16,
            ignore_robots_txt: false,
            keep_html_in_memory: false,
            user_agent: String::from("tarantula"),
        }
    }
}
