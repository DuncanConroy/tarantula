use chrono::{DateTime, Utc};

use responses::link::Link;

#[derive(Debug)]
pub struct UriResult {
    pub parse_complete_time: DateTime<Utc>,
    pub links: Vec<Link>,
}
