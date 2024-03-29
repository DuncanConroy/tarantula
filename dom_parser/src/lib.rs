use std::sync::Arc;

use chrono::Utc;
use ego_tree::Tree;
use scraper::{Html, Node};

use linkresult::link_type_checker::LinkTypeChecker;
use linkresult::uri_result::UriResult;
use responses::link::Link;

pub trait DomParser: Sync + Send {
    fn get_links(&self, parent_protocol: &str, source_domain: &str, body: &String) -> Option<UriResult>;
}

pub struct DomParserService {
    link_type_checker: Arc<LinkTypeChecker>,
}

impl DomParser for DomParserService {
    fn get_links(&self, parent_protocol: &str, source_domain: &str, body: &String) -> Option<UriResult> {
        let dom = Html::parse_document(body);

        let mut links = self.extract_links(&parent_protocol, &source_domain, dom.tree);
        let parse_complete_time = Utc::now();
        links.sort_by(|a, b| a.uri.cmp(&b.uri));

        Some(UriResult {
            links,
            parse_complete_time,
        })
    }
}

impl DomParserService{
    pub fn new(link_type_checker: Arc<LinkTypeChecker>) -> DomParserService {
        DomParserService {
            link_type_checker,
        }
    }
    fn extract_links(
        &self,
        parent_protocol: &str,
        host: &str,
        node: Tree<Node>,
    ) -> Vec<Link> {
        let link_attribute_identifiers = vec!["href", "src", "data-src"];
        node.values()
            .filter_map(|current_node| {
                let (_, link) = current_node
                    .as_element()?
                    .attrs()
                    .find(|attribute| link_attribute_identifiers.contains(&attribute.0))?;
                Some(Link {
                    uri: link.trim().to_string(),
                    scope: self.link_type_checker.get_uri_scope(&host, &link),
                    protocol: self.link_type_checker.get_uri_protocol(&parent_protocol, &link),
                    source_tag: Some(format!("{:?}", current_node.as_element().unwrap())),
                })
            })
            .collect()
    }
}
