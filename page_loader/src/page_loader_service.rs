use std::sync::{Arc, Mutex};
use std::thread;

use log::debug;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::time::Instant;

use linkresult::UriProtocol;
use tarantula_core::core::page::Page;

use crate::commands::fetch_header_command::DefaultFetchHeaderCommand;
use crate::commands::page_crawl_command::{CrawlCommand, PageCrawlCommand};
use crate::http::http_client::HttpClientImpl;
use crate::page_loader_service::Command::LoadPage;
use crate::task_context::task_context::{DefaultTaskContext, FullTaskContext, TaskContextInit};
use crate::task_context_manager::{DefaultTaskManager, TaskManager};

pub trait CommandFactory: Sync + Send {
    fn create_page_crawl_command(&self, url: String, task_context: Arc<Mutex<dyn FullTaskContext>>, current_depth: u16) -> Box<dyn CrawlCommand>;
}

pub struct PageCrawlCommandFactory;

impl PageCrawlCommandFactory {
    pub fn new() -> PageCrawlCommandFactory {
        PageCrawlCommandFactory {}
    }
}

impl CommandFactory for PageCrawlCommandFactory {
    fn create_page_crawl_command(&self, url: String, task_context: Arc<Mutex<dyn FullTaskContext>>, current_depth: u16) -> Box<dyn CrawlCommand> {
        Box::new(PageCrawlCommand::new(url, task_context, current_depth, Box::new(DefaultFetchHeaderCommand {})))
    }
}

pub struct PageLoaderService {
    mpsc_sender: Option<Sender<Command>>,
    task_manager: Box<Arc<Mutex<dyn TaskManager>>>,
    // all_known_links/AppContext/TaskContext
    // services

    // angenommen wir packen hier eine hashmap hin mit domain -> known_links. wann koennen wir diese map wieder aufraeumen?
    // ggf. timestamp des letzten LoadPage commands mitspeichern irgendwo und wenn diff zu jetzt > 1-10 min, speicher freigeben.
    // das koennen wir wunderbar im taskcontext speichern und den dann irgendwann auslaufen lassen


    // CrawlDomainCommand -> legt task context in hashmap an
    // neuer garbage collection thread schaut regelmaessig, wann das letzte LoadPageCommand fuer den entpsrechenden task (uuid) angenommen wurde
    // und nach gewisser zeit (durchschnittscrawlzeit * 2) wird der taskcontext geschlossen

    // zwei response types (enum) im channel - 1. initial mit uuid, um mittels webserver zu returnen. 2. page_response zum senden an pubsub
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
        let buffer_size = num_cpus::get() / 2;
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
                    Command::LoadPage { url, response_channel, task_context, current_depth } => {
                        debug!("received LoadPage command with url: {} on thread {:?}, depth: {}", url, thread::current().name(), current_depth);
                        let tx_task = tx_clone.clone();
                        let local_command_factory = arc_command_factory.clone();
                        tokio::spawn(async move {
                            let mut page_crawl_command = local_command_factory.create_page_crawl_command(url, task_context, current_depth);
                            do_load(response_channel, page_crawl_command, tx_task).await
                        }).await.expect("Problem with spawned worker thread for LoadPageCommand");
                    }
                    Command::CrawlDomainCommand { url, response_channel, .. } => {
                        debug!("received CrawlDomainCommand with url: {} on thread {:?}", url, thread::current().name());
                        let task_context = Arc::new(Mutex::new(DefaultTaskContext::init(url.clone())));
                        arc_page_loader_service_clone.task_manager.lock().unwrap().add_task(task_context.clone());
                        tx_clone.send(LoadPage { url, response_channel, task_context: task_context.clone(), current_depth: 0 }).await;
                    }
                }
            }
        });

        arc_page_loader_service.mpsc_sender.as_ref().unwrap().clone()
    }
}

async fn do_load(response_channel: Sender<Page>, page_crawl_command: Box<dyn CrawlCommand>, tx: Sender<Command>) {
    // updated last_command_received for garbage collection handling
    page_crawl_command.get_task_context().lock().unwrap().set_last_command_received(Instant::now());

    let url = page_crawl_command.get_url_clone();
    debug!("got url: {:?}", url);
    // legacy
    // tarantula_core::core::init(RunConfig::new(url), response_channel.clone()).await;

    // new approach
    let http_client = Box::new(HttpClientImpl::new());
    if let Ok(Some(crawl_result)) = page_crawl_command.crawl(http_client).await {
        let task_context = page_crawl_command.get_task_context();
        if crawl_result.links.is_some() {
            for link in crawl_result.links.unwrap() {
                let resp = response_channel.clone();
                tx.send(LoadPage { url: String::from(link.uri), response_channel: resp, task_context: task_context.clone(), current_depth: page_crawl_command.get_current_depth() + 1 }).await;
            }
        }

        let page_result = Page::new_root(url.clone(), Some(UriProtocol::HTTPS));
        response_channel.send(page_result).await.expect("Could not send result to response channel");
    } else {
        todo!("Proper error handling is required!");
    }
}

pub enum Command {
    LoadPage {
        url: String,
        response_channel: mpsc::Sender<Page>,
        task_context: Arc<Mutex<dyn FullTaskContext>>,
        current_depth: u16,
    },
    CrawlDomainCommand {
        url: String,
        response_channel: mpsc::Sender<Page>,
        last_crawled_timestamp: u64,
    },
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use async_trait::async_trait;

    use linkresult::Link;

    use crate::http::http_client::HttpClient;
    use crate::page_loader_service::Command::{CrawlDomainCommand, LoadPage};
    use crate::page_response::PageResponse;
    use crate::task_context::task_context::{DefaultTaskContext, TaskContext, TaskContextInit};

    use super::*;

    fn create_default_task_context() -> Arc<Mutex<DefaultTaskContext>> {
        Arc::new(Mutex::new(DefaultTaskContext::init(String::from("https://example.com"))))
    }

    #[tokio::test]
    async fn creates_task_for_crawl_domain_command() {
        // can we actually check for the task_manager?

        // given
        let tx = PageLoaderService::init();
        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        let task_context = create_default_task_context();

        // when
        let send_result = tx.send(CrawlDomainCommand { url: String::from("https://example.com"), response_channel: resp_tx.clone(), last_crawled_timestamp: 0 }).await;

        // then
        assert_eq!(true, send_result.is_ok());
        let expected_result = Page::new_root("https://example.com".into(), Some(UriProtocol::HTTPS));
        let actual_result = resp_rx.recv().await.unwrap();
        assert_eq!(expected_result.link.uri, actual_result.link.uri);
    }

    #[tokio::test]
    async fn starts_working_on_receiving_load_page_command() {
        // given
        let tx = PageLoaderService::init();
        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        let task_context = create_default_task_context();

        // when
        let send_result = tx.send(LoadPage { url: String::from("https://example.com"), response_channel: resp_tx.clone(), task_context: task_context.clone(), current_depth: 0 }).await;

        // then
        assert_eq!(true, send_result.is_ok());
        let expected_result = Page::new_root("https://example.com".into(), Some(UriProtocol::HTTPS));
        let actual_result = resp_rx.recv().await.unwrap();
        assert_eq!(expected_result.link.uri, actual_result.link.uri);
    }

    #[tokio::test]
    async fn on_receiving_load_page_command_task_contexts_last_command_received_is_updated() {
        // given
        let tx = PageLoaderService::init();
        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        let task_context = create_default_task_context();
        let initial_last_command_received_instant = task_context.lock().unwrap().get_last_command_received();

        // when
        let send_result = tx.send(LoadPage { url: String::from("https://example.com"), response_channel: resp_tx.clone(), task_context: task_context.clone(), current_depth: 0 }).await;

        // then
        // need to wait for the channel result first...
        let actual_result = resp_rx.recv().await.unwrap();
        let updated_last_command_received_instant = task_context.lock().unwrap().get_last_command_received();
        assert_ne!(updated_last_command_received_instant, initial_last_command_received_instant);
    }

    struct StubPageCrawlCommand {
        url: String,
        task_context: Arc<Mutex<dyn FullTaskContext>>,
    }

    impl StubPageCrawlCommand {
        fn new(url: String) -> StubPageCrawlCommand {
            let task_context = Arc::new(Mutex::new(DefaultTaskContext::init(url.clone())));
            StubPageCrawlCommand { url, task_context }
        }
    }

    #[async_trait]
    impl CrawlCommand for StubPageCrawlCommand {
        fn get_url_clone(&self) -> String {
            self.url.clone()
        }

        async fn crawl(&self, http_client: Box<dyn HttpClient>) -> Result<Option<PageResponse>, String> {
            let mut response = PageResponse::new(self.url.clone());
            if self.url != "https://inner" {
                // if this is the initial crawl, we want to emulate additional links`
                response.links = Some(vec![Link::from_str("https://inner")]);
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
        fn create_page_crawl_command(&self, url: String, task_context: Arc<Mutex<dyn FullTaskContext>>, current_depth: u16) -> Box<dyn CrawlCommand> {
            Box::new(StubPageCrawlCommand::new(url))
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn triggers_additional_load_commands_for_subpages() {
        // given
        let stub_page_crawl_command_factory = StubFactory::new();
        let tx = PageLoaderService::init_with_factory(Box::new(stub_page_crawl_command_factory));
        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        let task_context = create_default_task_context();

        // when
        let send_result = tx.send(LoadPage { url: String::from("https://example.com"), response_channel: resp_tx.clone(), task_context: task_context.clone(), current_depth: 0 }).await;

        // then
        assert_eq!(true, send_result.is_ok());
        let expected_result = Page::new_root("https://example.com".into(), Some(UriProtocol::HTTPS));
        let actual_result = resp_rx.recv().await.unwrap();
        assert_eq!(expected_result.link.uri, actual_result.link.uri);
        let expected_result = Page::new_root("https://inner".into(), Some(UriProtocol::HTTPS));
        let actual_result = resp_rx.recv().await.unwrap();
        assert_eq!(expected_result.link.uri, actual_result.link.uri);
    }
}
