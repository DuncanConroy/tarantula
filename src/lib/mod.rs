use async_recursion::async_recursion;
use chrono::Utc;
use hyper::{Body, Client, Request, Uri};
use hyper_tls::HttpsConnector;

use linkresult::{get_uri_scope, Link, uri_result, UriResult, UriScope};
use page::Page;
use std::fmt::Error;

pub mod page;

// A simple type alias so as to DRY.
pub type DynResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone)]
pub struct RunConfig {
    pub url: String,
    pub follow_redirects: bool,
    pub maximum_depth: u8,
}

impl RunConfig {
    pub fn new(url: String) -> RunConfig {
        RunConfig {
            url,
            follow_redirects: false,
            maximum_depth: 16,
        }
    }
}

pub async fn fetch_page(mut page: &mut Page, uri: Uri) -> DynResult<()> {
    page.response_timings.overall_start_time = Utc::now();
    println!("URI: {}", page.link.uri);

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    let req = Request::builder()
        .method("HEAD")
        .uri(&uri)
        .body(Body::from(""))
        .expect("HEAD request builder");

    page.response_timings.head_request_start_time = Some(Utc::now());
    let head = client.request(req).await?;
    page.response_timings.head_request_complete_time = Some(Utc::now());
    if !head.status().is_success() {
        page.set_response(head).await;
        return Err(format!(
            "HTTP Status: {} :: {:#?}",
            page.get_status_code().unwrap(),
            page.page_response.as_ref().unwrap().headers
        )
            .into());
    }

    page.response_timings.get_request_start_time = Some(Utc::now());
    page.set_response(client.get(uri).await?).await;
    page.response_timings.get_request_complete_time = Some(Utc::now());

    if let Some(content_type) = page.get_content_type() {
        if !content_type.contains("text/html") {
            return Err(format!("Content-Type: {}", content_type).into());
        }

        // println!("Status: {}", response.status());
        // println!("Headers: {:#?}\n", response.headers());

        // println!("BODY: {}", body);

        // println!("\nDone!");
        return Ok(());
    }
    Err(format!("No content-type header found! {:?}", head).into())
}

pub struct LoadPageArguments {
    pub page: Page,
    pub protocol: String,
    pub host: String,
    pub known_links: Vec<String>,
    pub same_domain_only: bool,
    pub depth: u8,
}

unsafe impl<'a, 'b> Send for LoadPageArguments {}

#[async_recursion]
pub async fn recursive_load_page_and_get_links(
    run_config: RunConfig,
    mut load_page_arguments: LoadPageArguments,
) -> DynResult<(Page, Vec<String>)> {
    if load_page_arguments.depth > run_config.maximum_depth.clone() {
        println!("Maximum depth exceeded!");
        return Ok((load_page_arguments.page, load_page_arguments.known_links));
    }

    let mut all_known_links = load_page_arguments.known_links;
    println!("all_known_links: {:#?}", all_known_links);

    // if all_known_links.contains(&load_page_arguments.page.link.uri) {
    //     println!(
    //         "Skipping already known {:?}",
    //         &load_page_arguments.page.link
    //     );
    //     return Ok((load_page_arguments.page, all_known_links));
    // }
    all_known_links.push(load_page_arguments.page.link.uri.clone());
    let item_uri = create_uri(
        &load_page_arguments.protocol,
        &load_page_arguments.host,
        &load_page_arguments.page.link.uri,
    );
    println!("trying {}", item_uri);
    let mut links_to_visit: Vec<Link> = find_links_to_visit(
        &load_page_arguments.host,
        &load_page_arguments.protocol,
        &all_known_links,
        &mut load_page_arguments.page,
        item_uri,
        load_page_arguments.same_domain_only,
    )
        .await
        .unwrap();

    println!(
        ">>found {} links to visit: {:?}",
        links_to_visit.len(),
        links_to_visit
    );

    all_known_links.append(&mut links_to_visit.iter_mut().map(|it| it.uri.clone()).collect());

    if links_to_visit.len() > 0 && load_page_arguments.page.descendants.is_none() {
        load_page_arguments.page.descendants = Some(vec![]);
    }
    if let Some(mut descendants) = load_page_arguments.page.descendants {
        for element in links_to_visit {
            let recursion_result = recursive_load_page_and_get_links(run_config.clone(), LoadPageArguments {
                page: Page::new(element),
                protocol: load_page_arguments.protocol.clone().into(),
                host: load_page_arguments.host.clone(),
                known_links: all_known_links.clone(),
                same_domain_only: load_page_arguments.same_domain_only,
                depth: load_page_arguments.depth + 1,
            }).await;

            if let Ok((current_page, additional_known_links)) = recursion_result {
                descendants.push(current_page);
                all_known_links = additional_known_links;
            }
        }
        load_page_arguments.page.descendants = Some(descendants);
    }

    load_page_arguments
        .page
        .response_timings
        .children_compete_time = Some(Utc::now());
    Ok((load_page_arguments.page, all_known_links))
}

async fn find_links_to_visit(
    source_domain: &str,
    protocol: &str,
    all_known_links: &Vec<String>,
    mut page_to_process: &mut Page,
    uri: Uri,
    same_domain_only: bool,
) -> DynResult<Vec<Link>> {
    if let Ok(fetch) = fetch_page(page_to_process, uri).await {
        let mut item_body = page_to_process.get_body().as_ref().unwrap().clone();
        page_to_process.response_timings.overall_complete_time = Some(Utc::now());
        if item_body.is_empty() {
            println!("No body found, no HTML to parse -> skipping");
            return Ok(Vec::<Link>::new());
        }

        let uri_result: UriResult =
            dom_parser::get_links(&protocol, source_domain, &mut item_body).unwrap();
        page_to_process.response_timings.parse_complete_time = Some(uri_result.parse_complete_time);

        let result: Vec<Link> = if same_domain_only {
            let links_this_domain = get_same_domain_links(source_domain, &uri_result.links);
            println!("Links on this domain: {}", links_this_domain.len());
            links_this_domain
        } else {
            uri_result.links
        };
        let links_to_visit: Vec<Link> = result
            .iter()
            .filter(|it| !all_known_links.contains(&it.uri))
            .map(|it| it)
            .cloned()
            .collect();
        return Ok(links_to_visit);
    };

    Ok(vec![])
}

fn get_same_domain_links(source_domain: &str, links: &Vec<Link>) -> Vec<Link> {
    let mut cloned_links = links.clone();
    cloned_links.sort_by(|a, b| a.uri.cmp(&b.uri));
    cloned_links.dedup_by(|a, b| a.uri.eq(&b.uri));
    cloned_links
        .iter()
        .map(|it| (it, get_uri_scope(source_domain, it.uri.as_str())))
        .filter_map(|it| match it.1 {
            Some(uri_result::UriScope::Root)
            | Some(uri_result::UriScope::SameDomain)
            | Some(uri_result::UriScope::DifferentSubDomain) => Some(it.0),
            _ => None,
        })
        .cloned()
        .collect()
}

fn create_uri(protocol: &str, host: &str, link: &String) -> Uri {
    let url_string = if link.starts_with("http") {
        link.to_owned()
    } else {
        format!("{}://{}{}", protocol, host, link)
    };

    url_string.parse::<hyper::Uri>().unwrap()
}

#[cfg(test)]
mod tests {
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
    fn get_domain_links_returns_correct_links() {
        let sorted_expected = vec![
            "/",
            "/account/login?redirect=https://example.com/",
            "/agb/",
            "/ausgabe/example-com-62-mindful-leadership/",
            "https://example.com/",
            "https://example.com/ausgabe/example-com-59-straight-outta-office/",
            "https://example.com/events/",
            "https://faq.example.com/",
        ];

        let result = get_same_domain_links("example.com", &str_to_links(all_links()));

        assert_eq!(result.len(), 8, "{:?}\n{:?}", result, sorted_expected);
        let result_strings: Vec<&String> = result.iter().map(|it| &it.uri).collect();
        assert_eq!(result_strings, sorted_expected);
    }
}
