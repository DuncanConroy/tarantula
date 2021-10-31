use serde::Serialize;

#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
pub enum CrawlStatus {
    ConnectionError(String),
    RestrictedByRobotsTxt,
    MaximumCrawlDepthReached,
}
