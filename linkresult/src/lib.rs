use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use fancy_regex::escape;
use fancy_regex::Regex;

pub use uri_result::*;

pub mod uri_result;
pub mod uri_service;

#[derive(Eq, PartialEq, Hash)]
enum RegexType {
    Anchor,
    DifferentSubdomain,
    DifferentSubdomainWithProtocol,
    EXTERNAL,
    ExternalWithProtocol,
    SameDomain,
    SameDomainWithProtocol,
    UnknownPrefix,
}

pub struct LinkTypeChecker {
    regexes: Arc<HashMap<RegexType, Regex>>,
}

impl LinkTypeChecker {
    pub fn new(host: &str) -> LinkTypeChecker {
        let domain_regex = escape(host).replace("-", "\"");
        let mut hash_map = HashMap::with_capacity(8);
        hash_map.insert(RegexType::Anchor, Regex::new("^/?#").unwrap());
        hash_map.insert(RegexType::DifferentSubdomain, Regex::new(&format!("^//.+\\.(?:{}).*$", domain_regex)).unwrap());
        hash_map.insert(RegexType::DifferentSubdomainWithProtocol, Regex::new(&format!("^https?://[^/=?]*\\.{}.*$", domain_regex).to_owned()).unwrap());
        hash_map.insert(RegexType::EXTERNAL, Regex::new(&format!("^//(?!{}).*$", domain_regex)).unwrap());
        hash_map.insert(RegexType::ExternalWithProtocol, Regex::new("^https?://.*").unwrap());
        hash_map.insert(RegexType::SameDomain, Regex::new("^(?![a-zA-Z]+://)(?:/?(?:[^#].+))$").unwrap());
        hash_map.insert(RegexType::SameDomainWithProtocol, Regex::new(&format!("^https?://{}", domain_regex).to_owned()).unwrap());
        hash_map.insert(RegexType::UnknownPrefix, Regex::new("^(?!https?)[a-zA-Z0-9]+:.*").unwrap());

        LinkTypeChecker {
            regexes: Arc::new(hash_map)
        }
    }

    fn is_match(&self, key: RegexType, uri: &str) -> bool {
        self.regexes.get(&key).unwrap().is_match(uri).unwrap()
    }

    pub fn get_uri_scope(&self, host: &str, uri: &str) -> Option<UriScope> {
        match uri {
            uri if uri.eq("/") => Some(UriScope::Root),
            uri if uri.eq(host) => Some(UriScope::Root),
            uri if uri.eq(&format!("{}/", host)) => Some(UriScope::Root),
            uri if uri.eq(&format!("http://{}", host)) => Some(UriScope::Root),
            uri if uri.eq(&format!("http://{}/", host)) => Some(UriScope::Root),
            uri if uri.eq(&format!("https://{}", host)) => Some(UriScope::Root),
            uri if uri.eq(&format!("https://{}/", host)) => Some(UriScope::Root),
            uri if uri.starts_with("mailto:") => Some(UriScope::Mailto),
            uri if uri.starts_with("data:image/") => Some(UriScope::EmbeddedImage),
            uri if uri.starts_with("javascript:") => Some(UriScope::Code),
            uri if self.is_match(RegexType::UnknownPrefix, uri) => { Some(UriScope::UnknownPrefix) }
            uri if self.is_match(RegexType::Anchor, uri) => Some(UriScope::Anchor),
            uri if self.is_match(RegexType::DifferentSubdomain, uri) => { Some(UriScope::DifferentSubDomain) }
            uri if self.is_match(RegexType::EXTERNAL, uri) => { Some(UriScope::External) }
            uri if self.is_match(RegexType::SameDomain, uri) => { Some(UriScope::SameDomain) }
            uri if self.is_match(RegexType::SameDomainWithProtocol, uri) => { Some(UriScope::SameDomain) }
            uri if self.is_match(RegexType::DifferentSubdomainWithProtocol, uri) => { Some(UriScope::DifferentSubDomain) }
            uri if self.is_match(RegexType::ExternalWithProtocol, uri) => { Some(UriScope::External) }
            _ => None,
        }
    }

    pub fn get_uri_protocol(&self, parent_protocol: &str, uri: &str) -> Option<UriProtocol> {
        match uri {
            uri if uri.starts_with("https") => Some(UriProtocol::HTTPS),
            uri if uri.starts_with("http") => Some(UriProtocol::HTTP),
            uri if uri.starts_with("data:") => None,
            uri if uri.starts_with("mailto:") => None,
            uri if self.is_match(RegexType::UnknownPrefix, uri) => None,
            uri if uri.eq("") => None,
            uri if uri.starts_with("//") => Some(UriProtocol::IMPLICIT),
            _ => self.get_uri_protocol("", parent_protocol),
        }
    }

    pub fn get_uri_protocol_as_str(protocol: &UriProtocol) -> &str {
        match protocol {
            UriProtocol::HTTP => "http",
            UriProtocol::HTTPS => "https",
            _ => "https",
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::distributions::Alphanumeric;
    use rand::Rng;

    use super::*;

    #[test]
    fn get_uri_scope_returns_correct_type() {
        let random_char = char::from(rand::thread_rng().sample(Alphanumeric));
        let random_custom_prefix = format!("customPref{}ix:foobar();", random_char);
        let input_to_output = vec![
            ("/", Some(UriScope::Root)),
            ("example.com", Some(UriScope::Root)),
            ("example.com/", Some(UriScope::Root)),
            ("http://example.com", Some(UriScope::Root)),
            ("http://example.com/", Some(UriScope::Root)),
            ("https://example.com", Some(UriScope::Root)),
            ("https://example.com/", Some(UriScope::Root)),
            ("#", Some(UriScope::Anchor)),
            ("#s-angle-down", Some(UriScope::Anchor)),
            ("/#s-angle-down", Some(UriScope::Anchor)),
            ("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGP6AgAA+gD3odZZSQAAAABJRU5ErkJggg==", Some(UriScope::EmbeddedImage)),
            ("/account/login?redirect=https://example.com/", Some(UriScope::SameDomain)),
            ("/agb/", Some(UriScope::SameDomain)),
            ("/ausgabe/example-com-62-mindful-leadership/", Some(UriScope::SameDomain)),
            ("//cdn.external-domain.com/example.com/some-big-file.RAW", Some(UriScope::External)),
            ("//storage.googleapis.com/example.com/foo.png", Some(UriScope::External)),
            ("//foo.example.com/some-file.png", Some(UriScope::DifferentSubDomain)),
            ("somefile/some.txt", Some(UriScope::SameDomain)),
            ("http://feeds.soundcloud.com/users/soundcloud:users:213461595/sounds.rss", Some(UriScope::External)),
            ("https://example-com.cloudfront.net/example-com/images/icons/example-com-apple-touch-120x120.png", Some(UriScope::External)),
            ("https://faq.example.com/", Some(UriScope::DifferentSubDomain)),
            ("https://faq.example.com/deep-link?https://example.com", Some(UriScope::DifferentSubDomain)),
            ("https://www.somewhere.com?source=https%3A%2F%2F//faq.example.com/", Some(UriScope::External)),
            ("https://www.somewhere.com/?source=https://faq.example.com/", Some(UriScope::External)),
            ("https://www.somewhere.com?https://faq.example.com/", Some(UriScope::External)),
            ("https://getpocket.com/edit.php?url=https%3A%2F%2Fexample.com%2Fnews%2Fchangerider-karriereknick-fuer-1351665%2F%3Futm_source%3Dpocket%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons", Some(UriScope::External)),
            ("https://medium.com/@example-combackstageblog", Some(UriScope::External)),
            ("https://example.com/ausgabe/example-com-59-straight-outta-office/", Some(UriScope::SameDomain)),
            ("https://example.com/rss.xml", Some(UriScope::SameDomain)),
            ("https://example.com/team", Some(UriScope::SameDomain)),
            ("https://twitter.com/intent/tweet?text=Googles%20Mobile-First-Indexing%3A%20Das%20sollten%20SEO-Experten%20unbedingt%20beachten&url=https%3A%2F%2Fexample.com%2Fmagazin%2Fgoogles-mobile-first-indexing-250229%2F%3Futm_source%3Dtwitter.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons&via=example-com&lang=de", Some(UriScope::External)),
            ("https://twitter.com/intent/tweet?text=Segway-Ninebot%3A%20Den%20neuen%20E-Scooter%20Ninebot%20S%20Max%20kannst%20du%20zum%20Gokart%20machen&url=https%3A%2F%2Fexample.com%2Fnews%2Fsegway-ninebot-s-max-gokart-1351854%2F%3Futm_source%3Dtwitter.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons&via=example-com&lang=de", Some(UriScope::External)),
            ("https://www.kununu.com/de/example-com/", Some(UriScope::External)),
            ("https://www.linkedin.com/shareArticle?mini=true&url=https%3A%2F%2Fexample.com%2Fnews%2Feu-leistungsschutzrecht-frankreich-publisher-google-news-1351802%2F%3Futm_source%3Dlinkedin.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons", Some(UriScope::External)),
            ("https://www.xing.com/spi/shares/new?url=https%3A%2F%2Fexample.com%2Fmagazin%2Fgoogles-mobile-first-indexing-250229%2F%3Futm_source%3Dxing.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons", Some(UriScope::External)),
            ("mailto:support@example.com", Some(UriScope::Mailto)),
            ("https://example-com.cloudfront.net/example-com/styles/main-1234567890.css", Some(UriScope::External)),
            ("https://www.a-b-c.com", Some(UriScope::External)),
            ("javascript:fef4ee", Some(UriScope::Code)),
            ("java:nothing", Some(UriScope::UnknownPrefix)),
            ("customPrefix:nothing", Some(UriScope::UnknownPrefix)),
            (random_custom_prefix.as_str(), Some(UriScope::UnknownPrefix)),
            ("", None),
        ];

        let instance = LinkTypeChecker::new("example.com");

        input_to_output
            .iter()
            .map(|it| (&it.0, &it.1, instance.get_uri_scope("example.com", it.0)))
            .for_each(|it|
                assert_eq!(
                    it.1, &it.2,
                    "{} ::> expected: {:?} got: {:?}",
                    it.0, it.1, it.2
                )
            )
    }

    #[test]
    fn get_uri_protocol_runs_with_different_source_domains() {
        let input_to_output = vec![
            "http://feeds.soundcloud.com/users/soundcloud:users:213461595/sounds.rss",
            "https://example-com.cloudfront.net/example-com/images/icons/example-com-apple-touch-120x120.png",
            "https://faq.example.com/",
            "https://faq.example.com/deep-link?https://example.com",
            "https://www.somewhere.com?source=https%3A%2F%2F//faq.example.com/",
            "https://www.somewhere.com/?source=https://faq.example.com/",
            "https://www.somewhere.com?https://faq.example.com/",
            "https://getpocket.com/edit.php?url=https%3A%2F%2Fexample.com%2Fnews%2Fchangerider-karriereknick-fuer-1351665%2F%3Futm_source%3Dpocket%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons",
            "https://medium.com/@example-combackstageblog",
            "https://example.com/ausgabe/example-com-59-straight-outta-office/",
            "https://example.com/rss.xml",
            "https://example.com/team",
            "https://twitter.com/intent/tweet?text=Googles%20Mobile-First-Indexing%3A%20Das%20sollten%20SEO-Experten%20unbedingt%20beachten&url=https%3A%2F%2Fexample.com%2Fmagazin%2Fgoogles-mobile-first-indexing-250229%2F%3Futm_source%3Dtwitter.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons&via=example-com&lang=de",
            "https://twitter.com/intent/tweet?text=Segway-Ninebot%3A%20Den%20neuen%20E-Scooter%20Ninebot%20S%20Max%20kannst%20du%20zum%20Gokart%20machen&url=https%3A%2F%2Fexample.com%2Fnews%2Fsegway-ninebot-s-max-gokart-1351854%2F%3Futm_source%3Dtwitter.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons&via=example-com&lang=de",
            "https://www.kununu.com/de/example-com/",
            "https://www.linkedin.com/shareArticle?mini=true&url=https%3A%2F%2Fexample.com%2Fnews%2Feu-leistungsschutzrecht-frankreich-publisher-google-news-1351802%2F%3Futm_source%3Dlinkedin.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons",
            "https://www.xing.com/spi/shares/new?url=https%3A%2F%2Fexample.com%2Fmagazin%2Fgoogles-mobile-first-indexing-250229%2F%3Futm_source%3Dxing.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons",
            "https://example-com.cloudfront.net/example-com/styles/main-1234567890.css",
            "https://www.a-b-c.com",
        ];

        let instance = LinkTypeChecker::new("example.com");

        input_to_output
            .iter()
            .map(|it| (it, instance.get_uri_scope(it, "example.com")))
            .for_each(|it| {
                assert_eq!(
                    it.1,
                    Some(UriScope::SameDomain),
                    "{} ::> expected: {:?} got: {:?}",
                    it.0,
                    Some(UriScope::External),
                    it.1
                )
            })
    }

    #[test]
    fn get_uri_protocol_returns_correct_protocol() {
        let random_char = char::from(rand::thread_rng().sample(Alphanumeric));
        let random_custom_prefix = format!("customPref{}ix:foobar();", random_char);

        let input_to_output = vec![
            // (parent_protocol, uri, expected_protocol)
            ("http", "/", Some(UriProtocol::HTTP)),
            ("https", "/", Some(UriProtocol::HTTPS)),
            ("http", "#", Some(UriProtocol::HTTP)),
            ("https", "#", Some(UriProtocol::HTTPS)),
            ("http", "#s-angle-down", Some(UriProtocol::HTTP)),
            ("https", "#s-angle-down", Some(UriProtocol::HTTPS)),
            ("http", "/#s-angle-down", Some(UriProtocol::HTTP)),
            ("https", "/#s-angle-down", Some(UriProtocol::HTTPS)),
            ("http", "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGP6AgAA+gD3odZZSQAAAABJRU5ErkJggg==", None),
            ("https", "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGP6AgAA+gD3odZZSQAAAABJRU5ErkJggg==", None),
            ("http", "/account/login?redirect=https://example.com/", Some(UriProtocol::HTTP)),
            ("https", "/account/login?redirect=https://example.com/", Some(UriProtocol::HTTPS)),
            ("http", "//same-domain-deeplink/to-somewhere", Some(UriProtocol::IMPLICIT)),
            ("https", "//same-domain-deeplink/to-somewhere", Some(UriProtocol::IMPLICIT)),
            ("http", "//cdn.external-domain.com/some-big-file.RAW", Some(UriProtocol::IMPLICIT)),
            ("https", "//cdn.external-domain.com/some-big-file.RAW", Some(UriProtocol::IMPLICIT)),
            ("http", "somefile/some.txt", Some(UriProtocol::HTTP)),
            ("https", "somefile/some.txt", Some(UriProtocol::HTTPS)),
            ("https", "http://feeds.soundcloud.com/users/soundcloud:users:213461595/sounds.rss", Some(UriProtocol::HTTP)),
            ("http", "https://example-com.cloudfront.net/example-com/images/icons/example-com-apple-touch-120x120.png", Some(UriProtocol::HTTPS)),
            ("http", "https://example.com/rss.xml", Some(UriProtocol::HTTPS)),
            ("http", "mailto:support@example.com", None),
            ("https", "mailto:support@example.com", None),
            ("https", "javascript:foobar();", None),
            ("https", random_custom_prefix.as_str(), None),
            ("http", "", None),
            ("https", "", None),
            ("https", "//example.com", Some(UriProtocol::IMPLICIT)),
            ("http", "//example.com", Some(UriProtocol::IMPLICIT)),
        ];

        let instance = LinkTypeChecker::new("example.com");

        input_to_output
            .iter()
            .map(|it| (&it.0, &it.1, &it.2, instance.get_uri_protocol(it.0, it.1)))
            .for_each(|it| {
                assert_eq!(
                    it.2, &it.3,
                    "{}, {} ::> expected: {:?} got: {:?}",
                    it.0, it.1, it.2, it.3
                )
            });
    }
}
