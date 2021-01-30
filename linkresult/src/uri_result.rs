use scraper::Node;
use chrono::{DateTime,Utc};

#[derive(Debug)]
pub struct ResponseTimings {
    request_start_time: DateTime<Utc>,
    request_complete_time: DateTime<Utc>,
    request_connection_confirmed_time: DateTime<Utc>,
}

#[derive(Debug)]
pub struct Link {
    uri: String,
    belonging: UriDestination,
    protocol: UriProtocol,
    source_tag: Node,
}

#[derive(Debug, PartialEq, Eq)]
pub enum UriProtocol {
    // http://example.com/bar
    HTTP,
    // https://example.com/bar
    HTTPS,
    // //example.com/bar
    IMPLICIT,
}

#[derive(Debug, PartialEq, Eq)]
pub enum UriDestination {
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
}

#[derive(Debug)]
pub struct UriResult<Link> {
    pub root: Link,
    pub response_timings: ResponseTimings,
    pub links: Vec<Link>,
}
