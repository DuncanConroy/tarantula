use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use uuid::Uuid;

use page_loader::page_loader_service::{CommandFactory, PageCrawlCommandFactory};
use page_loader::task_context::task_context::{DefaultTaskContext, TaskContextInit};
use responses::run_config::RunConfig;

#[tokio::test]
async fn invalid_urls_will_still_send_response() {
    let url = String::from("https://unreachable-domain.no");
    let channel = mpsc::channel(1);
    let uuid = Uuid::new_v4();
    let task_context = Arc::new(Mutex::new(DefaultTaskContext::init(RunConfig::new(url.clone(), None), uuid.clone(), channel.0)));
    let crawl_command = PageCrawlCommandFactory::new().create_page_crawl_command(url.clone(), url.clone(), task_context.clone(), 0);
    let http_client = crawl_command.get_task_context().lock().unwrap().get_http_client();
    let result = crawl_command.crawl(http_client, uuid.clone(), None).await;

    println!("TEST: page_crawl_command_it::invalid_urls_will_still_send_response -> {:?}", result);
    assert_eq!(result.is_ok(), true, "Should have result for unreachable domains");
    assert_eq!(result.as_ref().unwrap().as_ref().unwrap().crawl_status.is_some(), true, "Should have crawl_status for unreachable domains");
    let error_message = format!("{:?}", result.unwrap().unwrap().crawl_status.unwrap());
    assert_eq!(error_message.contains("error trying to connect"), true, "Should contain error message for unreachable domains");
}