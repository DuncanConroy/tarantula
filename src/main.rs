use std::env;
use std::process;

use lib::*;
use linkresult::{get_uri_protocol, get_uri_protocol_as_str, get_uri_scope, Link, UriScope};
use page::*;
use std::str::FromStr;

mod lib;

#[tokio::main]
async fn main() -> DynResult<()> {
    pretty_env_logger::init();
    // todo: clean up options to go before the url
    // todo: don't load non-html files, e.g. PNG
    // todo: see docs folder for refactoring
    // todo: respect robots.txt file
    // todo: add option to keep or dispose of html source
    // TODO: multi-threaded
    process().await;
    Ok(())
}

fn parse_runconfig_from_args() -> Result<RunConfig, &'static str> {
    let url = match env::args().nth(1) {
        Some(url) => Ok(url),
        _ => Err("Usage: tarantula <url> [<follow_redirects (true|false, default=false)>] [<maximum_depth (default=16)>]"),
    };
    let mut run_config = RunConfig::new(url.unwrap());
    if let Some(follow_redirects) = env::args().nth(2) {
        run_config.follow_redirects = follow_redirects.to_lowercase().eq("true")
    }
    if let Some(maximum_depth) = env::args().nth(3) {
        run_config.maximum_depth = u8::from_str(&maximum_depth).unwrap()
    }
    Ok(run_config)
}

async fn process() {
    let run_config = parse_runconfig_from_args().unwrap();
    let uri = run_config.url.clone();
    let protocol = get_uri_protocol("", &uri);
    if let None = protocol {
        eprintln!("Invalid protocol {:?} in uri {}", protocol, uri);
        process::exit(1)
    }

    let protocol_unwrapped = protocol.clone().unwrap();
    let protocol_str = get_uri_protocol_as_str(&protocol_unwrapped);
    let mut page = Page::new(Link {
        scope: Some(UriScope::Root),
        protocol: protocol.clone(),
        uri,
        source_tag: None,
    });
    let page = lib::recursive_load_page_and_get_links(run_config, lib::LoadPageArguments {
        host: page.get_uri().host().unwrap().into(),
        protocol: protocol_str.into(),
        known_links: vec![],
        page,
        same_domain_only: true,
        depth: 1
    })
        .await;
    println!("Finished.");
    println!("Tarantula result:\n{:?}", page.unwrap())
}
