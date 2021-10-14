use std::{fs::read_to_string, path::PathBuf};
use std::sync::Arc;

use dom_parser::{DomParser, DomParserService};
use linkresult::link_type_checker::LinkTypeChecker;

#[test]
fn extract_links_returns_correct_links_and_nodes() {
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.push("tests/resources/example.com.html");
    let html_file = read_to_string(&d).unwrap();

    let host = "www.example.com";
    let instance = DomParserService::new(Arc::new(LinkTypeChecker::new(host)));
    let result = instance.get_links("https", host, &html_file);
    assert_eq!(result.is_some(), true, "Should have a result");
    assert_eq!(result.unwrap().links.len(), 451 + 79, "Number of links should match"); // href: 451, (data-)?src: 79
}