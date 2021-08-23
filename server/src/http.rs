use rocket::serde::{Deserialize, json::{Json, json, Value}};

#[derive(Clone, Debug, Deserialize)]
pub struct RunConfig {
    pub url: String,
    pub ignore_redirects: bool,
    pub maximum_redirects: u8,
    pub maximum_depth: u8,
    pub ignore_robots_txt: bool,
    pub keep_html_in_memory: bool,
    pub user_agent: String,
}

impl RunConfig {
    pub fn new(url: String) -> RunConfig {
        RunConfig {
            url,
            ignore_redirects: false,
            maximum_redirects: 10,
            maximum_depth: 16,
            ignore_robots_txt: false,
            keep_html_in_memory: false,
            user_agent: String::from("tarantula"),
        }
    }
}

#[put("/crawl", format = "json", data = "<run_config>")]
pub fn crawl(run_config: Json<RunConfig>) -> &'static str {
    "Hello, world!"
}

#[get("/crawl")]
pub fn crawl_get() -> &'static str {
    "Det er feil!"
}

#[catch(404)]
fn not_found() -> Value {
    json!({
        "status": "error",
        "reason": "You're an idiot!"
    })
}