extern crate rocket;

use std::fs::File;
use std::io::Write;
use std::process;

use log::info;

use page_loader::page_loader_service::PageLoaderService;

// A simple type alias so as to DRY.
pub type DynResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[rocket::main]
async fn main() -> DynResult<()> {
    // console_subscriber::init();

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
    let log_init = log4rs::init_file("config/log4rs.yaml", Default::default());
    if log_init.is_ok() {
        log_init.unwrap();
    }
}