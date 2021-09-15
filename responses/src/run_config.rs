use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct RunConfig {
    pub url: String,
    pub ignore_redirects: Option<bool>,
    pub maximum_redirects: Option<u8>,
    pub maximum_depth: Option<u16>,
    pub ignore_robots_txt: Option<bool>,
    pub keep_html_in_memory: Option<bool>,
    pub user_agent: Option<String>,
    pub callback_url: Option<String>,
    pub crawl_delay_ms: Option<usize>,
}

impl RunConfig {
    pub fn new(url: String, callback_url: Option<String>) -> RunConfig {
        RunConfig {
            url,
            ignore_redirects: Some(false),
            maximum_redirects: Some(10),
            maximum_depth: Some(16),
            ignore_robots_txt: Some(false),
            keep_html_in_memory: Some(false),
            user_agent: Some(String::from("tarantula")),
            callback_url,
            crawl_delay_ms: Some(10_000),
        }
    }
}