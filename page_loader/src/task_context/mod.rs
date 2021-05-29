use std::sync::{Arc, Mutex};

use hyper::Uri;

use dom_parser::DomParser;
use linkresult::LinkTypeChecker;
use linkresult::uri_service::UriService;

pub struct TaskContext {
    task_config: TaskConfig,
    dom_parser: Arc<DomParser>,
    link_type_checker: Arc<LinkTypeChecker>,
    uri_service: Arc<UriService>,
}

impl TaskContext {
    pub fn init(uri: String) -> TaskContext {
        let hyper_uri = uri.parse::<hyper::Uri>().unwrap();
        let link_type_checker = Arc::new(LinkTypeChecker::new(hyper_uri.host().unwrap()));
        let dom_parser = Arc::new(DomParser::new(link_type_checker.clone()));
        let uri_service = Arc::new(UriService::new(link_type_checker.clone()));
        TaskContext {
            task_config: TaskConfig::new(uri),
            dom_parser,
            link_type_checker,
            uri_service,
        }
    }
}

struct TaskConfig {
    uri: Uri,
    ignore_redirects: bool,
    maximum_redirects: u8,
    maximum_depth: u8,
    ignore_robots_txt: bool,
    keep_html_in_memory: bool,
    user_agent: String,
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
