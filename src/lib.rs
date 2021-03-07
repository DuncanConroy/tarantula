use linkresult::{Link, ResponseTimings};
use hyper::{Response, Body, StatusCode, Uri};
use hyper::http::HeaderValue;

pub struct Page {
    pub uri: Uri,
    pub links: Vec<Link>,
    pub response: Response<Body>,
    pub response_timings: ResponseTimings,
    pub parent: Box<Option<Page>>,
}

impl Page {
    pub fn new(uri: Uri) -> Page {
        Page {
            uri,
            links: vec![],
            response: Default::default(),
            response_timings: ResponseTimings::new(),
            parent: Box::new(None),
        }
    }
    pub fn get_content_length(&self) -> usize {
        self.response.headers().get("content-length").unwrap().to_str().unwrap().parse().unwrap()
    }

    pub fn get_content_type(&self) -> &str {
        self.response.headers().get("content-type").unwrap().to_str().unwrap()
    }

    pub fn get_status_code(&self) -> StatusCode {
        self.response.status()
    }
}