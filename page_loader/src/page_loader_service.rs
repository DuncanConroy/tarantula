use std::thread;

use log::debug;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

use core::core::page::Page;
use core::core::RunConfig;

use crate::page_loader_service::Command::LoadPage;
use crate::task_context;
use crate::commands::page_crawl_command::PageCrawlCommand;

pub struct PageLoaderService {
    mpsc_sender: Option<Sender<Command>>,
}

impl PageLoaderService {
    fn new() -> PageLoaderService {
        PageLoaderService {
            mpsc_sender: None,
        }
    }

    pub fn init() -> Sender<Command> {
        let buffer_size = num_cpus::get() / 2;
        let (tx, mut rx) = mpsc::channel(buffer_size);
        let tx_clone = tx.clone();

        let mut page_loader_service = PageLoaderService::new();
        page_loader_service.mpsc_sender = Some(tx);

        let manager = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    Command::LoadPage { url, response_channel, .. } => {
                        debug!("received LoadPage command with url: {} on thread {:?}", url, thread::current().name());
                        let tx_task = tx_clone.clone();
                        tokio::spawn(async move {
                            let page_crawl_command = PageCrawlCommand::new(url);
                            do_load(response_channel, page_crawl_command, tx_task).await
                        }).await;
                    }
                }
            }
        });


        page_loader_service.mpsc_sender.as_ref().unwrap().clone()
    }
}

// TODO: generischen response_channel mit anderem objekt
async fn do_load(response_channel: Sender<Ppage>, page_crawl_command:PageCrawlCommand, tx: Sender<Command>) {
    let url = page_crawl_command.request_object.url.clone();
    debug!("got url: {:?}", url);
    // let task_context = task_context::init(url);
    core::core::init(RunConfig::new(url), response_channel).await;
    // response_channel.send(url.clone()).await.expect("Could not send result to response channel");
    // if url != String::from("https://inner") {
    //     tx.send(LoadPage {
    //         url: String::from("https://inner"),
    //         last_crawled_timestamp: 0,
    //         response_channel: response_channel.clone(),
    //     }).await;
    // }
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
    use crate::page_loader_service::*;
    use crate::page_loader_service::Command::LoadPage;
    use crate::page_response::PageResponse;
    use mockall_double::*;
    #[double]
    use crate::commands::page_crawl_command::PageCrawlCommand;

    use mockall::*;
    use mockall::predicate::*;

    #[tokio::test]
    async fn starts_working_on_receiving_command() {
        let tx = PageLoaderService::init();
        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        let send_result = tx.send(LoadPage { url: String::from("https://example.com"), last_crawled_timestamp: 0, response_channel: resp_tx.clone() }).await;

        assert_eq!(true, send_result.is_ok());
        assert_eq!("https://example.com", resp_rx.recv().await.unwrap());
    }

    // #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    // async fn triggers_additional_load_commands_for_subpages(){
    //     let mut mock_page_crawl_command = PageCrawlCommand::new(String::from("https://example.com"));
    //     mock_page_crawl_command.expect_crawl()
    //         .returning(||PageResponse::new(String::from("https://inner")));
    //     let mut page_loader_service = PageLoaderService::new();
    //     let tx = page_loader_service.init();
    //     let (resp_tx, mut resp_rx) = mpsc::channel(1);
    //     let send_result = tx.send(LoadPage { url: String::from("https://example.com"), last_crawled_timestamp: 0, response_channel: resp_tx.clone() }).await;
    //
    //     assert_eq!(true, send_result.is_ok());
    //     assert_eq!("https://example.com", resp_rx.recv().await.unwrap());
    //     assert_eq!("https://inner", resp_rx.recv().await.unwrap());
    // }
}
