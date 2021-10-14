use serde::Serialize;

#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
pub enum CrawlStatus {
    RestrictedByRobotsTxt,
    MaximumCrawlDepthReached,
}
