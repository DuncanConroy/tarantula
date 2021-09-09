use hyper::{Body, Client, Request};
use hyper_tls::HttpsConnector;
use rocket::serde::{Deserialize, json::Json};
use rocket::tokio;
use rocket::tokio::sync::mpsc;

use page_loader::page_loader_service::Command::CrawlDomainCommand;
use page_loader::page_loader_service::PageLoaderService;

#[derive(Clone, Debug, Deserialize)]
pub struct RunConfig {
    pub url: String,
    pub ignore_redirects: bool,
    pub maximum_redirects: u8,
    pub maximum_depth: u8,
    pub ignore_robots_txt: bool,
    pub keep_html_in_memory: bool,
    pub user_agent: String,
    pub callback_url: Option<String>,
}

impl RunConfig {
    pub fn new(url: String, callback_url: Option<String>) -> RunConfig {
        RunConfig {
            url,
            ignore_redirects: false,
            maximum_redirects: 10,
            maximum_depth: 16,
            ignore_robots_txt: false,
            keep_html_in_memory: false,
            user_agent: String::from("tarantula"),
            callback_url,
        }
    }
}

#[put("/crawl", data = "<run_config>")]
pub fn crawl(run_config: Json<RunConfig>) -> &'static str {
    tokio::spawn(process(run_config.0));
    "OK"
}

async fn process(run_config:RunConfig) {
    let num_cpus = num_cpus::get();
    let tx = PageLoaderService::init();
    let (resp_tx, mut resp_rx) = mpsc::channel(num_cpus * 2);

    let send_result = tx.send(CrawlDomainCommand { url: run_config.url.clone(), last_crawled_timestamp: 0, response_channel: resp_tx.clone() }).await;
    let connector = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(connector);

    let manager = tokio::spawn(async move {
        let mut responses = 0;
        while let Some(page_response) = resp_rx.recv().await {
            let page_response_json = rocket::serde::json::serde_json::to_string(&page_response).unwrap();
            info!("Received from threads: {:?}", page_response_json.clone());
            responses = responses + 1;
            info!(". -> {}", responses);

            if let Some(callback_url) = run_config.callback_url.clone() {
                let req = Request::builder()
                    .header("user-agent", run_config.user_agent.clone())
                    .method("POST")
                    .uri(callback_url)
                    .body(Body::from(page_response_json))
                    .expect(&format!("POST request builder"));
                client.request(req).await;
            }
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