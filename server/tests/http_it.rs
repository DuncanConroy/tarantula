use rocket::http::Status;
use rocket::local::asynchronous::Client;
use rocket::serde::json::serde_json;

use page_loader::page_loader_service::PageLoaderService;
use responses::run_config::RunConfig;

#[rocket::async_test]
async fn valid_request_responses_with_task_uuid() {
    let page_loader_tx_channel = PageLoaderService::init();
    let rocket = server::http::rocket(page_loader_tx_channel);
    let client = Client::tracked(rocket).await.unwrap();
    let task = RunConfig::new("https://foo".into(), None);
    let mut req = client.put("/crawl");
    req.set_body(&serde_json::to_string(&task).unwrap());
    let response = req.dispatch().await;
    assert_eq!(response.status().code, Status::Accepted.code);

    let response_body = response.into_string().await.unwrap();
    println!("{:?}", response_body);
    assert_eq!(response_body.len(), 36);
}