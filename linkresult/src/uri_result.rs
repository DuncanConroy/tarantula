use chrono::{DateTime, Utc};
use scraper::Node;

#[derive(Debug, Clone)]
pub struct ResponseTimings {
    pub overall_start_time: DateTime<Utc>,
    pub overall_complete_time: Option<DateTime<Utc>>,
    pub parse_complete_time: Option<DateTime<Utc>>,
    pub head_request_start_time: Option<DateTime<Utc>>,
    pub head_request_complete_time: Option<DateTime<Utc>>,
    pub get_request_start_time: Option<DateTime<Utc>>,
    pub get_request_complete_time: Option<DateTime<Utc>>,
    pub children_compete_time: Option<DateTime<Utc>>,
}

impl ResponseTimings {
    pub fn new() -> ResponseTimings {
        ResponseTimings {
            overall_start_time: Utc::now(),
            overall_complete_time: None,
            parse_complete_time: None,
            head_request_start_time: None,
            head_request_complete_time: None,
            get_request_start_time: None,
            get_request_complete_time: None,
            children_compete_time: None,
        }
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

unsafe impl Send for Link {}

unsafe impl Sync for Link {}

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
    pub parse_complete_time: DateTime<Utc>,
    pub links: Vec<Link>,
}
