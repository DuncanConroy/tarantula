use chrono::Utc;
use ego_tree::Tree;
use scraper::{Html, Node};

use linkresult::{get_uri_protocol, get_uri_scope, uri_result, Link, UriResult};

pub fn get_links(parent_protocol: &str, source_domain: &str, body: &str) -> Option<UriResult> {
    let dom = Html::parse_document(body);
    // println!("{:?}", dom);
    // print(&dom.tree);

    let mut links = extract_links(&parent_protocol, &source_domain, &dom.tree);
    let parse_complete_time = Utc::now();
    links.sort_by(|a, b| a.uri.cmp(&b.uri));
    // links.dedup();
    // println!("Links total: {}", links.len());
    // links.iter().for_each(|it| println!("{:#?}", it));
    // let results: UriResult = UriResult { links: links };
    // println!("uriResults: {:#?}", results);

    Some(UriResult {
        links,
        parse_complete_time,
    })
}

// fn print(node: &Tree<Node>) {
//     node.values().for_each(|it| {
//         println!("{:#?}", it);
//     });
// }

fn extract_links<'a>(
    parent_protocol: &str,
    source_domain: &str,
    node: &'a Tree<Node>,
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
                scope: get_uri_scope(&source_domain, &link),
                protocol: get_uri_protocol(&parent_protocol, &link),
                source_tag: Some(current_node.to_owned()),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{fs::read_to_string, path::PathBuf};

    use super::*;

    fn all_links<'a>() -> Vec<&'a str> {
        let links = vec![
            // valid, same domain: 8 elements, unsorted
            "https://example.com/",
            "https://example.com/ausgabe/example-com-59-straight-outta-office/",
            "/account/login?redirect=https://example.com/",
            "/",
            "/",
            "/agb/",
            "/agb/",
            "/ausgabe/example-com-62-mindful-leadership/",
            "/ausgabe/example-com-62-mindful-leadership/",
            "https://example.com/events/",
            "https://faq.example.com/",
            "https://example.com/events/",

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
            "http://feeds2.feedburner.com/example-com-magazin/",
            "https://example-com.cloudfront.net/example-com/styles/main-1234567890.css",
            "https://getpocket.com/edit.php?url=https%3A%2F%2Fexample.com%2Fnews%2Fbiz-chef-bitcoin-system-1352881%2F%3Futm_source%3Dpocket%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons",
            "https://twitter.com/intent/tweet?text=BIZ-Chef%3A%20Das%20Bitcoin-System%20kann%20zusammenbrechen&url=https%3A%2F%2Fexample.com%2Fnews%2Fbiz-chef-bitcoin-system-1352881%2F%3Futm_source%3Dtwitter.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons&via=example-com&lang=de",
            "https://twitter.com/intent/tweet?text=Clubnotes.io%20%E2%80%93%20so%20machst%20du%20Notizen%20in%20deinem%20Clubhouse-Talk&url=https%3A%2F%2Fexample.com%2Fnews%2Fclubnotesio-machst-notizen-1352852%2F%3Futm_source%3Dtwitter.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons&via=example-com&lang=de",
            "https://twitter.com/example-com",
            "https://www.facebook.com/sharer.php?u=https%3A%2F%2Fexample.com%2Fnews%2Fbusiness-trends-gaming-zukunft-1350706%2F%3Futm_source%3Dfacebook.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons",
            "https://www.facebook.com/sharer.php?u=https%3A%2F%2Fexample.com%2Fnews%2Fclubnotesio-machst-notizen-1352852%2F%3Futm_source%3Dfacebook.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons",
            "https://www.facebook.com/example-comMagazin",
            "https://www.kununu.com/de/example-com/",
            "https://www.linkedin.com/shareArticle?mini=true&url=https%3A%2F%2Fexample.com%2Fnews%2Fcoinbase-kryptomarktplatz-direktplatzierung-boersenstart-1352871%2F%3Futm_source%3Dlinkedin.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons",
            "https://www.linkedin.com/shareArticle?mini=true&url=https%3A%2F%2Fexample.com%2Fnews%2Ftwitter-plant-facebook-1352857%2F%3Futm_source%3Dlinkedin.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons",
            "mailto:support@example.com",
            "//storage.googleapis.com/example.com/assets/foo.png",
        ];

        links
    }

    fn str_to_links(links: Vec<&str>) -> Vec<Link> {
        links.iter().map(|it| Link::from_str(it)).collect()
    }

    #[test]
    fn extract_links_returns_correct_links_and_nodes() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d = d.parent().unwrap().to_path_buf();
        d.push("resources/test/example.com.html");
        let html_file = read_to_string(&d).unwrap();

        let input = Html::parse_document(html_file.as_str());
        let result = extract_links("https", "www.example.com", &input.tree);
        assert_eq!(result.len(), 451 + 79); // href: 451, (data-)?src: 79
    }
}
