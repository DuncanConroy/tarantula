use async_recursion::async_recursion;
use chrono::Utc;
use hyper::{Body, Client, header, Request, Response, Uri};
use hyper_tls::HttpsConnector;

use linkresult::{get_uri_scope, Link, uri_result, uri_service, UriResult};
use page::Page;

pub mod page;

// A simple type alias so as to DRY.
pub type DynResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Debug)]
pub struct RunConfig {
    pub url: String,
    pub follow_redirects: bool,
    pub maximum_redirects: u8,
    pub maximum_depth: u8,
    pub ignore_robots_txt: bool,
    pub keep_html_in_memory: bool,
}

impl RunConfig {
    pub fn new(url: String) -> RunConfig {
        RunConfig {
            url,
            follow_redirects: true,
            maximum_redirects: 10,
            maximum_depth: 16,
            ignore_robots_txt: false,
            keep_html_in_memory: false,
        }
    }
}

#[async_recursion]
async fn fetch_head(uri: Uri, follow_redirects: bool, current_redirect: u8, maximum_redirects: u8, parent_uri: &Option<String>) -> DynResult<(Uri, Response<Body>)> {
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    let req = Request::builder()
        .method("HEAD")
        .uri(uri.clone())
        .body(Body::from(""))
        .expect("HEAD request builder");

    match follow_redirects {
        true => {
            let response = client.request(req).await.unwrap();
            println!("HEAD for {}: {:?}", uri.clone(), response.headers().clone());
            if current_redirect < maximum_redirects && response.status().is_redirection() {
                if let Some(location_header) = response.headers().get("location") {
                    // let uri = Uri::from_str(location_header.to_str().unwrap()).unwrap();
                    let adjusted_uri_str = uri_service::form_full_url(uri.scheme_str().unwrap(), location_header.to_str().unwrap(), uri.host().unwrap(), parent_uri);
                    let adjusted_uri = adjusted_uri_str.parse::<hyper::Uri>().unwrap();
                    println!("Following redirect {}", adjusted_uri);
                    let response = fetch_head(adjusted_uri, follow_redirects, current_redirect + 1, maximum_redirects, parent_uri).await;
                    return response;
                }
                println!("No valid location found in redirect header {:?}", response);
            }
            Ok((uri, response))
        }
        false => Ok((uri, client.request(req).await.unwrap()))
    }
}

pub async fn fetch_page(mut page: &mut Page, uri: Uri, follow_redirects: bool, maximum_redirects: u8, host: String, protocol: String) -> DynResult<()> {
    page.response_timings.overall_start_time = Utc::now();
    println!("URI: {}", page.link.uri);
    let adjusted_uri = uri_service::form_full_url(&protocol, uri.path(), &host, &page.parent_uri).parse::<hyper::Uri>().unwrap();
    println!("Adjusted URI: {}", adjusted_uri);

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    page.response_timings.head_request_start_time = Some(Utc::now());
    let (uri, head) = fetch_head(adjusted_uri, follow_redirects, 1, maximum_redirects, &page.parent_uri).await.unwrap();
    page.response_timings.head_request_complete_time = Some(Utc::now());
    if !head.status().is_success() {
        page.set_response(head).await;
        return Err(format!(
            "HTTP Status: {} :: {:#?}",
            page.get_status_code().unwrap(),
            page.page_response.as_ref().unwrap().headers
        ).into());
    }
    if let Some(content_type) = head.headers().get(header::CONTENT_TYPE) {
        let content_type_string = String::from(content_type.to_str().unwrap());
        if !content_type_string.starts_with("text/html") {
            return Err(format!(
                "Skipping URL {}, as content-type is {:?}",
                uri,
                content_type
            ).into());
        }
    }

    page.response_timings.get_request_start_time = Some(Utc::now());
    let response = client.get(uri.to_owned()).await?;
    page.set_response(response).await;
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
        println!("Maximum depth exceeded ({} > {})!", load_page_arguments.depth, run_config.maximum_depth);
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
    let item_uri = uri_service::create_uri(
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
        run_config.follow_redirects,
        run_config.maximum_redirects,
        &load_page_arguments.host,
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
            let current_page = Page::new_with_parent(element, load_page_arguments.page.link.uri.clone());
            let recursion_result = recursive_load_page_and_get_links(run_config.clone(), LoadPageArguments {
                page: current_page,
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
    follow_redirects: bool,
    maximum_redirects: u8,
    host: &str,
) -> DynResult<Vec<Link>> {
    if let Ok(()) = fetch_page(page_to_process, uri, follow_redirects, maximum_redirects, String::from(host), String::from(protocol)).await {
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

#[cfg(test)]
mod tests {
    use testutils::*;

    use super::*;

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
