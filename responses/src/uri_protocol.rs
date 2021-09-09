use serde::Serialize;

#[derive(Debug, PartialEq, Eq, Clone, Serialize)]
pub enum UriProtocol {
    // http://example.com/bar
    HTTP,
    // https://example.com/bar
    HTTPS,
    // //example.com/bar
    IMPLICIT,
}