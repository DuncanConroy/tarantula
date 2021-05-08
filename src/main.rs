use std::str::FromStr;
use std::sync::mpsc;

use clap::App;
use clap::load_yaml;
use log::{info, trace};
use log4rs;

use lib::*;

mod lib;

#[tokio::main]
async fn main() -> DynResult<()> {
    log4rs::init_file("config/log4rs.yaml", Default::default()).unwrap();
    // pretty_env_logger::init();
    info!("Starting tarantula");

    // todo: restructure memory layout to use a centralized list of strings/uris, e.g. like string table in AVM
    // todo: cleanup memory consumption
    // todo: stream results to WHERE? :D
    // TODO: multi-threaded
    process().await;
    Ok(())
}

fn parse_runconfig_from_args() -> Result<RunConfig, &'static str> {
    let yaml = load_yaml!("cli.yaml");
    let matches = App::from_yaml(yaml).get_matches();

    let url = matches.value_of("URL").unwrap();
    let mut run_config = RunConfig::new(url.to_string());

    run_config.ignore_redirects = matches.is_present("ignore_redirects");
    run_config.ignore_robots_txt = matches.is_present("ignore_robots_txt");

    if let Some(maximum_depth) = matches.value_of("maximum_depth") {
        run_config.maximum_depth = u8::from_str(&maximum_depth).unwrap()
    }
    if let Some(maximum_redirects) = matches.value_of("maximum_redirects") {
        run_config.maximum_redirects = u8::from_str(&maximum_redirects).unwrap()
    }
    if let Some(_) = matches.value_of("keep_html_in_memory") {
        run_config.keep_html_in_memory = true
    }

    info!("RunConfig: {:#?}", run_config);

    Ok(run_config)
}

async fn process() {
    let run_config = parse_runconfig_from_args().unwrap();
    let (tx, rx) = mpsc::channel();
    let handle = tokio::spawn(async move {
        let page_result = lib::init(run_config).await;
        tx.send(page_result);
    });

    let page = rx.recv().unwrap();

    info!("Finished.");
    trace!("Tarantula result:\n{:?}", page.unwrap())
}
