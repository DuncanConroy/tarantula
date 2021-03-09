use hyper::{Uri, Response, Body, StatusCode};
use linkresult::ResponseTimings;

#[derive(Debug)]
pub struct Page {
    pub uri: Uri,
    response: Response<String>,
    pub response_timings: ResponseTimings,
    pub parent: Box<Option<Page>>,
    pub descendants: Vec<Page>,
    body: Option<String>,
}

impl Page {
    pub fn new(uri: Uri) -> Page {
        Page {
            uri,
            response: Default::default(),
            response_timings: ResponseTimings::new(),
            parent: Box::new(None),
            descendants: vec![],
            body: None,
        }
    }

    pub async fn set_response(&mut self, response: Response<Body>) {
        let transform = async |it| String::from_utf8_lossy(hyper::body::to_bytes(it).await.unwrap().as_ref()).to_string();
        self.response = response.map(transform);
    }

    pub fn get_response(&self) -> &Response<String> {
        &self.response
    }

    pub fn get_body(&self) -> &String {
        self.response.body()
    }

    pub fn get_content_length(&self) -> usize {
        self.response.headers().get("content-length").unwrap().to_str().unwrap().parse().unwrap()
    }

    pub fn get_content_type(&self) -> Option<&str> {
        if let Some(content_type) = self.response.headers().get("content-type") {
            if let Ok(str) = content_type.to_str() {
                Some(str)
            }
        }
        None
    }

    pub fn get_status_code(&self) -> StatusCode {
        self.response.status()
    }

    pub fn get_links(&self) -> Vec<Uri> {
        self.descendants.iter().map(|it| it.uri).collect()
    }
}