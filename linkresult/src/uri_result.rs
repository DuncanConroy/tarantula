use scraper::Node;
use chrono::{DateTime, Utc};

#[derive(Debug)]
pub struct ResponseTimings {
    pub request_start_time: DateTime<Utc>,
    pub request_complete_time: Option<DateTime<Utc>>,
    pub request_connection_confirmed_time: Option<DateTime<Utc>>,
    pub parse_complete_time: Option<DateTime<Utc>>,
}

impl ResponseTimings {
    fn set_complete_time(&mut self, time: DateTime<Utc>) {
        self.request_complete_time = Some(time);
    }

    fn set_request_connection_confirmed_time(&mut self, time:DateTime<Utc>) {
        self.request_connection_confirmed_time = Some(time);
    }

    fn set_parse_complete_time(&mut self, time: DateTime<Utc>){
        self.parse_complete_time = Some(time);
    }
}

#[derive(Debug, Clone)]
pub struct Link {
    pub uri: String,
    pub scope: Option<UriScope>,
    pub protocol: Option<UriProtocol>,
    pub source_tag: Option<Node>,
}

impl Link {
    pub fn from_str(s: &str) -> Link {
        Link {
            uri: s.to_string(),
            scope: None,
            protocol: None,
            source_tag: None,
        }
    }
}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        self.uri == other.uri
    }

    fn ne(&self, other: &Self) -> bool {
        !self.eq(other)
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum UriProtocol {
    // http://example.com/bar
    HTTP,
    // https://example.com/bar
    HTTPS,
    // //example.com/bar
    IMPLICIT,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum UriScope {
    // /
    Root,
    // example.com/deeplink | deeplink | /deeplink
    SameDomain,
    // diffsub.example.com/deeplink
    DifferentSubDomain,
    // https://www.end-of-the-internet.com/
    External,
    // #somewhere
    Anchor,
    // mailto:foo.bar@example.com
    Mailto,
    // data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGP6AgAA+gD3odZZSQAAAABJRU5ErkJggg==
    EmbeddedImage,
    // javascript:function foo(){}
    Code,
    // somespecial:anycode
    UnknownPrefix,
}


#[derive(Debug)]
pub struct UriResult {
    pub parent: Option<Link>,
    pub response_timings: ResponseTimings,
    pub links: Vec<Link>,
}
