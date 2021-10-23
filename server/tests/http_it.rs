use rocket::form::FromForm;
use rocket::local::asynchronous::Client;

use page_loader::page_loader_service::{PageLoaderService, PageLoaderServiceCommand};
use responses::run_config::RunConfig;

#[rocket::async_test]
async fn valid_request_responses_with_task_uuid() {
    let page_loader_tx_channel = PageLoaderService::init();
    let rocket = server::http::rocket(page_loader_tx_channel);
    let client = Client::tracked(rocket).await.unwrap();
    // let task = RunConfig::new("https://foo".into(), None);
    let mut req = client.put("/crawl");
    // req.set_body(task);
    let response = req.dispatch().await;

    let response_body = response.body();
    assert_eq!(response_body.is_some(), true);
    // assert_eq!(response.status().code, 200);
    println!("{:?}", response_body);

    // TODO: extend tests
}