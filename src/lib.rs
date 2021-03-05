use linkresult::{Link, ResponseTimings};
use hyper::{Response, Body, StatusCode, Uri};
use hyper::http::HeaderValue;

pub struct Page {
    pub uri: Uri,
    pub links: Vec<Link>,
    pub response: Response<Body>,
    pub response_timings: ResponseTimings,
    pub parent: Option<Page>,
}

impl Page {
    pub fn new(uri: Uri) -> Page{
        Page{
            uri,
            links: vec![],
            response: Default::default(),
            response_timings: ResponseTimings::new(),
            parent: None
        }
    }
    pub fn get_content_length(&self) -> usize {
        self.response.headers().get("content-length").unwrap() as usize
    }

    pub fn get_content_type(&self) -> String {
        self.response.headers().get("content-type").unwrap().to_string()
    }

    pub fn get_status_code(&self) -> StatusCode {
        self.response.status()
    }
}