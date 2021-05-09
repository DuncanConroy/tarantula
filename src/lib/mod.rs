use std::collections::HashSet;
use std::ops::Deref;
use std::process;
use std::sync::{Arc, Mutex};

use async_recursion::async_recursion;
use chrono::Utc;
use hyper::{Body, Client, header, Request, Response, Uri};
use hyper_tls::HttpsConnector;
use log::{debug, error, info, trace, warn};
use robotparser::RobotFileParser;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex as TokioMutex;
use tokio_stream::StreamExt;

use dom_parser::DomParser;
use linkresult::{Link, LinkTypeChecker, uri_result, uri_service, UriResult};
use linkresult::uri_service::UriService;
use page::Page;

pub mod page;

// A simple type alias so as to DRY.
pub type DynResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct RobotsSingleton<'a> {
    pub instance: Option<RobotFileParser<'a>>,
}

impl RobotsSingleton<'_> {
    fn can_fetch(&mut self, user_agent: &str, item_uri: &Uri) -> bool {
        if self.instance.is_none() {
            self.instance = Some(RobotFileParser::new(format!("{}://{}/robots.txt", item_uri.scheme_str().unwrap(), item_uri.host().unwrap())));
            self.instance.as_ref().unwrap().read();
        }

        self.instance.as_ref().unwrap().can_fetch(user_agent, &item_uri.to_string())
    }
}

pub(crate) static mut ROBOTS_TXT: RobotsSingleton = RobotsSingleton { instance: None };

#[derive(Clone, Debug)]
pub struct RunConfig {
    pub url: String,
    pub ignore_redirects: bool,
    pub maximum_redirects: u8,
    pub maximum_depth: u8,
    pub ignore_robots_txt: bool,
    pub keep_html_in_memory: bool,
    pub user_agent: String,
}

impl RunConfig {
    pub fn new(url: String) -> RunConfig {
        RunConfig {
            url,
            ignore_redirects: false,
            maximum_redirects: 10,
            maximum_depth: 16,
            ignore_robots_txt: false,
            keep_html_in_memory: false,
            user_agent: String::from("tarantula"),
        }
    }
}

struct AppContext {
    root_uri: String,
    link_type_checker: Arc<Mutex<LinkTypeChecker>>,
    dom_parser: Arc<Mutex<DomParser>>,
    uri_service: Arc<Mutex<UriService>>,
}

unsafe impl Send for AppContext {}

impl AppContext {
    pub fn new(uri: String) -> AppContext {
        let hyper_uri = uri.parse::<hyper::Uri>().unwrap();
        let host = hyper_uri.host().unwrap();
        let link_type_checker = Arc::new(Mutex::new(LinkTypeChecker::new(host)));
        let dom_parser = Arc::new(Mutex::new(DomParser::new(link_type_checker.clone())));
        let uri_service = Arc::new(Mutex::new(UriService::new(link_type_checker.clone())));
        AppContext {
            root_uri: uri,
            link_type_checker,
            dom_parser,
            uri_service,
        }
    }
}

pub async fn init(run_config: RunConfig, tx: Sender<Page>) -> Option<Page> {
    let uri = run_config.url.clone();
    let app_context = Arc::new(Mutex::new(AppContext::new(uri.clone())));
    let protocol = app_context.lock().unwrap().link_type_checker.lock().unwrap().get_uri_protocol("", &uri);
    if let None = protocol {
        error!("Invalid protocol {:?} in uri {}", protocol, uri);
        process::exit(1)
    }

    let protocol_unwrapped = protocol.clone().unwrap();
    let protocol_str = LinkTypeChecker::get_uri_protocol_as_str(&protocol_unwrapped);
    let page = Page::new_root(uri.clone(), protocol);

    let all_known_links = Arc::new(TokioMutex::new(HashSet::new()));
    let load_page_arguments = LoadPageArguments {
        host: page.get_uri().host().unwrap().into(),
        protocol: protocol_str.into(),
        page,
        same_domain_only: true,
        depth: 1,
    };

    let result = recursive_load_page_and_get_links(run_config, load_page_arguments, all_known_links.clone(), app_context.clone(), tx.clone()).await;
    Some(result.unwrap())
}

#[async_recursion]
async fn fetch_head(uri: Uri, ignore_redirects: bool, current_redirect: u8, maximum_redirects: u8, parent_uri: &Option<String>, uri_service: Arc<Mutex<UriService>>) -> DynResult<(Uri, Response<Body>)> {
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    let req = Request::builder()
        .method("HEAD")
        .uri(uri.clone())
        .body(Body::from(""))
        .expect("HEAD request builder");

    if ignore_redirects {
        Ok((uri, client.request(req).await.unwrap()))
    } else {
        let response = client.request(req).await.unwrap();
        trace!("HEAD for {}: {:?}", uri, response.headers());
        if current_redirect < maximum_redirects && response.status().is_redirection() {
            if let Some(location_header) = response.headers().get("location") {
                let adjusted_uri = uri_service.lock().unwrap().form_full_url(uri.scheme_str().unwrap(), location_header.to_str().unwrap(), uri.host().unwrap(), parent_uri);
                debug!("Following redirect {}", adjusted_uri);
                let response = fetch_head(adjusted_uri, ignore_redirects, current_redirect + 1, maximum_redirects, parent_uri, uri_service.clone()).await;
                return response;
            }
            info!("No valid location found in redirect header {:?}", response);
        }
        Ok((uri, response))
    }
}

async fn fetch_page(mut page: &mut Page, uri: Uri, run_config: &RunConfig, host: String, protocol: String, uri_service: Arc<Mutex<UriService>>) -> DynResult<()> {
    page.response_timings.overall_start_time = Utc::now();
    debug!("URI: {}", page.link.uri);
    let adjusted_uri = uri_service.lock().unwrap().form_full_url(&protocol, uri.path(), &host, &page.parent_uri);
    trace!("Adjusted URI: {}", adjusted_uri);

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    page.response_timings.head_request_start_time = Some(Utc::now());
    let (uri, head) = fetch_head(adjusted_uri, run_config.ignore_redirects, 1, run_config.maximum_redirects, &page.parent_uri, uri_service.clone()).await.unwrap();
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
        return Ok(());
    }
    Err(format!("No content-type header found! {:?}", head).into())
}

struct LoadPageArguments {
    page: Page,
    protocol: String,
    host: String,
    same_domain_only: bool,
    depth: u8,
}

unsafe impl Send for LoadPageArguments {}

#[async_recursion]
async fn recursive_load_page_and_get_links(
    run_config: RunConfig,
    mut load_page_arguments: LoadPageArguments,
    all_known_links: Arc<TokioMutex<HashSet<String>>>,
    app_context: Arc<Mutex<AppContext>>,
    tx: Sender<Page>,
) -> DynResult<Page> {
    if load_page_arguments.depth > run_config.maximum_depth {
        info!("Maximum depth exceeded ({} > {})!", load_page_arguments.depth, run_config.maximum_depth);
        return Ok(load_page_arguments.page);
    }

    trace!("all_known_links: {:#?}", all_known_links.lock().await);

    let uri_service = app_context.lock().unwrap().uri_service.clone();
    tokio::spawn(async move {
        all_known_links.lock().await.insert(load_page_arguments.page.link.uri.clone());
        let item_uri = prepare_item_url(&load_page_arguments, uri_service);
        let mut links_to_visit: Vec<Link>;

        if !run_config.ignore_robots_txt && !can_crawl(&run_config.user_agent, &item_uri) {
            links_to_visit = vec![];
            info!("Skipping {} due to robots.txt.", item_uri.to_string());
        } else {
            info!("trying {}", item_uri);
            links_to_visit = find_links_to_visit(
                &load_page_arguments.host,
                &load_page_arguments.protocol,
                all_known_links.clone(),
                &mut load_page_arguments.page,
                item_uri,
                load_page_arguments.same_domain_only,
                &run_config,
                app_context.clone(),
            )
                .await
                .unwrap();

            debug!(">>found {} links to visit: {:?}", links_to_visit.len(), links_to_visit);
        }

        let links_to_visit_as_string = links_to_visit.iter_mut().map(|it| it.uri.clone()).collect();
        all_known_links.lock().await.extend::<Vec<String>>(links_to_visit_as_string);

        if links_to_visit.len() > 0 && load_page_arguments.page.descendants.is_none() {
            load_page_arguments.page.descendants = Some(vec![]);
        }
        if let Some(mut descendants) = load_page_arguments.page.descendants {
            let mut stream = tokio_stream::iter(links_to_visit);
            while let Some(element) = stream.next().await {
                let current_page = Page::new_with_parent(element, load_page_arguments.page.link.uri.clone());
                let recursion_result = recursive_load_page_and_get_links(
                    run_config.clone(),
                    LoadPageArguments {
                        page: current_page,
                        protocol: load_page_arguments.protocol.clone().into(),
                        host: load_page_arguments.host.clone(),
                        same_domain_only: load_page_arguments.same_domain_only,
                        depth: load_page_arguments.depth + 1,
                    },
                    all_known_links.clone(),
                    app_context.clone(),
                    tx.clone(),
                ).await;

                if let Ok(current_page) = recursion_result {
                    descendants.push(current_page);
                }
            }
            load_page_arguments.page.descendants = Some(descendants);
        }

        load_page_arguments
            .page
            .response_timings
            .children_compete_time = Some(Utc::now());
        tx.send(load_page_arguments.page.clone()).await?;
        Ok(load_page_arguments.page)
    }).await?
}

fn prepare_item_url(load_page_arguments: &LoadPageArguments, uri_service: Arc<Mutex<UriService>>) -> Uri {
    uri_service.lock().unwrap().form_full_url(
        &load_page_arguments.protocol,
        &load_page_arguments.page.link.uri,
        &load_page_arguments.host,
        &load_page_arguments.page.parent_uri,
    )
}

async fn find_links_to_visit(
    host: &str,
    protocol: &str,
    all_known_links: Arc<TokioMutex<HashSet<String>>>,
    mut page_to_process: &mut Page,
    uri: Uri,
    same_domain_only: bool,
    run_config: &RunConfig,
    app_context: Arc<Mutex<AppContext>>,
) -> DynResult<Vec<Link>> {
    let uri_service = app_context.lock().unwrap().uri_service.clone();
    //tokio::spawn(async move {
    if let Ok(()) = fetch_page(page_to_process, uri, run_config, String::from(host), String::from(protocol), uri_service).await {
        let item_body = page_to_process.get_body().as_ref().unwrap().clone();
        page_to_process.response_timings.overall_complete_time = Some(Utc::now());
        if item_body.is_empty() {
            warn!("No body found, no HTML to parse -> skipping");
            return Ok(Vec::<Link>::new());
        }

        let uri_result: UriResult =
            app_context.lock().unwrap().dom_parser.lock().unwrap().get_links(&protocol, host, item_body).unwrap();
        page_to_process.response_timings.parse_complete_time = Some(uri_result.parse_complete_time);

        let result: Vec<Link> = if same_domain_only {
            let links_this_domain = get_same_domain_links(host, &uri_result.links, app_context.lock().unwrap().link_type_checker.clone());
            info!("Links on this domain: {}", links_this_domain.len());
            links_this_domain
        } else {
            uri_result.links
        };
        let links_to_visit = filter_links_to_visit(result, all_known_links.clone()).await;
        if !run_config.keep_html_in_memory {
            page_to_process.reset_body();
        }
        return Ok(links_to_visit);
    };

    Ok(vec![])
    //}).await?
}

fn get_same_domain_links(source_domain: &str, links: &Vec<Link>, link_type_checker: Arc<Mutex<LinkTypeChecker>>) -> Vec<Link> {
    let mut cloned_links = links.clone();
    cloned_links.sort_by(|a, b| a.uri.cmp(&b.uri));
    cloned_links.dedup_by(|a, b| a.uri.eq(&b.uri));
    cloned_links
        .iter()
        .map(|it| (it, link_type_checker.lock().unwrap().get_uri_scope(source_domain, it.uri.as_str())))
        .filter_map(|it| match it.1 {
            Some(uri_result::UriScope::Root)
            | Some(uri_result::UriScope::SameDomain)
            | Some(uri_result::UriScope::DifferentSubDomain) => Some(it.0),
            _ => None,
        })
        .cloned()
        .collect()
}

fn can_crawl(user_agent: &str, item_uri: &Uri) -> bool {
    let mut can_crawl: bool = false;
    unsafe {
        can_crawl = ROBOTS_TXT.can_fetch(&user_agent, &item_uri);
        debug!("Can crawl {}: {}", item_uri.to_string(), can_crawl);
    }

    can_crawl
}

async fn filter_links_to_visit(input: Vec<Link>, known_links: Arc<TokioMutex<HashSet<String>>>) -> Vec<Link> {
    let known_links_unlocked = known_links.lock().await;
    input.iter()
        .filter(|it| !known_links_unlocked.contains(&it.uri))
        .map(|it| it)
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

        let link_type_checker = Arc::new(Mutex::new(LinkTypeChecker::new("example.com")));
        let result = get_same_domain_links("example.com", &str_to_links(all_links()), link_type_checker);

        assert_eq!(result.len(), 8, "{:?}\n{:?}", result, sorted_expected);
        let result_strings: Vec<&String> = result.iter().map(|it| &it.uri).collect();
        assert_eq!(result_strings, sorted_expected);
    }

    #[tokio::test]
    async fn filter_links_to_visit_filters_correctly() {
        let mut links = HashSet::new();
        links.insert(String::from("https://example.com/foo"));
        links.insert(String::from("https://example.com/bar"));
        let known_links = Arc::new(TokioMutex::new(links));

        let result = filter_links_to_visit(vec![
            Link::from_str("https://example.com/foo"),
            Link::from_str("https://example.com/bar"),
            Link::from_str("https://example.com/foobar")],
                                           known_links.clone()).await;
        assert_eq!(result, vec![Link::from_str("https://example.com/foobar")]);
    }
}
