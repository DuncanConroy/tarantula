use std::sync::{Arc, Mutex};

use hyper::Uri;
use log::trace;

use crate::{LinkTypeChecker, UriProtocol, UriScope};

pub struct UriService {
    link_type_checker: Arc<Mutex<LinkTypeChecker>>,
}

unsafe impl Send for UriService {}

impl UriService {
    pub fn new(link_type_checker: Arc<Mutex<LinkTypeChecker>>) -> UriService {
        UriService { link_type_checker }
    }

    pub fn form_full_url(&self, protocol: &str, uri: &str, host: &str, parent_uri: &Option<String>) -> Uri {
        trace!("form_full_url {}, {}, {}, {:?}", protocol, uri, host, parent_uri);
        let to_uri = |input: &str| String::from(input).parse::<hyper::Uri>().unwrap();
        let do_normalize = |uri: &str, parent_uri: &Option<String>| -> Uri {
            let normalized_uri = normalize_url(uri.into(), parent_uri);
            let adjusted_uri = prefix_uri_with_forward_slash(&normalized_uri);
            to_uri(&create_uri_string(protocol, host, &adjusted_uri))
        };
        let link_type_checker = self.link_type_checker.lock().unwrap();
        if let Some(scope) = link_type_checker.get_uri_scope(host, uri) {
            return match scope {
                UriScope::Root => to_uri(&create_uri_string(protocol, host, "/")),
                UriScope::SameDomain => do_normalize(uri, parent_uri),
                UriScope::Anchor => do_normalize(uri, parent_uri),
                _ => {
                    if let Some(uri_protocol) = link_type_checker.get_uri_protocol(protocol, uri) {
                        if uri_protocol == UriProtocol::IMPLICIT {
                            return format!("{}:{}", protocol, uri).parse::<hyper::Uri>().unwrap();
                        }
                    }
                    to_uri(uri)
                }
            };
        }
        to_uri(uri)
    }
}

fn prefix_uri_with_forward_slash(uri: &str) -> String {
    if uri.starts_with("/") || uri.starts_with("http://") || uri.starts_with("https://") { uri.to_string() } else { format!("/{}", uri) }
}

fn create_uri_string(protocol: &str, host: &str, link: &str) -> String {
    let link_string = String::from(link);
    let url_string = if link_string.starts_with("http") {
        link_string.to_owned()
    } else {
        format!("{}://{}{}", protocol, host, link)
    };

    url_string
}

fn normalize_url(uri: String, parent_uri: &Option<String>) -> String {
    trace!("normalize uri: {}", uri);
    if !uri.contains("../") {
        return uri;
    }

    let absolute_uri = format!("{}{}", parent_uri.as_ref().unwrap(), uri);
    trace!("absolute: {}", absolute_uri);

    let parts = absolute_uri.split("/");
    let mut parts_out = vec![];

    trace!("parts: {:?}", parts);
    for current in parts {
        if current != ".." {
            parts_out.push(current);
        } else {
            parts_out.pop();
        }
        trace!("parts_out: {:?}", parts_out);
    }

    parts_out.join("/")
}

#[cfg(test)]
mod tests {
    use std::ops::Add;

    use super::*;

    #[test]
    fn form_full_url_returns_correct_uri() {
        let input = vec![
            ("/", "https://example.com/"),
            ("/account/login?redirect=https://example.com/", "https://example.com/account/login?redirect=https://example.com/"),
            ("/agb/", "https://example.com/agb/"),
            ("/ausgabe/example-com-62-mindful-leadership/", "https://example.com/ausgabe/example-com-62-mindful-leadership/"),
            ("#", "https://example.com/#"),
            ("#s-angle-down", "https://example.com/#s-angle-down"),
            ("/#foo", "https://example.com/#foo"),
            ("example.com", "https://example.com/"),
            ("https://example.com/", "https://example.com/"),
            ("https://example.com/ausgabe/example-com-59-straight-outta-office/", "https://example.com/ausgabe/example-com-59-straight-outta-office/"),
            ("https://example.com/events/", "https://example.com/events/"),
            ("https://faq.example.com/", "https://faq.example.com/"),
            ("https://twitter.com/example-com", "https://twitter.com/example-com"),
            ("mailto:support@example.com", "mailto:support@example.com"),
            ("//storage.googleapis.com/example.com/assets/foo.png", "https://storage.googleapis.com/example.com/assets/foo.png"),
        ];

        let host = "example.com";
        let link_type_checker = Arc::new(Mutex::new(LinkTypeChecker::new(host)));
        let instance = UriService::new(link_type_checker.clone());
        input.iter()
            .for_each(|(uri, expected)| {
                let result = instance.form_full_url("https", uri, host, &Some(String::from("")));
                let formatted = format!("{}{}", host, uri);
                let scope = link_type_checker.lock().unwrap().get_uri_scope(host, &formatted);
                assert_eq!(&result, expected, "{} should be {} :: {:?}", uri, expected, scope.unwrap());
            });
    }

    #[test]
    fn normalize_url() {
        let input = vec![
            ("https://www.example.com/about/appsecurity/tools/", "../../../about/appsecurity/research/presentations/", "https://www.example.com/about/appsecurity/research/presentations/"),
        ];

        let host = "example.com";
        let link_type_checker = Arc::new(Mutex::new(LinkTypeChecker::new(host)));
        let instance = UriService::new(link_type_checker.clone());
        input.iter()
            .for_each(|(parent_uri, uri, expected)| {
                let result = instance.form_full_url("https", uri, host, &Some(String::from("").add(parent_uri)));
                let formatted = format!("{}{}", host, uri);
                let scope = link_type_checker.lock().unwrap().get_uri_scope(host, &formatted);
                assert_eq!(&result, expected, "{} should be {} :: {:?}", uri, expected, scope.unwrap());
            });
    }
}