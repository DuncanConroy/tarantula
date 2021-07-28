use std::sync::Arc;

use hyper::Uri;
use log::trace;

use crate::{LinkTypeChecker, UriProtocol, UriScope};

pub struct UriService {
    link_type_checker: Arc<LinkTypeChecker>,
}

unsafe impl Send for UriService {}

impl UriService {
    pub fn new(link_type_checker: Arc<LinkTypeChecker>) -> UriService {
        UriService { link_type_checker }
    }

    pub fn form_full_url(&self, protocol: &str, uri: &str, host: &str, parent_uri: &Option<String>) -> Uri {
        trace!("form_full_url {}, {}, {}, {:?}", protocol, uri, host, parent_uri);
        let pre_cleaned_uri = pre_clean_uri(host, uri);
        trace!("pre_cleaned uri {}", pre_cleaned_uri);
        let to_uri = |input: &str| {
            match String::from(input).parse::<hyper::Uri>() {
                Ok(parsed_uri) => parsed_uri,
                Err(_) => try_autofix_invalid_url(input)
            }
        };
        let do_normalize = |uri: &str, parent_uri: &Option<String>| -> Uri {
            let normalized_uri = normalize_url(uri.into(), parent_uri);
            let adjusted_uri = prefix_uri_with_forward_slash(&normalized_uri);
            to_uri(&create_uri_string(protocol, host, &adjusted_uri))
        };

        if let Some(scope) = self.link_type_checker.get_uri_scope(host, &pre_cleaned_uri) {
            return match scope {
                UriScope::Root => to_uri(&create_uri_string(protocol, host, "/")),
                UriScope::SameDomain => do_normalize(&pre_cleaned_uri, parent_uri),
                UriScope::Anchor => do_normalize(&pre_cleaned_uri, parent_uri),
                _ => {
                    if let Some(uri_protocol) = self.link_type_checker.get_uri_protocol(protocol, &pre_cleaned_uri) {
                        if uri_protocol == UriProtocol::IMPLICIT {
                            return format!("{}:{}", protocol, pre_cleaned_uri).parse::<hyper::Uri>().unwrap();
                        }
                    }
                    to_uri(&pre_cleaned_uri)
                }
            };
        }
        to_uri(&pre_cleaned_uri)
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
        format!("{}://{}{}", protocol, host, link_string)
    };

    url_string
}

fn pre_clean_uri(host: &str, uri: &str) -> String {
    let mut cleaned_uri = String::from(uri);

    if cleaned_uri.contains("?") {
        let parts:Vec<_> = cleaned_uri.split("?").collect();
        let cleaned_front_part = pre_clean_uri(host, parts.first().unwrap());
        let cleaned_last_parts = urlencoding::encode(&parts[1..].join("")).into_owned()
            .replace("%3D","=");
        cleaned_uri = format!("{}?{}", cleaned_front_part, cleaned_last_parts);
    }

    let mut protocol="";
    if cleaned_uri.starts_with("http://") {
        protocol = "http://";
        cleaned_uri = cleaned_uri.replace("http://", "");
    } else if cleaned_uri.starts_with("https://") {
        protocol = "https://";
        cleaned_uri = cleaned_uri.replace("https://", "");
    } else if cleaned_uri.starts_with("//") {
        protocol = "//";
        cleaned_uri = cleaned_uri.replace("//", "");
    }

    while cleaned_uri.contains("//") {
        cleaned_uri = cleaned_uri.replace("//", "/");
    }

    if cleaned_uri.starts_with("/") && (host.ends_with("/") || protocol == "//") {
        cleaned_uri = cleaned_uri[1..].into();
    }

    format!("{}{}",protocol, cleaned_uri)
}

fn normalize_url(uri: String, parent_uri: &Option<String>) -> String {
    trace!("normalize uri: {}", uri);
    if !uri.contains("../") {
        return uri;
    }

    let mut modified_parent_uri = String::from("");
    if parent_uri.is_some() {
        modified_parent_uri = parent_uri.as_ref().unwrap().clone();
        if !modified_parent_uri.ends_with("/") {
            modified_parent_uri += "/"
        }
    }
    let absolute_uri = format!("{}{}", modified_parent_uri, uri);
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

fn try_autofix_invalid_url(uri: &str) -> Uri {
    let autofixed_uri = urlencoding::encode(uri).into_owned()
        .replace("%3A", ":")
        .replace("%2F", "/");

    match autofixed_uri.parse::<hyper::Uri>() {
        Ok(parse_uri) => parse_uri,
        Err(error_message) => {
            panic!("Problem with uri {}. Autofixing failed with {}: {}", uri, autofixed_uri, error_message)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Add;

    use super::*;

    #[test]
    fn form_full_url_returns_correct_uri() {
        let input = vec![
            ("/", "https://example.com/"),
            ("/account/login?redirect=https://example.com/", "https://example.com/account/login?redirect=https%3A%2F%2Fexample.com%2F"),
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
            ("/some invalid url/assets/my picture.png", "https://example.com/some%20invalid%20url/assets/my%20picture.png"),
        ];

        let host = "example.com";
        let link_type_checker = Arc::new(LinkTypeChecker::new(host));
        let instance = UriService::new(link_type_checker.clone());
        input.iter()
            .for_each(|(uri, expected)| {
                let result = instance.form_full_url("https", uri, host, &Some(String::from("")));
                let formatted = format!("{}{}", host, uri);
                let scope = link_type_checker.get_uri_scope(host, &formatted);
                assert_eq!(&result, expected, "{} should be {} :: {:?}", uri, expected, scope.unwrap());
            });
    }

    #[test]
    fn clean_and_normalize_url() {
        let input = vec![
            ("https://www.example.com/", "/foo/", "https://www.example.com/foo/"),
            ("https://www.example.com", "/foo/", "https://www.example.com/foo/"),
            ("https://www.example.com/", "//foo//", "https://foo/"),
            ("https://www.example.com/", "///////foo//////", "https://foo/"),
            ("https://www.example.com/", "http-headers-explained/", "https://www.example.com/http-headers-explained/"),
            ("https://www.example.com/about/appsecurity/tools/", "../../../about/appsecurity/research/presentations/", "https://www.example.com/about/appsecurity/research/presentations/"),
            ("https://www.example.com/about/appsecurity/tools", "../../../about/appsecurity/research/presentations/", "https://www.example.com/about/appsecurity/research/presentations/"),
        ];

        let host = "www.example.com";
        let link_type_checker = Arc::new(LinkTypeChecker::new(host));
        let instance = UriService::new(link_type_checker.clone());
        input.iter()
            .for_each(|(parent_uri, uri, expected)| {
                let result = instance.form_full_url("https", uri, host, &Some(String::from("").add(parent_uri)));
                assert_eq!(&result, expected, "{} should be {}", &result, expected);
            });
    }
}