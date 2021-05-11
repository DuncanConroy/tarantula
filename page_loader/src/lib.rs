// Event-driven page loader

use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};
use tokio::sync::mpsc::{Receiver, Sender};

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
        self.mpsc_sender = Some(tx);

        let manager = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    Command::LoadPage { url, response_channel } => {
                        println!("got url: {:?}", url);
                        response_channel.send(url);
                    }
                }
            }
        });

        self.mpsc_sender.as_ref().unwrap().clone()
    }
}

pub enum Command {
    LoadPage {
        url: String,
        response_channel: oneshot::Sender<String>,
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use crate::*;
    use crate::Command::LoadPage;

    #[tokio::test]
    async fn starts_working_on_receiving_command() {
        let mut page_loader_service = PageLoaderService::new();
        let tx = page_loader_service.init();
        let (resp_tx, resp_rx) = oneshot::channel();
        let send_result = tx.send(LoadPage { url: String::from("https://example.com"), response_channel: resp_tx }).await;
        assert_eq!(true, send_result.is_ok());

        assert_eq!("https://example.com", resp_rx.await.unwrap());
    }
}
