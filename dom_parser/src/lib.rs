use ego_tree::Tree;
use scraper::{Html, Node};

use linkresult::{UriResult, get_uri_destination, uri_result};

pub fn get_links(source_domain: &str, body: &str) -> Vec<String> {
    let dom = Html::parse_document(body);
    // print(&dom.tree);

    let mut links = extract_links(&dom.tree);
    links.sort();
    // links.dedup();
    println!("Links total: {}", links.len());
    links.iter().for_each(|it| println!("{:#?}", it));
    // let results: UriResult = UriResult { links: links };
    // println!("uriResults: {:#?}", results);
    let links_this_domain: Vec<&str> = get_same_domain_links(&source_domain, &links);
    println!("Links on this domain: {}", links_this_domain.len());
    links_this_domain.iter().map(|it| it.to_string()).collect()
}


pub fn get_same_domain_links<'a>(source_domain: &str, links: &Vec<&'a str>) -> Vec<&'a str> {
    let mut cloned_links = links.clone();
    cloned_links.sort();
    cloned_links.dedup();
    cloned_links
        .iter()
        .map(|it| (it, get_uri_destination(source_domain, it)))
        .filter(|it| it.1.is_some())
        .filter(|it| match it.1.as_ref().unwrap() {
            uri_result::UriDestination::Root |
            uri_result::UriDestination::SameDomain |
            uri_result::UriDestination::DifferentSubDomain => true,
            _ => false
        })
        .map(|it| it.0)
        .cloned()
        .collect()
}

fn print(node: &Tree<Node>) {
    // let x = node;
    // println!("{}", x)
    let x = node.values().for_each(|it| {
        println!("{:#?}", it);
    });
}

fn extract_links(node: &Tree<Node>) -> Vec<&str> {
    node
        .values()
        .filter_map(|it| it.as_element()?.attrs().next())
        .filter(|it| it.0 == "href")
        .map(|it| it.1)
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn get_domain_links_returns_correct_links() {
        let all_links = vec![
            // valid, same domain: 8 elements, unsorted
            "https://t3n.de/",
            "https://t3n.de/ausgabe/t3n-59-straight-outta-office/",
            "/account/login?redirect=https://t3n.de/",
            "/",
            "/",
            "/agb/",
            "/agb/",
            "/ausgabe/t3n-62-mindful-leadership/",
            "/ausgabe/t3n-62-mindful-leadership/",
            "https://t3n.de/events/",
            "https://faq.t3n.de/",
            "https://t3n.de/events/",

            // invalid &| extern
            "#",
            "#s-angle-down",
            "#s-angle-down",
            "#s-angle-down",
            "#s-brief",
            "#s-business-development",
            "#s-content-redaktion",
            "#s-design-ux",
            "#s-facebook",
            "#s-flipboard",
            "#s-instagram",
            "#s-itunes",
            "#s-pocket",
            "#s-produktmanagement-projektmanagement",
            "#s-rss",
            "#s-soundcloud",
            "http://www.agof.de/",
            "http://feeds2.feedburner.com/t3n-magazin/",
            "https://d1quwwdmdfumn6.cloudfront.net/t3n/2018/styles/main-1610630962.css",
            "https://getpocket.com/edit.php?url=https%3A%2F%2Ft3n.de%2Fnews%2Fbiz-chef-bitcoin-system-1352881%2F%3Futm_source%3Dpocket%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons",
            "https://twitter.com/intent/tweet?text=BIZ-Chef%3A%20Das%20Bitcoin-System%20kann%20zusammenbrechen&url=https%3A%2F%2Ft3n.de%2Fnews%2Fbiz-chef-bitcoin-system-1352881%2F%3Futm_source%3Dtwitter.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons&via=t3n&lang=de",
            "https://twitter.com/intent/tweet?text=Clubnotes.io%20%E2%80%93%20so%20machst%20du%20Notizen%20in%20deinem%20Clubhouse-Talk&url=https%3A%2F%2Ft3n.de%2Fnews%2Fclubnotesio-machst-notizen-1352852%2F%3Futm_source%3Dtwitter.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons&via=t3n&lang=de",
            "https://twitter.com/t3n",
            "https://www.facebook.com/sharer.php?u=https%3A%2F%2Ft3n.de%2Fnews%2Fbusiness-trends-gaming-zukunft-1350706%2F%3Futm_source%3Dfacebook.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons",
            "https://www.facebook.com/sharer.php?u=https%3A%2F%2Ft3n.de%2Fnews%2Fclubnotesio-machst-notizen-1352852%2F%3Futm_source%3Dfacebook.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons",
            "https://www.facebook.com/t3nMagazin",
            "https://www.kununu.com/de/t3n/",
            "https://www.linkedin.com/shareArticle?mini=true&url=https%3A%2F%2Ft3n.de%2Fnews%2Fcoinbase-kryptomarktplatz-direktplatzierung-boersenstart-1352871%2F%3Futm_source%3Dlinkedin.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons",
            "https://www.linkedin.com/shareArticle?mini=true&url=https%3A%2F%2Ft3n.de%2Fnews%2Ftwitter-plant-facebook-1352857%2F%3Futm_source%3Dlinkedin.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons",
            "mailto:support@t3n.de",
        ];

        let result = get_same_domain_links("t3n.de", &all_links);
        assert_eq!(result.len(), 8);
        let sorted_expected = vec![
            "/",
            "/account/login?redirect=https://t3n.de/",
            "/agb/",
            "/ausgabe/t3n-62-mindful-leadership/",
            "https://faq.t3n.de/",
            "https://t3n.de/",
            "https://t3n.de/ausgabe/t3n-59-straight-outta-office/",
            "https://t3n.de/events/",
        ];
        assert_eq!(result, sorted_expected);
    }
}