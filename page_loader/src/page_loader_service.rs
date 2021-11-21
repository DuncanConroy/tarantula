use std::{fmt, thread};
use std::borrow::Borrow;
use std::cmp::max;
use std::fmt::Formatter;
use std::sync::{Arc, Mutex};

use log::{debug, error, warn};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::time::Instant;
use uuid::Uuid;

use responses::link::Link;
use responses::page_response::PageResponse;
use responses::run_config::RunConfig;
use responses::uri_scope::UriScope;

use crate::commands::fetch_header_command::DefaultFetchHeaderCommand;
use crate::commands::page_crawl_command::{CrawlCommand, PageCrawlCommand};
use crate::commands::page_download_command::DefaultPageDownloadCommand;
use crate::events::crawler_event::CrawlerEvent;
use crate::events::crawler_event::CrawlerEvent::PageEvent;
use crate::page_loader_service::PageLoaderServiceCommand::LoadPageCommand;
use crate::task_context::task_context::{DefaultTaskContext, FullTaskContext, TaskContextInit};
use crate::task_context_manager::{DefaultTaskManager, TaskManager};

pub trait CommandFactory: Sync + Send {
    fn create_page_crawl_command(&self, url: String, raw_url: String, task_context: Arc<Mutex<dyn FullTaskContext>>, current_depth: u16) -> Box<dyn CrawlCommand>;
}

pub struct PageCrawlCommandFactory;

impl PageCrawlCommandFactory {
    pub fn new() -> PageCrawlCommandFactory {
        PageCrawlCommandFactory {}
    }
}

impl CommandFactory for PageCrawlCommandFactory {
    fn create_page_crawl_command(&self, url: String, raw_url: String, task_context: Arc<Mutex<dyn FullTaskContext>>, current_depth: u16) -> Box<dyn CrawlCommand> {
        Box::new(PageCrawlCommand::new(url,
                                       raw_url,
                                       task_context,
                                       current_depth,
                                       Box::new(DefaultFetchHeaderCommand {}),
                                       Box::new(DefaultPageDownloadCommand {})))
    }
}

pub struct PageLoaderService {
    task_manager: Box<Arc<Mutex<dyn TaskManager>>>,
}

impl PageLoaderService {
    fn new() -> PageLoaderService {
        PageLoaderService {
            task_manager: Box::new(DefaultTaskManager::init(60_000)),
        }
    }

    pub fn init() -> Sender<PageLoaderServiceCommand> {
        PageLoaderService::init_with_factory(Box::new(PageCrawlCommandFactory::new()))
    }

    pub fn init_with_factory(page_crawl_command_factory: Box<dyn CommandFactory>) -> Sender<PageLoaderServiceCommand> {
        let buffer_size = max((num_cpus::get() / 2) * 10, 2);
        let (tx, mut rx) = mpsc::channel(buffer_size);
        let tx_clone = tx.clone();

        tokio::spawn(async move {
            let page_loader_service = PageLoaderService::new();

            let arc_command_factory = Arc::new(page_crawl_command_factory);
            while let Some(event) = rx.recv().await {
                match event {
                    PageLoaderServiceCommand::LoadPageCommand { url, raw_url, response_channel, task_context, current_depth } => {
                        PageLoaderService::handle_load_page_command(&tx_clone, arc_command_factory.clone(), url, raw_url, response_channel, task_context, current_depth);
                    }
                    PageLoaderServiceCommand::CrawlDomainCommand { run_config, response_channel, task_context_uuid, .. } => {
                        PageLoaderService::handle_crawl_domain_command(&tx_clone, &page_loader_service, run_config, response_channel, task_context_uuid).await;
                    }
                }
            }
            debug!("End of while loop >>PageLoaderService")
        });

        tx
    }

    async fn handle_crawl_domain_command(tx_clone: &Sender<PageLoaderServiceCommand>, page_loader_service: &PageLoaderService, run_config: RunConfig, response_channel: Sender<CrawlerEvent>, task_context_uuid: Uuid) {
        debug!("received CrawlDomainCommand with run_config: {:?} and uuid: {} on thread {:?}", run_config, task_context_uuid, thread::current().name());
        let default_task_context = DefaultTaskContext::init(run_config.clone(), task_context_uuid, response_channel.clone());
        let task_context = Arc::new(Mutex::new(default_task_context));
        tx_clone.send(LoadPageCommand { url: run_config.url.clone(), raw_url: run_config.url.clone(), response_channel, task_context: task_context.clone(), current_depth: 0 }).await.expect("Problem with spawned worker thread for CrawlDomainCommand");
        page_loader_service.task_manager.lock().unwrap().add_task(task_context);
    }

    fn handle_load_page_command(tx_clone: &Sender<PageLoaderServiceCommand>, arc_command_factory: Arc<Box<dyn CommandFactory>>, url: String, raw_url: String, response_channel: Sender<CrawlerEvent>, task_context: Arc<Mutex<dyn FullTaskContext>>, current_depth: u16) {
        debug!("received LoadPage command with url: {} (raw_url: {}) on thread {:?}, depth: {}", url, raw_url, thread::current().name(), current_depth);
        let tx_task = tx_clone.clone();
        let local_command_factory = arc_command_factory.clone();
        tokio::spawn(async move {
            let robots_txt_info_url = task_context.lock().unwrap().get_config().lock().unwrap().robots_txt_info_url.clone();
            let page_crawl_command = local_command_factory.create_page_crawl_command(url, raw_url, task_context, current_depth);
            do_load(response_channel, page_crawl_command, tx_task, robots_txt_info_url).await;
        });// Don't await here. Otherwise all processes might hang indefinitely
    }
}

async fn do_load(response_channel: Sender<CrawlerEvent>, page_crawl_command: Box<dyn CrawlCommand>, tx: Sender<PageLoaderServiceCommand>, robots_txt_info_url: Option<String>) {
    // updated last_command_received for garbage collection handling
    page_crawl_command.get_task_context().lock().unwrap().set_last_command_received(Instant::now());
    let url = page_crawl_command.get_url_clone();
    debug!("got url: {:?}", url);

    let http_client = page_crawl_command.get_task_context().lock().unwrap().get_http_client();
    let task_context_uuid = page_crawl_command.get_task_context().lock().unwrap().get_uuid_clone();
    let page_response = page_crawl_command.crawl(http_client, task_context_uuid, robots_txt_info_url).await;
    if let Ok(page_response_result) = page_response {
        if let Some(crawl_result) = page_response_result {
            consume_crawl_result(&response_channel, &page_crawl_command, &tx, crawl_result).await
        } else {
            debug!("Link skipped - already known");
        }
    } else {
        // todo!("Proper error handling is required!");
        error!("No page response from http call");
    }

    // dropping of these channels cannot be tested. therefore take double care with them!
    drop(tx);
    drop(response_channel);
}

async fn consume_crawl_result(response_channel: &Sender<CrawlerEvent>, page_crawl_command: &Box<dyn CrawlCommand>, tx: &Sender<PageLoaderServiceCommand>, crawl_result: PageResponse) {
    let task_context = page_crawl_command.get_task_context();
    add_links_to_known_list(&mut page_crawl_command.get_task_context().lock().unwrap()
        .get_all_crawled_links().lock().unwrap(), &crawl_result);
    let links = crawl_result.borrow().links.clone();
    let max_crawl_depth = task_context.lock().unwrap().get_config().lock().unwrap().maximum_depth;
    if links.is_some() && page_crawl_command.get_current_depth() <= max_crawl_depth {
        let mut links_deduped = links.unwrap();
        links_deduped.dedup_by(|a, b| a.uri.eq(&b.uri));
        let mut all_tasked_links = task_context.lock().unwrap().get_all_tasked_links().lock().unwrap().clone();
        let mut all_crawled_and_tasked_links = task_context.lock().unwrap().get_all_crawled_links().lock().unwrap().clone();
        all_crawled_and_tasked_links.append(&mut all_tasked_links);
        all_crawled_and_tasked_links.dedup();
        links_deduped.retain(|it| it.scope.is_some());
        for link in links_deduped {
            match link.scope.as_ref().unwrap() {
                UriScope::Root |
                UriScope::SameDomain |
                UriScope::DifferentSubDomain => {
                    let (url, load_page_command) = prepare_load_command(response_channel, &page_crawl_command, task_context.clone(), &link);

                    if !all_crawled_and_tasked_links.contains(&url) {
                        tx.send(load_page_command).await.expect(&format!("Issue sending LoadPage command to tx: {:?}", url.clone()));
                    }
                }
                _ => { continue; }
            }
        }
    }
    let send_result = response_channel.send(PageEvent { page_response: crawl_result }).await;
    if send_result.is_err() {
        warn!("Couldn't send PageResponse for PageCrawlCommand id {}", page_crawl_command.get_uuid_clone());
    } else {
        debug!("all_known_links: {}", page_crawl_command.get_task_context().lock().unwrap().get_all_crawled_links().lock().unwrap().len());
        debug!("all_tasked_links: {}", page_crawl_command.get_task_context().lock().unwrap().get_all_tasked_links().lock().unwrap().len());
    }
}

fn prepare_load_command(response_channel: &Sender<CrawlerEvent>, page_crawl_command: &Box<dyn CrawlCommand>, task_context: Arc<Mutex<dyn FullTaskContext>>, link: &Link) -> (String, PageLoaderServiceCommand) {
    let request = page_crawl_command.get_page_request();
    let protocol = request.lock().unwrap().get_protocol();
    let host = request.lock().unwrap().get_host();
    let url = task_context.lock().unwrap().get_uri_service().form_full_url(
        &protocol,
        &link.uri,
        &host,
        &Some(page_crawl_command.get_url_clone()),
    ).to_string();

    let resp = response_channel.clone();
    let load_page_command = LoadPageCommand { url: url.clone(), raw_url: link.uri.clone(), response_channel: resp, task_context: task_context.clone(), current_depth: page_crawl_command.get_current_depth() + 1 };
    (url, load_page_command)
}

fn add_links_to_known_list(all_known_links: &mut Vec<String>, crawl_result: &PageResponse) {
    if !all_known_links.contains(&crawl_result.original_requested_url) {
        all_known_links.push(crawl_result.original_requested_url.clone());
    }
    if let Some(final_url) = &crawl_result.final_url_after_redirects {
        if !all_known_links.contains(&final_url) {
            all_known_links.push(final_url.clone());
        }
    }
}

#[derive(Clone)]
pub enum PageLoaderServiceCommand {
    LoadPageCommand {
        url: String,
        raw_url: String,
        response_channel: mpsc::Sender<CrawlerEvent>,
        task_context: Arc<Mutex<dyn FullTaskContext>>,
        current_depth: u16,
    },
    CrawlDomainCommand {
        run_config: RunConfig,
        response_channel: mpsc::Sender<CrawlerEvent>,
        task_context_uuid: Uuid,
        last_crawled_timestamp: u64,
    },
}

impl fmt::Debug for PageLoaderServiceCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &*self {
            #[allow(unused_variables)] // allowing, as this is the signature
            PageLoaderServiceCommand::LoadPageCommand { url, raw_url, response_channel, task_context, current_depth } => f.debug_struct("LoadPageCommand")
                .field("url", &url)
                .field("raw_url", &raw_url)
                .field("current_depth", &current_depth)
                .finish(),
            #[allow(unused_variables)] // allowing, as this is the signature
            PageLoaderServiceCommand::CrawlDomainCommand { run_config, response_channel, task_context_uuid, last_crawled_timestamp } => f.debug_struct("CrawlDomainCommand")
                .field("run_config", &run_config)
                .field("task_context_uuid", &task_context_uuid)
                .field("last_crawled_timestamp", &last_crawled_timestamp)
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use hyper::Error;
    use uuid::Uuid;

    use responses::link::Link;

    use crate::http::http_client::HttpClient;
    use crate::page_loader_service::PageLoaderServiceCommand::{CrawlDomainCommand, LoadPageCommand};
    use crate::page_request::PageRequest;
    use crate::task_context::task_context::{DefaultTaskContext, TaskContext, TaskContextInit};

    use super::*;

    struct StubPageCrawlCommand {
        url: String,
        task_context: Arc<Mutex<dyn FullTaskContext>>,
        page_request: Arc<Mutex<PageRequest>>,
        uuid: Uuid,
    }

    impl StubPageCrawlCommand {
        fn new(url: String, response_channel: Sender<CrawlerEvent>) -> StubPageCrawlCommand {
            let task_context = create_default_task_context(response_channel);
            let page_request = Arc::new(Mutex::new(PageRequest::new(url.clone(), url.clone(), None, task_context.clone())));
            StubPageCrawlCommand { url, task_context, page_request, uuid: Uuid::new_v4() }
        }
    }

    #[async_trait]
    impl CrawlCommand for StubPageCrawlCommand {
        fn get_url_clone(&self) -> String {
            self.url.clone()
        }

        fn get_uuid_clone(&self) -> Uuid { self.uuid.clone() }

        fn get_page_request(&self) -> Arc<Mutex<PageRequest>> {
            self.page_request.clone()
        }

        #[allow(unused_variables)] // allowing, as we don't use http_client in this stub
        async fn crawl(&self, http_client: Arc<dyn HttpClient>, task_context_uuid: Uuid, robots_txt_info_url: Option<String>) -> std::result::Result<Option<PageResponse>, Error> {
            let mut response = PageResponse::new(self.url.clone(), self.url.clone(), Uuid::new_v4());
            if !self.url.starts_with("https://example.com/inner") {
                // if this is the initial crawl, we want to emulate additional links`
                response.links = Some(vec![
                    Link::from_str_with_scope("https://example.com/inner1", Some(UriScope::SameDomain)),
                    Link::from_str_with_scope("https://example.com/inner2", Some(UriScope::SameDomain)),
                    Link::from_str_with_scope("https://example.com/inner3", Some(UriScope::SameDomain)),
                    Link::from_str_with_scope("https://example.com/inner4", Some(UriScope::SameDomain)),
                    Link::from_str_with_scope("https://example.com/inner5", Some(UriScope::SameDomain)),
                    Link::from_str_with_scope("https://example.com/inner6", Some(UriScope::SameDomain)),
                    Link::from_str_with_scope("https://example.com/inner7", Some(UriScope::SameDomain)),
                    Link::from_str_with_scope("https://example.com/inner8", Some(UriScope::SameDomain)),
                    Link::from_str_with_scope("https://example.com/inner9", Some(UriScope::SameDomain)),
                    Link::from_str_with_scope("https://example.com/inner10", Some(UriScope::SameDomain)),
                ]);
            }
            Ok(Some(response))
        }

        fn get_task_context(&self) -> Arc<Mutex<dyn FullTaskContext>> {
            self.task_context.clone()
        }

        fn get_current_depth(&self) -> u16 { 1 }
    }

    struct StubFactory;

    impl CommandFactory for StubFactory {
        #[allow(unused)] // necessary, because we're stubbing this and not actually using everything that is provided by the trait signature
        fn create_page_crawl_command(&self, url: String, raw_url: String, task_context: Arc<Mutex<dyn FullTaskContext>>, current_depth: u16) -> Box<dyn CrawlCommand> {
            let response_channel = task_context.lock().unwrap().get_response_channel().clone();
            let mut command = StubPageCrawlCommand::new(url, response_channel);
            command.task_context = task_context;
            Box::new(command)
        }
    }

    fn create_default_task_context(response_channel: Sender<CrawlerEvent>) -> Arc<Mutex<DefaultTaskContext>> {
        Arc::new(Mutex::new(DefaultTaskContext::init(RunConfig::new(String::from("https://example.com"), None), Uuid::new_v4(), response_channel)))
    }

    #[tokio::test]
    async fn creates_task_for_crawl_domain_command() {
        // can we actually check for the task_manager?

        // given
        let stub_page_crawl_command_factory = StubFactory {};
        let tx = PageLoaderService::init_with_factory(Box::new(stub_page_crawl_command_factory));
        let (resp_tx, mut resp_rx) = mpsc::channel(1);

        // when
        // NOTE: use "/inner" in the url to trick the StubPageCrawlCommand
        let send_result = tx.send(CrawlDomainCommand { run_config: RunConfig::new(String::from("https://example.com/inner"), None), response_channel: resp_tx.clone(), task_context_uuid: Uuid::new_v4(), last_crawled_timestamp: 0 }).await;

        // then
        assert_eq!(true, send_result.is_ok());
        let expected_result = PageResponse::new("https://example.com/inner".into(), "/inner".into(), Uuid::new_v4());
        if let CrawlerEvent::PageEvent { page_response: actual_result } = resp_rx.recv().await.unwrap() {
            assert_eq!(expected_result.original_requested_url, actual_result.original_requested_url);
        } else {
            panic!("Wrong type!");
        }
    }

    #[tokio::test]
    async fn starts_working_on_receiving_load_page_command() {
        // given
        let stub_page_crawl_command_factory = StubFactory {};
        let tx = PageLoaderService::init_with_factory(Box::new(stub_page_crawl_command_factory));
        let (resp_tx, mut resp_rx) = mpsc::channel(2);
        let task_context = create_default_task_context(resp_tx.clone());

        // when
        // NOTE: use "/inner" in the url to trick the StubPageCrawlCommand
        let send_result = tx.send(LoadPageCommand { url: String::from("https://example.com/inner"), raw_url: String::from("/inner"), response_channel: resp_tx.clone(), task_context: task_context.clone(), current_depth: 0 }).await;

        // then
        assert_eq!(true, send_result.is_ok());
        let expected_result = PageResponse::new("https://example.com/inner".into(), "inner".into(), Uuid::new_v4());
        if let CrawlerEvent::PageEvent { page_response: actual_result } = resp_rx.recv().await.unwrap() {
            assert_eq!(expected_result.original_requested_url, actual_result.original_requested_url);
        } else {
            panic!("Wrong type");
        }
    }

    #[tokio::test]
    async fn on_receiving_load_page_command_task_contexts_last_command_received_is_updated() {
        // given
        let stub_page_crawl_command_factory = StubFactory {};
        let tx = PageLoaderService::init_with_factory(Box::new(stub_page_crawl_command_factory));
        let (resp_tx, mut resp_rx) = mpsc::channel(2);
        let task_context = create_default_task_context(resp_tx.clone());
        let initial_last_command_received_instant = task_context.lock().unwrap().get_last_command_received();

        // when
        let _send_result = tx.send(LoadPageCommand { url: String::from("https://example.com"), raw_url: String::from("/"), response_channel: resp_tx.clone(), task_context: task_context.clone(), current_depth: 0 }).await;

        // then
        // need to wait for the channel result first...
        let _actual_result = resp_rx.recv().await.unwrap();
        let updated_last_command_received_instant = task_context.lock().unwrap().get_last_command_received();
        assert_ne!(updated_last_command_received_instant, initial_last_command_received_instant);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn triggers_additional_load_commands_for_subpages() {
        // given
        let stub_page_crawl_command_factory = StubFactory {};
        let tx = PageLoaderService::init_with_factory(Box::new(stub_page_crawl_command_factory));
        let (resp_tx, mut resp_rx) = mpsc::channel(2);
        let task_context = create_default_task_context(resp_tx.clone());

        // when
        let send_result = tx.send(LoadPageCommand { url: String::from("https://example.com"), raw_url: String::from("/"), response_channel: resp_tx.clone(), task_context: task_context.clone(), current_depth: 0 }).await;

        // then
        assert_eq!(true, send_result.is_ok());
        let mut expected_results = vec![PageResponse::new("https://example.com".into(), "/".into(), Uuid::new_v4())];
        for i in 1..=10 {
            expected_results.push(PageResponse::new(format!("https://example.com/inner{}", i), format!("/inner{}", i), Uuid::new_v4()));
        }

        let mut actual_results = vec![];
        for _ in 0..expected_results.len() {
            if let CrawlerEvent::PageEvent { page_response: actual_result } = resp_rx.recv().await.unwrap() {
                let expected_result = expected_results
                    .drain_filter(|it: &mut PageResponse| it.original_requested_url.eq(&actual_result.original_requested_url));
                // println!("Got {:?}", actual_result);
                assert_eq!(expected_result.count(), 1);
                actual_results.push(actual_result);
            } else {
                panic!("Wrong type");
            }
        }

        assert_eq!(expected_results.len(), 0);
    }
}
