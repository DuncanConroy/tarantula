use std::sync::{Arc, Mutex};

use chrono::Utc;
use ego_tree::Tree;
use scraper::{Html, Node};

use linkresult::{Link, LinkTypeChecker, UriResult};

pub struct DomParser {
    link_type_checker: Arc<LinkTypeChecker>,
}

impl DomParser {
    pub fn new(link_type_checker: Arc<LinkTypeChecker>) -> DomParser {
        DomParser {
            link_type_checker,
        }
    }

    pub fn get_links(&self, parent_protocol: &str, source_domain: &str, body: String) -> Option<UriResult> {
        let dom = Html::parse_document(&body);

        let mut links = self.extract_links(&parent_protocol, &source_domain, dom.tree);
        let parse_complete_time = Utc::now();
        links.sort_by(|a, b| a.uri.cmp(&b.uri));

        Some(UriResult {
            links,
            parse_complete_time,
        })
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
                    source_tag: Some(current_node.clone()),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::read_to_string, path::PathBuf};

    use super::*;

    fn str_to_links(links: Vec<&str>) -> Vec<Link> {
        links.iter().map(|it| Link::from_str(it)).collect()
    }

    #[test]
    fn extract_links_returns_correct_links_and_nodes() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d = d.parent().unwrap().to_path_buf();
        d.push("resources/test/example.com.html");
        let html_file = read_to_string(&d).unwrap();

        let host = "www.example.com";
        let instance = DomParser::new(Arc::new(LinkTypeChecker::new(host)));
        let input = Html::parse_document(html_file.as_str());
        let result = instance.extract_links("https", host, input.tree);
        assert_eq!(result.len(), 451 + 79); // href: 451, (data-)?src: 79
    }
}
