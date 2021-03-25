use hyper::Uri;

use crate::{get_uri_protocol, get_uri_scope, UriProtocol, UriScope};

pub fn form_full_url(protocol: &str, uri: &str, host: &str) -> String {
    if let Some(scope) = get_uri_scope(host, uri) {
        return match scope {
            UriScope::Root => {
                create_uri_string(protocol, host, "/")
            }
            UriScope::SameDomain => {
                let adjusted_uri = adjust_uri(uri);
                create_uri_string(protocol, host, &adjusted_uri)
            }
            UriScope::Anchor => {
                let adjusted_uri = adjust_uri(uri);
                create_uri_string(protocol, host, &adjusted_uri)
            }
            _ => {
                if let Some(uri_protocol) = get_uri_protocol(protocol, uri) {
                    if uri_protocol == UriProtocol::IMPLICIT {
                        format!("{}:{}", protocol, uri)
                    } else {
                        String::from(uri)
                    }
                } else {
                    String::from(uri)
                }
            }
        };
    }
    String::from(uri)
}

fn adjust_uri(uri: &str) -> String {
    if uri.starts_with("/") || uri.starts_with("http://") || uri.starts_with("https://") { uri.to_string() } else { format!("/{}", uri) }
}

pub fn create_uri_string(protocol: &str, host: &str, link: &str) -> String {
    let link_string = String::from(link);
    let url_string = if link_string.starts_with("http") {
        link_string.to_owned()
    } else {
        format!("{}://{}{}", protocol, host, link)
    };

    url_string
}

pub fn create_uri(protocol: &str, host: &str, link: &String) -> Uri {
    let uri_string = create_uri_string(protocol, host, &link);
    uri_string.parse::<hyper::Uri>().unwrap()
}

#[cfg(test)]
mod tests {
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
        input.iter()
            .for_each(|(uri, expected)| {
                let result = form_full_url("https", uri, host);
                let formatted = format!("{}{}", host, uri);
                let scope = get_uri_scope(host, &formatted);
                assert_eq!(&result, expected, "{} should be {} :: {:?}", uri, expected, scope.unwrap());
            });
    }
}