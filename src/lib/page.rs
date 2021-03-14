use hyper::{Body, HeaderMap, Response, StatusCode, Uri, Version};
use linkresult::{Link, ResponseTimings};

#[derive(Debug, Clone)]
pub struct PageResponse {
    pub status: StatusCode,
    pub version: Version,
    pub headers: HeaderMap,
}

#[derive(Debug, Clone)]
pub struct Page<'a> {
    pub link: Link,
    pub response_timings: ResponseTimings,
    pub descendants: Option<Vec<Page<'a>>>,
    pub parent: Option<&'a Page<'a>>,
    pub page_response: Option<PageResponse>,
    body: Option<String>,
}

impl<'a> Page<'a> {
    pub fn new(link: Link) -> Page<'a> {
        Page {
            link,
            page_response: None,
            response_timings: ResponseTimings::new(),
            parent: None,
            descendants: None,
            body: None,
        }
    }

    pub async fn set_response(&mut self, response: Response<Body>) {
        let (parts, body) = response.into_parts();
        self.page_response = Some(PageResponse {
            version: parts.version,
            status: parts.status,
            headers: parts.headers,
        });
        self.body = Some(
            String::from_utf8_lossy(hyper::body::to_bytes(body).await.unwrap().as_ref())
                .to_string(),
        );
    }

    pub fn get_body(&self) -> &Option<String> {
        &self.body
    }

    pub fn get_content_length(&self) -> usize {
        self.page_response
            .as_ref()
            .unwrap()
            .headers
            .get("content-length")
            .unwrap()
            .to_str()
            .unwrap()
            .parse()
            .unwrap()
    }

    pub fn get_content_type(&self) -> Option<&str> {
        println!("{:?}", self.page_response);
        if let Some(content_type) = self.page_response.as_ref()?.headers.get("content-type") {
            if let Ok(str) = content_type.to_str() {
                return Some(str);
            }
        }
        None
    }

    pub fn get_status_code(&self) -> Option<&StatusCode> {
        if let Some(response) = &self.page_response {
            return Some(&response.status);
        }
        None
    }

    pub fn get_links(&self) -> Vec<&Link> {
        match &self.descendants {
            Some(pages) => pages.iter().map(|it| &it.link).collect(),
            None => vec![],
        }
    }

    pub fn get_protocol(&self) -> String {
        let uri = self.get_uri();
        println!("get protocol: {}", uri);
        uri.scheme_str().unwrap().to_owned()
    }

    pub fn get_uri(&self) -> Uri {
        self.link.uri.parse::<hyper::Uri>().unwrap()
    }
}
