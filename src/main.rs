extern crate rocket;

use std::fs::File;
use std::io::Write;
use std::process;

use tracing::info;
use tracing::level_filters::LevelFilter;

use page_loader::page_loader_service::PageLoaderService;

// A simple type alias so as to DRY.
pub type DynResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[rocket::main]
async fn main() -> DynResult<()> {
    let mut file = File::create("process.pid").unwrap();
    file.write_all(process::id().to_string().as_bytes()).unwrap();

    init_log();

    info!("Starting tarantula");

    let page_loader_tx_channel = PageLoaderService::init();

    let _ = server::http::rocket(page_loader_tx_channel)
        .launch()
        .await;

    Ok(())
}

fn init_log() {
    use tracing_subscriber::prelude::*;

    let console_layer = console_subscriber::spawn();
    let fmt_layer = tracing_subscriber::fmt::layer();

    tracing_subscriber::registry()
        .with(console_layer)
        .with(fmt_layer.with_filter(LevelFilter::INFO))
        //  .with(..potential additional layer..)
        .init();

    // let log_init = log4rs::config::load_config_file("config/log4rs.yaml", Default::default()).unwrap();
    // log4rs::init_config(log_init);

    // let log_init = log4rs::init_file("config/log4rs.yaml", Default::default());
    // if log_init.is_ok() {
    //     log_init.unwrap();
    // }
}