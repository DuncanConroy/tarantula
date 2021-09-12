#[macro_use] extern crate rocket;

// use clap::App;
// use clap::load_yaml;
// use log::info;
// use tokio::sync::mpsc;

// use page_loader::page_loader_service::Command::CrawlDomainCommand;
// use page_loader::page_loader_service::PageLoaderService;
use server::http::crawl;

// A simple type alias so as to DRY.
pub type DynResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[launch]
pub async fn rocket() -> _ {
    rocket::build().mount("/", routes![crawl])
}

// #[tokio::main]
// // #[rocket::main]
// async fn main() -> DynResult<()> {
//     log4rs::init_file("config/log4rs.yaml", Default::default()).unwrap();
//     info!("Starting tarantula");
//     // rocket::build().mount("/", routes![crawl, crawl_get]).launch();
//
//     process().await;
//     Ok(())
// }

// fn parse_runconfig_from_args() -> Result<RunConfig, &'static str> {
//     let yaml = load_yaml!("cli.yaml");
//     let matches = App::from_yaml(yaml).get_matches();
//
//     let url = matches.value_of("URL").unwrap();
//     let mut run_config = RunConfig::new(url.to_string(), None);
//
//     run_config.ignore_redirects = matches.is_present("ignore_redirects");
//     run_config.ignore_robots_txt = matches.is_present("ignore_robots_txt");
//
//     if let Some(maximum_depth) = matches.value_of("maximum_depth") {
//         run_config.maximum_depth = u8::from_str(&maximum_depth).unwrap()
//     }
//     if let Some(maximum_redirects) = matches.value_of("maximum_redirects") {
//         run_config.maximum_redirects = u8::from_str(&maximum_redirects).unwrap()
//     }
//     if let Some(_) = matches.value_of("keep_html_in_memory") {
//         run_config.keep_html_in_memory = true
//     }
//
//     info!("RunConfig: {:#?}", run_config);
//
//     Ok(run_config)
// }
//
// async fn process() {
//     let num_cpus = num_cpus::get();
//     let tx = PageLoaderService::init();
//     let (resp_tx, mut resp_rx) = mpsc::channel(num_cpus * 2);
//
//     // only used for local command line - todo: refactor out of here
//     let run_config = parse_runconfig_from_args().unwrap();
//     let send_result = tx.send(CrawlDomainCommand { url: run_config.url, last_crawled_timestamp: 0, response_channel: resp_tx.clone() }).await;
//
//     let manager = tokio::spawn(async move {
//         let mut responses = 0;
//         while let Some(page_response) = resp_rx.recv().await {
//             info!("Received from threads: {:?}", page_response);
//             responses = responses + 1;
//             info!(". -> {}", responses);
//         }
//     });
//
//     manager.await.unwrap();
//
//     info!("Finished.");
// }
