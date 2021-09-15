use std::ops::Deref;

use hyper::{Body, Client, Request};
use hyper_tls::HttpsConnector;
use rocket::{State, tokio};
use rocket::response::status;
use rocket::serde::json::Json;
use rocket::tokio::sync::mpsc;
use rocket::tokio::sync::mpsc::Sender;
use uuid::Uuid;

use page_loader::events::crawler_event::CrawlerEvent;
use page_loader::page_loader_service::Command;
use page_loader::page_loader_service::Command::CrawlDomainCommand;
use responses::complete_response::CompleteResponse;
use responses::run_config::RunConfig;

#[put("/crawl", data = "<run_config>")]
pub fn crawl(run_config: Json<RunConfig>, page_loader_tx_channel: &State<Sender<Command>>) -> status::Accepted<String> {
    let task_context_uuid = Uuid::new_v4();
    tokio::spawn(process(run_config.0, task_context_uuid.clone(), page_loader_tx_channel.deref().deref().clone()));
    status::Accepted(Some(format!("{}", task_context_uuid)))
}

async fn process(run_config: RunConfig, task_context_uuid: Uuid, page_loader_tx_channel: Sender<Command>) {
    let num_cpus = num_cpus::get();
    let (resp_tx, mut resp_rx) = mpsc::channel(num_cpus * 2);

    let _send_result = page_loader_tx_channel.send(CrawlDomainCommand { run_config: run_config.clone(), task_context_uuid, last_crawled_timestamp: 0, response_channel: resp_tx.clone() }).await;
    let connector = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(connector);

    let manager = tokio::spawn(async move {
        let mut responses = 0;
        while let Some(event) = resp_rx.recv().await {
            let payload: String;
            let do_break: bool;
            match event {
                CrawlerEvent::PageEvent { page_response } => {
                    let page_response_json = rocket::serde::json::serde_json::to_string(&page_response).unwrap();
                    info!("Received from threads - PageEvent: {:?}", page_response);
                    responses = responses + 1;
                    info!(". -> {}", responses);

                    payload = page_response_json;
                    do_break = false;
                }
                CrawlerEvent::CompleteEvent { uuid } => {
                    let complete_response = CompleteResponse { uuid };
                    info!("Received from threads - CompleteEvent: {:?}", complete_response);
                    payload = rocket::serde::json::serde_json::to_string(&complete_response).unwrap();
                    do_break = true;
                }
            }

            if let Some(callback_url) = run_config.callback_url.clone() {
                let req = Request::builder()
                    .header("user-agent", run_config.user_agent.as_ref().unwrap().clone())
                    .method("POST")
                    .uri(callback_url)
                    .body(Body::from(payload))
                    .expect(&format!("POST request builder"));
                client.request(req).await.expect("Couldn't send request to callback");
            };

            if do_break { break; }
        }
    });

    manager.await.unwrap();

    info!("Finished.");
}

// use rocket_contrib::json::{Json, JsonError};
// use rocket_contrib::json::JsonValue;
//
// #[derive(Serialize, Deserialize)]
// struct PostChecklist {
//     pub title: String,
//     pub description: String,
//     pub color: Option<String>,
// }
//
// #[post("/system/<system_id>", data = "<checklist>")]
// fn create_for_system(checklist: Result<Json<PostChecklist>, JsonError>, system_id: i32, user: &User, connection: DbConn) -> Result<Json<JsonValue>, CustomResponder> {
//     match checklist {
//         Ok(checklist_item) => {
//             let system = match user.parent_user {
//                 None => (System::by_usergroup_and_id(&system_id, &user.usergroup, &connection.0)),
//                 Some(_) => (System::by_user_and_id(&system_id, &user.id.unwrap(), &connection.0))
//             };
//             match system {
//                 Some(system) => {
//                     let checklist = Checklist {
//                         usergroup: user.usergroup,
//                         system_id: Some(system.id.unwrap()),
//                         title: checklist_item.title.clone(),
//                         description: checklist_item.description.clone(),
//                         is_template: false,
//                         crdate: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
//                         color: checklist_item.color.clone(),
//                         ..Default::default()
//                     };
//                     let created_checklist = Checklist::create(checklist, &connection.0).unwrap();
//                     Ok(Json(json!({"data": created_checklist,"status": {"code": 200,"text": "api.checklist.created"}})))
//                 }
//                 None => return Err(CustomResponder::InternalServerError(Json(json!({ "status": {"code": 500, "text": "api.systems.system_not_found" }}))))
//             }
//         }
//         Err(jsonerror) => {
//             let errorstring = match jsonerror {
//                 JsonError::Io(_) => { String::from("") }
//                 JsonError::Parse(_, e) => { e.to_string() }
//             };
//             Err(CustomResponder::UnprocessableEntity(Json(json!({"status": {"code": 422,"text": errorstring}}))))
//         }
//     }