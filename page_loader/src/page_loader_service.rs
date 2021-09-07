use std::{fmt, thread};
use std::borrow::Borrow;
use std::cmp::max;
use std::fmt::Formatter;
use std::sync::{Arc, Mutex};

use log::debug;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::time::Instant;

use linkresult::UriScope;

use crate::commands::fetch_header_command::DefaultFetchHeaderCommand;
use crate::commands::page_crawl_command::{CrawlCommand, PageCrawlCommand};
use crate::commands::page_download_command::DefaultPageDownloadCommand;
use crate::page_loader_service::Command::LoadPageCommand;
use crate::page_response::PageResponse;
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
    fn create_page_crawl_command(&self, url: String, raw_url:String, task_context: Arc<Mutex<dyn FullTaskContext>>, current_depth: u16) -> Box<dyn CrawlCommand> {
        Box::new(PageCrawlCommand::new(url,
                                       raw_url,
                                       task_context,
                                       current_depth,
                                       Box::new(DefaultFetchHeaderCommand {}),
                                       Box::new(DefaultPageDownloadCommand {})))
    }
}

pub struct PageLoaderService {
    mpsc_sender: Option<Sender<Command>>,
    task_manager: Box<Arc<Mutex<dyn TaskManager>>>,
}

impl PageLoaderService {
    fn new() -> PageLoaderService {
        PageLoaderService {
            mpsc_sender: None,
            task_manager: Box::new(DefaultTaskManager::init(60_000)),
        }
    }

    pub fn init() -> Sender<Command> {
        PageLoaderService::init_with_factory(Box::new(PageCrawlCommandFactory::new()))
    }

    pub fn init_with_factory(page_crawl_command_factory: Box<dyn CommandFactory>) -> Sender<Command> {
        let buffer_size = max(num_cpus::get() / 2, 2);
        let (tx, mut rx) = mpsc::channel(buffer_size);
        let tx_clone = tx.clone();

        let mut page_loader_service = PageLoaderService::new();
        page_loader_service.mpsc_sender = Some(tx);
        let arc_page_loader_service = Arc::new(page_loader_service);
        let arc_page_loader_service_clone = arc_page_loader_service.clone();

        let arc_command_factory = Arc::new(page_crawl_command_factory);

        let _manager = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    Command::LoadPageCommand { url, raw_url, response_channel, task_context, current_depth } => {
                        debug!("received LoadPage command with url: {} (raw_url: {}) on thread {:?}, depth: {}", url, raw_url, thread::current().name(), current_depth);
                        let tx_task = tx_clone.clone();
                        let local_command_factory = arc_command_factory.clone();
                        tokio::spawn(async move {
                            let page_crawl_command = local_command_factory.create_page_crawl_command(url, raw_url, task_context, current_depth);
                            do_load(response_channel, page_crawl_command, tx_task).await
                        });// Don't await here. Otherwise all processes might hang indefinitely
                    }
                    Command::CrawlDomainCommand { url, response_channel, .. } => {
                        debug!("received CrawlDomainCommand with url: {} on thread {:?}", url, thread::current().name());
                        let task_context = Arc::new(Mutex::new(DefaultTaskContext::init(url.clone())));
                        arc_page_loader_service_clone.task_manager.lock().unwrap().add_task(task_context.clone());
                        tx_clone.send(LoadPageCommand { url: url.clone(), raw_url:url, response_channel, task_context: task_context.clone(), current_depth: 0 }).await.expect("Problem with spawned worker thread for CrawlDomainCommand");
                    }
                }
            }
            debug!("End of while loop >>PageLoaderService")
        });

        arc_page_loader_service.mpsc_sender.as_ref().unwrap().clone()
    }
}

async fn do_load(response_channel: Sender<PageResponse>, page_crawl_command: Box<dyn CrawlCommand>, tx: Sender<Command>) {
    // updated last_command_received for garbage collection handling
    page_crawl_command.get_task_context().lock().unwrap().set_last_command_received(Instant::now());
    let url = page_crawl_command.get_url_clone();
    debug!("got url: {:?}", url);

    let http_client = page_crawl_command.get_task_context().lock().unwrap().get_http_client();
    let page_response = page_crawl_command.crawl(http_client).await;
    if let Ok(page_response_result) = page_response {
        if let Some(crawl_result) = page_response_result {
            add_links_to_known_list(&page_crawl_command, &crawl_result);
            let links = crawl_result.borrow().links.clone();
            let task_context = page_crawl_command.get_task_context();
            if links.is_some() {
                let mut links_deduped = links.unwrap();
                links_deduped.dedup_by(|a, b| a.uri.eq(&b.uri));
                for link in links_deduped {
                    // todo!("TEST")
                    if link.scope.is_none() { continue; }

                    match link.scope.as_ref().unwrap() {
                        UriScope::Root |
                        UriScope::SameDomain |
                        UriScope::DifferentSubDomain => {
                            let request = page_crawl_command.get_page_request();
                            let protocol = request.lock().unwrap().get_protocol();
                            let host = request.lock().unwrap().get_host();
                            drop(request);
                            let url = task_context.lock().unwrap().get_uri_service().form_full_url(
                                &protocol,
                                &String::from(link.uri.clone()),
                                &host,
                                &Some(page_crawl_command.get_url_clone()),
                            ).to_string();


                            let resp = response_channel.clone();
                            let load_page_command = LoadPageCommand { url: url.clone(), raw_url: link.uri.clone() , response_channel: resp, task_context: task_context.clone(), current_depth: page_crawl_command.get_current_depth() + 1 };
                            tx.send(load_page_command).await.expect(&format!("Issue sending LoadPage command to tx: {:?}", url.clone()));
                        }
                        _ => { continue; }
                    }
                }
            }
            response_channel.send(crawl_result).await.expect("Could not send result to response channel");
        } else {
            // todo: send some response to response channel - we got nothing here :)
            // todo!("Proper error handling");
        }
    } else {
        todo!("Proper error handling is required!");
    }
}

fn add_links_to_known_list(page_crawl_command: &Box<dyn CrawlCommand>, crawl_result: &PageResponse) {
    // TODO: refactor to not use internals of page crawl command. use function on crawl command instead - decoupling
    page_crawl_command.get_task_context().lock().unwrap()
        .get_all_known_links().lock().unwrap()
        .push(crawl_result.original_requested_url.clone());
    if let Some(final_url) = &crawl_result.final_url_after_redirects {
        page_crawl_command.get_task_context().lock().unwrap()
            .get_all_known_links().lock().unwrap()
            .push(final_url.clone());
    }
}

#[derive(Clone)]
pub enum Command {
    LoadPageCommand {
        url: String,
        raw_url: String,
        response_channel: mpsc::Sender<PageResponse>,
        task_context: Arc<Mutex<dyn FullTaskContext>>,
        current_depth: u16,
    },
    CrawlDomainCommand {
        url: String,
        response_channel: mpsc::Sender<PageResponse>,
        last_crawled_timestamp: u64,
    },
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &*self {
            #[allow(unused_variables)] // allowing, as this is the signature
            Command::LoadPageCommand{url, raw_url, response_channel, task_context, current_depth} => f.debug_struct("LoadPageCommand")
                .field("url", &url)
                .field("raw_url", &raw_url)
                .field("current_depth", &current_depth)
                .finish(),
            #[allow(unused_variables)] // allowing, as this is the signature
            Command::CrawlDomainCommand{url, response_channel, last_crawled_timestamp} => f.debug_struct("CrawlDomainCommand")
                .field("url", &url)
                .field("last_crawled_timestamp", &last_crawled_timestamp)
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use linkresult::Link;

    use crate::http::http_client::HttpClient;
    use crate::page_loader_service::Command::{CrawlDomainCommand, LoadPageCommand};
    use crate::page_request::PageRequest;
    use crate::page_response::PageResponse;
    use crate::task_context::task_context::{DefaultTaskContext, TaskContext, TaskContextInit};

    use super::*;

    struct StubPageCrawlCommand {
        url: String,
        task_context: Arc<Mutex<dyn FullTaskContext>>,
        page_request: Arc<Mutex<PageRequest>>,
    }

    impl StubPageCrawlCommand {
        fn new(url: String) -> StubPageCrawlCommand {
            let task_context = create_default_task_context();
            let page_request = Arc::new(Mutex::new(PageRequest::new(url.clone(), url.clone(), None, task_context.clone())));
            StubPageCrawlCommand { url, task_context, page_request }
        }
    }

    #[async_trait]
    impl CrawlCommand for StubPageCrawlCommand {
        fn get_url_clone(&self) -> String {
            self.url.clone()
        }

        fn get_page_request(&self) -> Arc<Mutex<PageRequest>> {
            self.page_request.clone()
        }

        #[allow(unused_variables)] // allowing, as we don't use http_client in this stub
        async fn crawl(&self, http_client: Arc<dyn HttpClient>) -> std::result::Result<Option<PageResponse>, String> {
            let mut response = PageResponse::new(self.url.clone(), self.url.clone());
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

    impl StubFactory {
        fn new() -> StubFactory {
            StubFactory {}
        }
    }

    impl CommandFactory for StubFactory {
        #[allow(unused)] // necessary, because we're stubbing this and not actually using everything that is provided by the trait signature
        fn create_page_crawl_command(&self, url: String, raw_url: String, task_context: Arc<Mutex<dyn FullTaskContext>>, current_depth: u16) -> Box<dyn CrawlCommand> {
            let mut command = StubPageCrawlCommand::new(url);
            command.task_context = task_context;
            Box::new(command)
        }
    }

    fn create_default_task_context() -> Arc<Mutex<DefaultTaskContext>> {
        Arc::new(Mutex::new(DefaultTaskContext::init(String::from("https://example.com"))))
    }

    #[tokio::test]
    async fn creates_task_for_crawl_domain_command() {
        // can we actually check for the task_manager?

        // given
        let stub_page_crawl_command_factory = StubFactory::new();
        let tx = PageLoaderService::init_with_factory(Box::new(stub_page_crawl_command_factory));
        let (resp_tx, mut resp_rx) = mpsc::channel(1);

        // when
        // NOTE: use "/inner" in the url to trick the StubPageCrawlCommand
        let send_result = tx.send(CrawlDomainCommand { url: String::from("https://example.com/inner"), response_channel: resp_tx.clone(), last_crawled_timestamp: 0 }).await;

        // then
        assert_eq!(true, send_result.is_ok());
        let expected_result = PageResponse::new("https://example.com/inner".into(), "/inner".into());
        let actual_result = resp_rx.recv().await.unwrap();
        assert_eq!(expected_result.original_requested_url, actual_result.original_requested_url);
    }

    #[tokio::test]
    async fn starts_working_on_receiving_load_page_command() {
        // given
        let stub_page_crawl_command_factory = StubFactory::new();
        let tx = PageLoaderService::init_with_factory(Box::new(stub_page_crawl_command_factory));
        let (resp_tx, mut resp_rx) = mpsc::channel(2);
        let task_context = create_default_task_context();

        // when
        // NOTE: use "/inner" in the url to trick the StubPageCrawlCommand
        let send_result = tx.send(LoadPageCommand { url: String::from("https://example.com/inner"), raw_url: String::from("/inner"), response_channel: resp_tx.clone(), task_context: task_context.clone(), current_depth: 0 }).await;

        // then
        assert_eq!(true, send_result.is_ok());
        let expected_result = PageResponse::new("https://example.com/inner".into(), "inner".into());
        let actual_result = resp_rx.recv().await.unwrap();
        assert_eq!(expected_result.original_requested_url, actual_result.original_requested_url);
    }

    #[tokio::test]
    async fn on_receiving_load_page_command_task_contexts_last_command_received_is_updated() {
        // given
        let stub_page_crawl_command_factory = StubFactory::new();
        let tx = PageLoaderService::init_with_factory(Box::new(stub_page_crawl_command_factory));
        let (resp_tx, mut resp_rx) = mpsc::channel(2);
        let task_context = create_default_task_context();
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
        let stub_page_crawl_command_factory = StubFactory::new();
        let tx = PageLoaderService::init_with_factory(Box::new(stub_page_crawl_command_factory));
        let (resp_tx, mut resp_rx) = mpsc::channel(2);
        let task_context = create_default_task_context();

        // when
        let send_result = tx.send(LoadPageCommand { url: String::from("https://example.com"), raw_url: String::from("/"), response_channel: resp_tx.clone(), task_context: task_context.clone(), current_depth: 0 }).await;

        // then
        assert_eq!(true, send_result.is_ok());
        let mut expected_results = vec![PageResponse::new("https://example.com".into(), "/".into())];
        for i in 1..=10 {
            expected_results.push(PageResponse::new(format!("https://example.com/inner{}", i), format!("/inner{}",i)));
        }

        let mut actual_results = vec![];
        for _ in 0..expected_results.len() {
            let actual_result = resp_rx.recv().await.unwrap();
            let expected_result = expected_results
                .drain_filter(|it: &mut PageResponse| it.original_requested_url.eq(&actual_result.original_requested_url));
            println!("Got {:?}", actual_result);
            assert_eq!(expected_result.count(), 1);
            actual_results.push(actual_result);
        }

        assert_eq!(expected_results.len(), 0);
    }
}
