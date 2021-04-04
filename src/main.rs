use std::process;

use lib::*;
use linkresult::{get_uri_protocol, get_uri_protocol_as_str, Link, UriScope};
use page::*;
use std::str::FromStr;

use clap::App;
use clap::load_yaml;

mod lib;

#[tokio::main]
async fn main() -> DynResult<()> {
    pretty_env_logger::init();
    // todo: don't load non-html files, e.g. PNG
    // todo: see docs folder for refactoring
    // todo: respect robots.txt file
    // todo: add option to keep or dispose of html source
    // TODO: multi-threaded
    process().await;
    Ok(())
}

fn parse_runconfig_from_args() -> Result<RunConfig, &'static str> {
    let yaml = load_yaml!("cli.yaml");
    let matches = App::from_yaml(yaml).get_matches();

    let url = matches.value_of("URL").unwrap();
    let mut run_config = RunConfig::new(url.to_string());

    run_config.follow_redirects = !matches.is_present("ignore_redirects");
    run_config.ignore_robots_txt =  matches.is_present("ignore_robots_txt");

    if let Some(maximum_depth) = matches.value_of("maximum_depth") {
        run_config.maximum_depth = u8::from_str(&maximum_depth).unwrap()
    }
    if let Some(maximum_redirects) = matches.value_of("maximum_redirects") {
        run_config.maximum_redirects = u8::from_str(&maximum_redirects).unwrap()
    }
    if let Some(keep_html_in_memory) = matches.value_of("keep_html_in_memory") {
        run_config.keep_html_in_memory = true
    }

    println!("{:#?}", run_config);

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
    let page = Page::new(Link {
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
