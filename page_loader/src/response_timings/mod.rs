use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct ResponseTimings {
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub name: String,
}

impl ResponseTimings {
    pub fn new(name: String) -> ResponseTimings {
        ResponseTimings {
            start_time: Some(DateTime::from(Utc::now())),
            end_time: None,
            name,
        }
    }

    pub fn from(name: String, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ResponseTimings {
        ResponseTimings {
            start_time: Some(start_time),
            end_time: Some(end_time),
            name,
        }
    }
}