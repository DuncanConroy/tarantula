use hyper::Uri;

use crate::{get_uri_protocol, get_uri_scope, UriProtocol, UriScope};

pub fn form_full_url(protocol: &str, uri: &str, host: &str, parent_uri: &Option<String>) -> Uri {
    println!("form_full_url {}, {}, {}, {:?}", protocol, uri, host, parent_uri);
    let to_uri = |input:&str| String::from(input).parse::<hyper::Uri>().unwrap();
    let do_normalize = |uri: &str, parent_uri: &Option<String>| -> Uri {
        let sanitized_uri = sanitize_url(uri.into(), parent_uri);
        let adjusted_uri = prefix_uri_with_forward_slash(&sanitized_uri);
        to_uri(&create_uri_string(protocol, host, &adjusted_uri))
    };
    if let Some(scope) = get_uri_scope(host, uri) {
        return match scope {
            UriScope::Root => to_uri(&create_uri_string(protocol, host, "/")),
            UriScope::SameDomain => do_normalize(uri, parent_uri),
            UriScope::Anchor =>  do_normalize(uri, parent_uri),
            _ => {
                if let Some(uri_protocol) = get_uri_protocol(protocol, uri) {
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

fn sanitize_url(uri: String, parent_uri: &Option<String>) -> String {
    println!("normalize uri: {}", uri);
    if !uri.contains("../") {
        return uri;
    }

    let absolute_uri = format!("{}{}", parent_uri.as_ref().unwrap(), uri);
    println!("absolute: {}", absolute_uri);

    let parts = absolute_uri.split("/");
    let mut parts_out = vec![];

    println!("parts: {:?}", parts);
    for current in parts {
        if current != ".." {
            parts_out.push(current);
        } else {
            parts_out.pop();
        }
        println!("parts_out: {:?}", parts_out);
    }

    parts_out.join("/")
}

pub fn create_uri(protocol: &str, host: &str, link: &String) -> Uri {
    let uri_string = create_uri_string(protocol, host, &link);
    uri_string.parse::<hyper::Uri>().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Add;

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
        input.iter()
            .for_each(|(uri, expected)| {
                let result = form_full_url("https", uri, host, &Some(String::from("")));
                let formatted = format!("{}{}", host, uri);
                let scope = get_uri_scope(host, &formatted);
                assert_eq!(&result, expected, "{} should be {} :: {:?}", uri, expected, scope.unwrap());
            });
    }

    #[test]
    fn normalize_url() {
        let input = vec![
            ("https://www.example.com/about/appsecurity/tools/", "../../../about/appsecurity/research/presentations/", "https://www.example.com/about/appsecurity/research/presentations/"),
        ];

        let host = "example.com";
        input.iter()
            .for_each(|(parent_uri, uri, expected)| {
                let result = form_full_url("https", uri, host, &Some(String::from("").add(parent_uri)));
                let formatted = format!("{}{}", host, uri);
                let scope = get_uri_scope(host, &formatted);
                assert_eq!(&result, expected, "{} should be {} :: {:?}", uri, expected, scope.unwrap());
            });
    }
}