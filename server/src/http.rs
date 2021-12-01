use std::ops::Deref;

use hyper::{Body, Client, Request};
use hyper_tls::HttpsConnector;
use rocket::{Build, Rocket, State, tokio};
use rocket::response::status;
use rocket::serde::json::Json;
use rocket::tokio::sync::mpsc;
use rocket::tokio::sync::mpsc::Sender;
use uuid::Uuid;

use page_loader::events::crawler_event::CrawlerEvent;
use page_loader::page_loader_service::PageLoaderServiceCommand;
use page_loader::page_loader_service::PageLoaderServiceCommand::CrawlDomainCommand;
use responses::complete_response::CompleteResponse;
use responses::run_config::RunConfig;

pub fn rocket(page_loader_tx_channel: Sender<PageLoaderServiceCommand>) -> Rocket<Build> {
    rocket::build()
        .mount("/", routes![crawl])
        .manage(page_loader_tx_channel)
}

#[put("/crawl", data = "<run_config>")]
pub fn crawl(run_config: Json<RunConfig>, page_loader_tx_channel: &State<Sender<PageLoaderServiceCommand>>) -> status::Accepted<String> {
    let task_context_uuid = Uuid::new_v4();
    tokio::spawn(process(run_config.0, task_context_uuid.clone(), page_loader_tx_channel.deref().deref().clone()));
    status::Accepted(Some(format!("{}", task_context_uuid)))
}

async fn process(run_config: RunConfig, task_context_uuid: Uuid, page_loader_tx_channel: Sender<PageLoaderServiceCommand>) {
    let num_cpus = num_cpus::get();
    let (resp_tx, mut resp_rx) = mpsc::channel(num_cpus * 2);
    if let Ok(_) = page_loader_tx_channel.send(CrawlDomainCommand {
        run_config: run_config.clone(),
        task_context_uuid,
        last_crawled_timestamp: 0,
        response_channel: resp_tx,
    }).await {
        let connector = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(connector);

        drop(page_loader_tx_channel);
        let mut responses = 0;
        let mut callback_url = run_config.callback_url.clone();
        while let Some(event) = resp_rx.recv().await {
            let payload: String;
            let do_break: bool;
            match event {
                CrawlerEvent::PageEvent { page_response } => {
                    let page_response_json = rocket::serde::json::serde_json::to_string(&page_response).unwrap();
                    info!("Received from threads - PageEvent: {:?}, numLinks: {}", page_response.final_url_after_redirects.as_ref(), page_response.links.as_ref().unwrap_or(&vec![]).len());
                    responses = responses + 1;
                    info!(". -> {}", responses);

                    payload = page_response_json;
                    drop(page_response);
                    do_break = false;
                }
                CrawlerEvent::CompleteEvent { uuid } => {
                    let complete_response = CompleteResponse { uuid };
                    info!("Received from threads - CompleteEvent: {:?}", complete_response);
                    payload = rocket::serde::json::serde_json::to_string(&complete_response).unwrap();
                    callback_url = run_config.callback_url_finished.clone();

                    drop(complete_response);
                    do_break = true;
                }
            }

            if let Some(callback_url_unwrapped) = callback_url.as_ref() {
                let req = Request::builder()
                    .header("user-agent", run_config.user_agent.as_ref().unwrap().clone())
                    .method("POST")
                    .uri(callback_url_unwrapped)
                    .body(Body::from(payload))
                    .expect(&format!("POST request builder"));
                client.request(req).await.expect("Couldn't send request to callback");
            } else {
                drop(payload);
            }

            if do_break { break; }
            drop(do_break);
        }
        // dropping of these channels cannot be tested. therefore take double care with them!
        resp_rx.close();
        drop(resp_rx);
    } else {
        panic!("Shit happened");
    }

    info!("Finished crawl.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn on_received_event_task_context_last_command_received_is_updated() {
        // given:
        todo!("write a test");
        // when:

        // then: last_command_received is updated on the task_context
    }
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