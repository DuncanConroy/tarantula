use std::thread;

use log::debug;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

use crate::page_loader_service::Command::LoadPage;

pub struct PageLoaderService {
    mpsc_sender: Option<Sender<Command>>,
}

impl PageLoaderService {
    fn new() -> PageLoaderService {
        PageLoaderService {
            mpsc_sender: None,
        }
    }

    fn init(&mut self) -> Sender<Command> {
        let buffer_size = num_cpus::get() / 2;
        let (tx, mut rx) = mpsc::channel(buffer_size);
        let tx_clone = tx.clone();
        self.mpsc_sender = Some(tx);

        let manager = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    Command::LoadPage { url, response_channel, .. } => {
                        debug!("received LoadPage command with url: {} on thread {:?}", url, thread::current().name());
                        let tx_task = tx_clone.clone();
                        tokio::spawn(async move {
                            do_load(response_channel, url, tx_task).await
                        }).await;
                    }
                }
            }
        });


        self.mpsc_sender.as_ref().unwrap().clone()
    }
}

async fn do_load(response_channel: Sender<String>, url: String, tx: Sender<Command>) {
    debug!("got url: {:?}", url);
    response_channel.send(url.clone()).await.expect("Could not send result to response channel");
    if url != String::from("https://inner") {
        tx.send(LoadPage {
            url: String::from("https://inner"),
            last_crawled_timestamp: 0,
            response_channel: response_channel.clone(),
        }).await;
    }
}

pub enum Command {
    LoadPage {
        url: String,
        last_crawled_timestamp: u64,
        response_channel: mpsc::Sender<String>,
    }
}

#[cfg(test)]
mod tests {
    use crate::page_loader_service::*;
    use crate::page_loader_service::Command::LoadPage;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn starts_working_on_receiving_command() {
        let mut page_loader_service = PageLoaderService::new();
        let tx = page_loader_service.init();
        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        let send_result = tx.send(LoadPage { url: String::from("https://example.com"), last_crawled_timestamp: 0, response_channel: resp_tx.clone() }).await;

        assert_eq!(true, send_result.is_ok());
        assert_eq!("https://example.com", resp_rx.recv().await.unwrap());
        assert_eq!("https://inner", resp_rx.recv().await.unwrap());
    }
}
