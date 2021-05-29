use std::sync::{Arc};
use std::thread;

use log::debug;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

use linkresult::UriProtocol;
use tarantula_core::core::page::Page;

use crate::commands::page_crawl_command::{CrawlCommand, PageCrawlCommand};
use crate::page_loader_service::Command::LoadPage;
use crate::task_context::TaskContext;

pub trait CommandFactory: Sync + Send {
    fn create_page_crawl_command(&self, url: String, task_context: Arc<dyn TaskContext>) -> Box<dyn CrawlCommand>;
}

pub struct PageCrawlCommandFactory;

impl PageCrawlCommandFactory {
    pub fn new() -> PageCrawlCommandFactory {
        PageCrawlCommandFactory {}
    }
}

impl CommandFactory for PageCrawlCommandFactory {
    fn create_page_crawl_command(&self, url: String, task_context: Arc<dyn TaskContext>) -> Box<dyn CrawlCommand> {
        Box::new(PageCrawlCommand::new(url, task_context))
    }
}

pub struct PageLoaderService {
    mpsc_sender: Option<Sender<Command>>,
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

        let arc_command_factory = Arc::new(page_crawl_command_factory);

        let _manager = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    Command::LoadPage { url, response_channel, task_context } => {
                        debug!("received LoadPage command with url: {} on thread {:?}", url, thread::current().name());
                        let tx_task = tx_clone.clone();
                        let local_command_factory = arc_command_factory.clone();
                        tokio::spawn(async move {
                            let page_crawl_command = local_command_factory.create_page_crawl_command(url, task_context);
                            do_load(response_channel, page_crawl_command, tx_task).await
                        }).await.expect("Problem with spawned worker thread for LoadPageCommand");
                    }
                    Command::CrawlDomainCommand { url, response_channel, .. } => {
                        debug!("received LoadPage command with url: {} on thread {:?}", url, thread::current().name());
                    }
                }
            }
        });

        page_loader_service.mpsc_sender.as_ref().unwrap().clone()
    }
}

async fn do_load(response_channel: Sender<Page>, page_crawl_command: Box<dyn CrawlCommand>, tx: Sender<Command>) {
    let url = page_crawl_command.get_url_clone();
    debug!("got url: {:?}", url);
    // legacy
    // tarantula_core::core::init(RunConfig::new(url), response_channel.clone()).await;

    // let task_context = task_context::init(url);

    // new approach
    let crawl_result = page_crawl_command.crawl();
    let task_context = page_crawl_command.get_task_context();
    if crawl_result.links.is_some() {
        for link in crawl_result.links.unwrap() {
            let resp = response_channel.clone();
            tx.send(LoadPage { url: String::from(link.uri), response_channel: resp, task_context: task_context.clone() }).await;
        }
    }

    let page_result = Page::new_root(url.clone(), Some(UriProtocol::HTTPS));
    response_channel.send(page_result).await.expect("Could not send result to response channel");
}

pub enum Command {
    LoadPage {
        url: String,
        response_channel: mpsc::Sender<Page>,
        task_context: Arc<dyn TaskContext>,
    },
    CrawlDomainCommand {
        url: String,
        response_channel: mpsc::Sender<Page>,
        last_crawled_timestamp: u64,
    },
}

#[cfg(test)]
mod tests {
    use linkresult::Link;

    use crate::commands::page_crawl_command::{CrawlCommand, PageCrawlCommand};
    use crate::page_loader_service::*;
    use crate::page_loader_service::Command::LoadPage;
    use crate::page_response::PageResponse;
    use crate::task_context::{DefaultTaskContext, TaskContextInit};

    #[tokio::test]
    async fn starts_working_on_receiving_command() {
        // given
        let tx = PageLoaderService::init();
        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        let task_context = Arc::new(DefaultTaskContext::init(String::from("https://example.com")));

        // when
        let send_result = tx.send(LoadPage { url: String::from("https://example.com"), response_channel: resp_tx.clone(), task_context: task_context.clone() }).await;

        // then
        assert_eq!(true, send_result.is_ok());
        let expected_result = Page::new_root("https://example.com".into(), Some(UriProtocol::HTTPS));
        let actual_result = resp_rx.recv().await.unwrap();
        assert_eq!(expected_result.link.uri, actual_result.link.uri);
    }

    struct StubPageCrawlCommand {
        url: String,
        task_context: Arc<dyn TaskContext>,
    }

    impl StubPageCrawlCommand {
        fn new(url: String) -> StubPageCrawlCommand {
            let task_context = Arc::new(DefaultTaskContext::init(url.clone()));
            StubPageCrawlCommand { url, task_context }
        }
    }

    impl CrawlCommand for StubPageCrawlCommand {
        fn get_url_clone(&self) -> String {
            self.url.clone()
        }

        fn crawl(&self) -> PageResponse {
            let mut response = PageResponse::new(self.url.clone());
            if self.url != "https://inner" {
                // if this is the initial crawl, we want to emulate additional links`
                response.links = Some(vec![Link::from_str("https://inner")]);
            }
            response
        }

        fn get_task_context(&self) -> Arc<dyn TaskContext> {
            self.task_context.clone()
        }
    }

    struct StubFactory;

    impl StubFactory {
        fn new() -> StubFactory {
            StubFactory {}
        }
    }

    impl CommandFactory for StubFactory {
        fn create_page_crawl_command(&self, url: String, task_context: Arc<dyn TaskContext>) -> Box<dyn CrawlCommand> {
            Box::new(StubPageCrawlCommand::new(url))
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn triggers_additional_load_commands_for_subpages() {
        // given
        let mut stub_page_crawl_command_factory = StubFactory::new();
        let tx = PageLoaderService::init_with_factory(Box::new(stub_page_crawl_command_factory));
        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        let task_context = Arc::new(DefaultTaskContext::init(String::from("https://example.com")));

        // when
        let send_result = tx.send(LoadPage { url: String::from("https://example.com"), response_channel: resp_tx.clone(), task_context: task_context.clone() }).await;

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
