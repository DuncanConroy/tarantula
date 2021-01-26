use ego_tree::Tree;
use scraper::node::Element;
use scraper::{Html, Node};

use crate::linkresult::UriResult;

pub fn parse_body(body: &str) -> Html {
    let dom = Html::parse_document(body);
    // print(&dom.tree);

    let mut links = extract_links(&dom.tree);
    links.sort();
    // links.dedup();
    links.iter().for_each(|it| println!("{:#?}", it));
    // let results: UriResult = UriResult { links: links };
    // println!("uriResults: {:#?}", results);
    //TODO: do write util um verschiedene arten urls zu parsen - tdd
    dom
}

fn print(node: &Tree<Node>) {
    // let x = node;
    // println!("{}", x)
    let x = node.values().for_each(|it| {
        println!("{:#?}", it);
    });
}

fn extract_links(node: &Tree<Node>) -> Vec<&str> {
    let links: Vec<&str> = node
        .values()
        .filter_map(|it| it.as_element()?.attrs().next())
        .filter(|it| it.0 == "href")
        .map(|it| it.1)
        .collect();

    links
}

// use std::default::Default;
// use std::iter::repeat;
// use std::string::String;
//
// use html5ever::{parse_document, Parser};
// use html5ever::tendril::TendrilSink;
// use rcdom::{Handle, NodeData, RcDom};
//
// fn walk(indent: usize, handle: &Handle) {
//     let node = handle;
//     // FIXME: don't allocate
//     print!("{}", repeat(" ").take(indent).collect::<String>());
//     match node.data {
//         NodeData::Document => println!("#Document"),
//
//         NodeData::Doctype {
//             ref name,
//             ref public_id,
//             ref system_id,
//         } => println!("<!DOCTYPE {} \"{}\" \"{}\">", name, public_id, system_id),
//
//         NodeData::Text { ref contents } => {
//             println!("#text: {}", escape_default(&contents.borrow()))
//         }
//
//         NodeData::Comment { ref contents } => println!("<!-- {} -->", escape_default(contents)),
//
//         NodeData::Element {
//             ref name,
//             ref attrs,
//             ..
//         } => {
//             assert!(name.ns == ns!(html));
//             print!("<{}", name.local);
//             for attr in attrs.borrow().iter() {
//                 assert!(attr.name.ns == ns!());
//                 print!(" {}=\"{}\"", attr.name.local, attr.value);
//             }
//             println!(">");
//         }
//
//         NodeData::ProcessingInstruction { .. } => unreachable!(),
//     }
//
//     for child in node.children.borrow().iter() {
//         walk(indent + 4, child);
//     }
// }
//
// // FIXME: Copy of str::escape_default from std, which is currently unstable
// pub fn escape_default(s: &str) -> String {
//     s.chars().flat_map(|c| c.escape_default()).collect()
// }
//
// pub fn parse_body(input: &mut String) -> RcDom {
//     let foo = Parser::from_utf8(Parser::new).;
//     let dom = parse_document(RcDom::default(), Default::default())
//         .from_utf8()
//         .read_from(input)
//         .unwrap();
//     walk(0, &dom.document);
//
//     if !dom.errors.is_empty() {
//         println!("\nParse errors:");
//         for err in dom.errors.iter() {
//             println!("    {}", err);
//         }
//     }
//
//     dom
// }
