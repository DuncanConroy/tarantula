use async_recursion::async_recursion;
use chrono::Utc;
use hyper::{Body, Client, Request};
use hyper_tls::HttpsConnector;
use linkresult::{Link, UriResult};
use page::Page;

pub mod page;

// A simple type alias so as to DRY.
pub type DynResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

// pub async fn fetch_url(
//     url: &hyper::Uri,
//     &mut response_timings: ResponseTimings,
// ) -> DynResult<String> {
//     println!("URI: {}", url);
//
//     let https = HttpsConnector::new();
//     let client = Client::builder().build::<_, hyper::Body>(https);
//
//     let req = Request::builder()
//         .method("HEAD")
//         .uri(url)
//         .body(Body::from(""))
//         .expect("HEAD request builder");
//
//     let head = client.request(req).await?;
//     if !head.status().is_success() {
//         return Ok(String::from(""));
//         // todo: should be in metadata/response
//     }
//     let content_type = head.headers().get("content-type");
//     if content_type.is_none() {
//         return Err(format!("No content-type header found! {:?}", head).into());
//     }
//     if !content_type
//         .unwrap()
//         .to_str()
//         .unwrap()
//         .to_string()
//         .contains("text/html")
//     {
//         return Ok(String::from(""));
//     }
//
//     let response = client.get(url.clone()).await?;
//
//     // println!("Status: {}", response.status());
//     // println!("Headers: {:#?}\n", response.headers());
//
//     let body: String =
//         String::from_utf8_lossy(hyper::body::to_bytes(response.into_body()).await?.as_ref())
//             .to_string();
//     // println!("BODY: {}", body);
//
//     // println!("\nDone!");
//
//     Ok(body)
// }

pub async fn fetch_page(page: &mut Page<'_>) -> DynResult<()> {
    page.response_timings.overall_start_time = Utc::now();
    println!("URI: {}", page.link.uri);

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    let req = Request::builder()
        .method("HEAD")
        .uri(&page.get_uri())
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
    page.set_response(client.get(page.get_uri()).await?).await;
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

pub struct LoadPageArguments<'a, 'b> {
    pub page: &'a mut Page<'a>,
    pub host: &'b str,
    pub known_links: Vec<Link>,
    pub same_domain_only: bool,
}

unsafe impl<'a, 'b> Send for LoadPageArguments<'a, 'b> {}

#[async_recursion]
pub async fn recursive_load_page_and_get_links<'a>(
    load_page_arguments: &'a mut LoadPageArguments,
) -> DynResult<()> {
    let mut all_known_links: Vec<Link> = load_page_arguments.known_links.clone();

    let item_url_string = create_url_string(
        &load_page_arguments.page.get_protocol(),
        &load_page_arguments.host,
        &load_page_arguments.page.link.uri,
    );
    println!("item_url_string {}", item_url_string);
    let item_url = item_url_string.parse::<hyper::Uri>().unwrap();
    println!("trying {}", item_url);
    let mut links_to_visit: Vec<Link> = find_links_to_visit2(
        &load_page_arguments.page,
        all_known_links.clone(),
        &mut load_page_arguments.page.clone(),
        load_page_arguments.same_domain_only,
    )
    .await?;
    load_page_arguments.page.descendants = Some(
        links_to_visit
            .iter()
            .map(|it| Page::new(it.to_owned()))
            .collect(),
    );

    println!(
        "found {} links to visit: {:?}",
        links_to_visit.len(),
        links_to_visit
    );

    all_known_links.append(&mut links_to_visit);

    if let Some(descendants) = load_page_arguments.page.descendants.clone() {
        for element in descendants {
            let mut current_page: Page = element;
            current_page.parent = Some(&load_page_arguments.page);
            let item_url_string = create_url_string(
                &load_page_arguments.page.get_protocol(),
                &load_page_arguments.host,
                &current_page.link.uri,
            );
            println!("item_url_string {}", item_url_string);
            let item_url = item_url_string.parse::<hyper::Uri>().unwrap();
            println!("trying {}", item_url);
            let mut links_to_visit: Vec<Link> = find_links_to_visit2(
                &load_page_arguments.page,
                all_known_links.clone(),
                &mut current_page,
                load_page_arguments.same_domain_only,
            )
            .await?;
            current_page.descendants = Some(
                links_to_visit
                    .iter()
                    .map(|it| Page::new(it.to_owned()))
                    .collect(),
            );

            println!(
                "found {} links to visit: {:?}",
                links_to_visit.len(),
                links_to_visit
            );

            all_known_links.append(&mut links_to_visit);
            recursive_load_page_and_get_links(&mut LoadPageArguments {
                page: &mut current_page,
                host: load_page_arguments.host.clone(),
                known_links: all_known_links.clone(),
                same_domain_only: load_page_arguments.same_domain_only,
            })
            .await?;
        }
    }

    load_page_arguments
        .page
        .response_timings
        .children_compete_time = Some(Utc::now());
    // Ok(all_known_links)
    Ok(())
}

async fn find_links_to_visit2<'a>(
    parent_page: &Page<'a>,
    all_known_links: Vec<Link>,
    page_to_process: &mut Page<'a>,
    same_domain_only: bool,
) -> DynResult<Vec<Link>> {
    fetch_page(page_to_process).await?;
    let mut item_body = page_to_process.get_body().as_ref().unwrap().clone();
    page_to_process.response_timings.overall_complete_time = Some(Utc::now());
    if item_body.is_empty() {
        println!("No body found, no HTML to parse -> skipping");
        return Ok(Vec::<Link>::new());
    }

    let uri_result: UriResult = dom_parser::get_links(
        &parent_page.get_protocol(),
        &parent_page.get_uri().host().unwrap(),
        &mut item_body,
        same_domain_only,
    )
    .unwrap();
    page_to_process.response_timings.parse_complete_time = Some(uri_result.parse_complete_time);

    let links_to_visit: Vec<Link> = uri_result
        .links
        .iter()
        .filter(|it| !all_known_links.contains(&it))
        .map(|it| it)
        .cloned() // TOOD: check if can be removed
        .collect();
    Ok(links_to_visit)
}

// async fn find_links_to_visit(
//     parent_protocol: &str,
//     parent_uri: Option<Link>,
//     all_known_links: Vec<Link>,
//     item_url: Uri,
// ) -> DynResult<Vec<Link>> {
//     let request_start_time = Utc::now();
//     let mut item_body = fetch_url(&item_url).await?;
//     let response_timings = ResponseTimings {
//         overall_start_time: request_start_time,
//         parse_complete_time: None,
//         overall_complete_time: Some(Utc::now()),
//     };
//     if item_body.is_empty() {
//         println!("No body found, now HTML to parse -> skipping");
//         return Ok(Vec::<Link>::new());
//     }
//
//     let uri_result: UriResult = dom_parser::get_links(
//         &parent_protocol,
//         parent_uri,
//         &item_url.host().unwrap(),
//         &mut item_body,
//         true,
//         response_timings,
//     )
//     .unwrap();
//
//     let links_to_visit: Vec<Link> = uri_result
//         .links
//         .iter()
//         .filter(|it| !all_known_links.contains(&it))
//         .map(|it| it)
//         .cloned()
//         .collect();
//     Ok(links_to_visit)
// }

fn create_url_string(protocol: &str, host: &str, link: &String) -> String {
    println!("#-> {},{},{}", protocol, host, link);
    if link.starts_with("http") {
        link.to_owned()
    } else {
        format!("{}://{}{}", protocol, host, link)
    }
}
