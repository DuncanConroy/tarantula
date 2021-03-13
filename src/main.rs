use std::env;

use lib::*;
use linkresult::{get_uri_protocol, get_uri_scope, Link};
use page::*;

mod lib;

async fn main2() {
    let uri = get_url_from_args().unwrap();
    let mut page = Page::new(Link {
        scope: get_uri_scope(&uri, &uri),
        protocol: get_uri_protocol("", &uri),
        uri,
        source_tag: None,
    });
    lib::recursive_load_page_and_get_links(&mut lib::LoadPageArguments {
        host: page.get_uri().host().unwrap(),
        known_links: vec![],
        page: &mut page,
        same_domain_only: true,
    })
    .await;
    // println!("{:?}", page)
}

#[tokio::main]
async fn main() -> DynResult<()> {
    pretty_env_logger::init();
    //todo: implement redirect following
    main2().await;
    return Ok(());
    ////
    // let url = parse_url_from_args()?;
    //
    // // todo: implement a page object in order to save all relevant information
    // let start_time = Utc::now();
    // let mut body = fetch_url(&url).await?;
    // let response_timings = ResponseTimings {
    //     overall_start_time: start_time,
    //     parse_complete_time: None,
    //     overall_complete_time: Some(Utc::now()),
    // };
    // println!("HOST:{}", &url.host().unwrap());
    // let protocol = format!("{}://", &url.scheme().unwrap());
    // let uri_result: UriResult = dom_parser::get_links(
    //     protocol.as_str(),
    //     None,
    //     &url.host().unwrap(),
    //     &mut body,
    //     true,
    //     response_timings,
    // )
    // .unwrap();
    // println!("links: {:?}", uri_result);
    //
    // //TODO: multi-threaded
    //
    // let known_links = vec![Link::from_str("/")];
    // let parent_uri = Some(Link::from_str(url.host().unwrap()));
    // let total_links = recursive_load_page_and_get_links(LoadPageArguments {
    //     parent_protocol: protocol,
    //     parent_uri,
    //     host: url.host().unwrap().to_string(),
    //     links: uri_result.links,
    //     known_links,
    // })
    // .await?;
    //
    // println!("total_links: {:?}", total_links);
    //
    // Ok(())
}

// fn parse_url_from_args() -> Result<Uri, &'static str> {
//     // Some simple CLI args requirements...
//     match env::args().nth(1) {
//         None => Err("Usage: client <url>"),
//         Some(url) => Ok(url.parse::<hyper::Uri>().unwrap()),
//     }
// }

fn get_url_from_args() -> Result<String, &'static str> {
    // Some simple CLI args requirements...
    match env::args().nth(1) {
        None => Err("Usage: client <url>"),
        Some(url) => Ok(url),
    }
}

// struct LoadPageArguments {
//     parent_protocol: String,
//     parent_uri: Option<Link>,
//     host: String,
//     links: Vec<Link>,
//     known_links: Vec<Link>,
// }
//
// unsafe impl Send for LoadPageArguments {}

// #[async_recursion]
// async fn recursive_load_page_and_get_links(
//     load_page_arguments: LoadPageArguments,
// ) -> DynResult<Vec<Link>> {
//     let mut all_known_links: Vec<Link> = vec![];
//     all_known_links.append(&mut load_page_arguments.known_links.clone());
//
//     for link in load_page_arguments.links {
//         let item_url_string = create_url_string(
//             &load_page_arguments.parent_protocol,
//             &load_page_arguments.host,
//             &link.uri,
//         );
//         println!("item_url_string {}", item_url_string);
//         let item_url = item_url_string.parse::<hyper::Uri>().unwrap();
//         println!("trying {}", item_url);
//         let mut links_to_visit = find_links_to_visit(
//             &load_page_arguments.parent_protocol,
//             load_page_arguments.parent_uri.clone(),
//             all_known_links.clone(),
//             item_url,
//         )
//         .await?;
//
//         println!(
//             "found {} links to visit: {:?}",
//             links_to_visit.len(),
//             links_to_visit
//         );
//
//         all_known_links.append(&mut links_to_visit);
//         recursive_load_page_and_get_links(LoadPageArguments {
//             parent_protocol: load_page_arguments.parent_protocol.clone(),
//             parent_uri: load_page_arguments.parent_uri.clone(),
//             host: load_page_arguments.host.clone(),
//             links: links_to_visit.clone(),
//             known_links: all_known_links.clone(),
//         })
//         .await?;
//     }
//
//     Ok(all_known_links)
// }
//
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
//
// fn create_url_string(protocol: &str, host: &str, link: &String) -> String {
//     println!("#-> {},{},{}", host, protocol, link);
//     if link.starts_with("http") {
//         link.to_owned()
//     } else {
//         format!("{}{}{}", protocol, host, link)
//     }
// }
