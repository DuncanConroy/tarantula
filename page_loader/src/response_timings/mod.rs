use chrono::{DateTime, Utc};
use serde::{Serialize, Serializer};
use serde::ser::SerializeStruct;

#[derive(Debug, Clone)]
pub struct ResponseTimings {
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub name: String,
}

impl Serialize for ResponseTimings {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("ResponseTimings", 3)?;
        s.serialize_field("start_time", &self.start_time.ok_or("None").unwrap().to_string())?;
        s.serialize_field("end_time", &self.end_time.ok_or("None").unwrap().to_string())?;
        s.serialize_field("name", &self.name)?;
        s.end()
    }
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