#[macro_use]
extern crate rocket;

use std::fs::File;
use std::io::Write;
use std::process;

use log::info;
use rocket::{Build, Rocket};

use page_loader::page_loader_service::PageLoaderService;
use server::http::crawl;

// A simple type alias so as to DRY.
pub type DynResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[rocket::main]
async fn main() -> DynResult<()> {
    let mut file = File::create("process.pid").unwrap();
    file.write_all(process::id().to_string().as_bytes()).unwrap();
    log4rs::init_file("config/log4rs.yaml", Default::default()).unwrap();
    info!("Starting tarantula");

    let page_loader_tx_channel = PageLoaderService::init();

    let _ = server::http::rocket(page_loader_tx_channel)
        .launch()
        .await;

    Ok(())
}