use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::thread;

use log::debug;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

use linkresult::UriProtocol;
use tarantula_core::core::page::Page;
use tarantula_core::core::RunConfig;

use crate::commands::page_crawl_command::{CrawlCommand, PageCrawlCommand};
use crate::page_loader_service::Command::LoadPage;
use crate::task_context;

pub trait CommandFactory: Send {
    fn create_page_crawl_command(&self, url: String) -> Box<dyn CrawlCommand>;
}

pub struct PageCrawlCommandFactory;

impl PageCrawlCommandFactory {
    pub fn new() -> PageCrawlCommandFactory {
        PageCrawlCommandFactory {}
    }
}

impl CommandFactory for PageCrawlCommandFactory {
    fn create_page_crawl_command(&self, url: String) -> Box<dyn CrawlCommand> {
        Box::new(PageCrawlCommand::new(url))
    }
}

pub struct PageLoaderService {
    mpsc_sender: Option<Sender<Command>>,
    // all_known_links/AppContext/TaskContext
    // services
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

        let arc_command_factory = Arc::new(Mutex::new(page_crawl_command_factory));

        let _manager = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    Command::LoadPage { url, response_channel, .. } => {
                        debug!("received LoadPage command with url: {} on thread {:?}", url, thread::current().name());
                        let tx_task = tx_clone.clone();
                        let local_command_factory = arc_command_factory.clone();
                        tokio::spawn(async move {
                            let page_crawl_command = local_command_factory.lock().unwrap().create_page_crawl_command(url);
                            do_load(response_channel, page_crawl_command, tx_task).await
                        }).await.expect("Problem with spawned worker thread for LoadPageCommand");
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
    if crawl_result.links.is_some() {
        for link in crawl_result.links.unwrap() {
            let resp = response_channel.clone();
            tx.send(LoadPage { url: String::from(link.uri), response_channel: resp, last_crawled_timestamp: 0 }).await;
        }
    }

    let page_result = Page::new_root(url.clone(), Some(UriProtocol::HTTPS));
    response_channel.send(page_result).await.expect("Could not send result to response channel");
}

pub enum Command {
    LoadPage {
        url: String,
        last_crawled_timestamp: u64,
        response_channel: mpsc::Sender<Page>,
    }
}

#[cfg(test)]
mod tests {
    use linkresult::Link;

    use crate::commands::page_crawl_command::{CrawlCommand, PageCrawlCommand};
    use crate::page_loader_service::*;
    use crate::page_loader_service::Command::LoadPage;
    use crate::page_response::PageResponse;

    #[tokio::test]
    async fn starts_working_on_receiving_command() {
        // given
        let tx = PageLoaderService::init();
        let (resp_tx, mut resp_rx) = mpsc::channel(1);

        // when
        let send_result = tx.send(LoadPage { url: String::from("https://example.com"), last_crawled_timestamp: 0, response_channel: resp_tx.clone() }).await;

        // then
        assert_eq!(true, send_result.is_ok());
        let expected_result = Page::new_root("https://example.com".into(), Some(UriProtocol::HTTPS));
        let actual_result = resp_rx.recv().await.unwrap();
        assert_eq!(expected_result.link.uri, actual_result.link.uri);
    }

    struct StubPageCrawlCommand {
        url: String,
    }

    impl StubPageCrawlCommand {
        fn new(url: String) -> StubPageCrawlCommand {
            StubPageCrawlCommand { url }
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
    }

    struct StubFactory;

    impl StubFactory {
        fn new() -> StubFactory {
            StubFactory {}
        }
    }

    impl CommandFactory for StubFactory {
        fn create_page_crawl_command(&self, url: String) -> Box<dyn CrawlCommand> {
            Box::new(StubPageCrawlCommand::new(url))
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn triggers_additional_load_commands_for_subpages() {
        // given
        let mut stub_page_crawl_command_factory = StubFactory::new();
        let tx = PageLoaderService::init_with_factory(Box::new(stub_page_crawl_command_factory));
        let (resp_tx, mut resp_rx) = mpsc::channel(1);

        // when
        let send_result = tx.send(LoadPage { url: String::from("https://example.com"), last_crawled_timestamp: 0, response_channel: resp_tx.clone() }).await;

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
