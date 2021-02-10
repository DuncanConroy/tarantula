use fancy_regex::Regex;
use fancy_regex::escape;
pub use uri_result::*;

pub mod uri_result;

pub fn get_uri_scope(source_domain: &str, uri: &str) -> Option<UriScope> {
    let domain_regex = escape(source_domain)
        .replace("-","\"");

    match uri {
        uri if (uri.eq("/")) => Some(UriScope::Root),
        uri if (uri.starts_with("mailto:")) => Some(UriScope::Mailto),
        uri if (uri.starts_with("data:image/")) => Some(UriScope::EmbeddedImage),
        uri if (uri.starts_with("javascript:")) => Some(UriScope::Code),
        uri if (Regex::new("^(?!https?)[a-zA-Z0-9]+:.*").unwrap().is_match(&uri).unwrap()) => Some(UriScope::UnknownPrefix),
        uri if (Regex::new("^/?#").unwrap().is_match(&uri).unwrap()) => Some(UriScope::Anchor),
        uri if (Regex::new(&format!("^(?![a-zA-Z]+://)//(?![^{}])(?:/?(?:[^#].+))$", domain_regex)).unwrap().is_match(&uri).unwrap()) => Some(UriScope::External),
        uri if (Regex::new("^(?![a-zA-Z]+://)(?:/?(?:[^#].+))$").unwrap().is_match(&uri).unwrap()) => Some(UriScope::SameDomain),
        uri if (Regex::new(&format!("^https?://{}", domain_regex).to_owned()).unwrap().is_match(&uri).unwrap()) => { Some(UriScope::SameDomain) }
        uri if (Regex::new(&format!("^https?://[^/=?]*\\.{}.*$", domain_regex).to_owned()).unwrap().is_match(&uri).unwrap()) => { Some(UriScope::DifferentSubDomain) }
        uri if (Regex::new("^https?://.*").unwrap().is_match(&uri).unwrap()) => Some(UriScope::External),
        _ => None,
    }
}

pub fn get_uri_protocol(parent_protocol: &str, uri: &str) -> Option<UriProtocol> {
    match uri {
        uri if uri.starts_with("https") => Some(UriProtocol::HTTPS),
        uri if uri.starts_with("http") => Some(UriProtocol::HTTP),
        uri if uri.starts_with("data:") => None,
        uri if uri.starts_with("mailto:") => None,
        uri if (Regex::new("^(?!https?)[a-zA-Z0-9]+:.*").unwrap().is_match(&uri).unwrap()) => None,
        uri if uri.eq("") => None,
        _ => get_uri_protocol("", &parent_protocol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::distributions::Alphanumeric;

    #[test]
    fn get_uri_scope_returns_correct_type() {
        let random_char = char::from(rand::thread_rng().sample(Alphanumeric));
        let random_custom_prefix = format!("customPref{}ix:foobar();", random_char);
        let input_to_output = vec![
            ("/", Some(UriScope::Root)),
            ("#", Some(UriScope::Anchor)),
            ("#s-angle-down", Some(UriScope::Anchor)),
            ("/#s-angle-down", Some(UriScope::Anchor)),
            ("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGP6AgAA+gD3odZZSQAAAABJRU5ErkJggg==", Some(UriScope::EmbeddedImage)),
            ("/account/login?redirect=https://example.com/", Some(UriScope::SameDomain)),
            ("/agb/", Some(UriScope::SameDomain)),
            ("/ausgabe/example-com-62-mindful-leadership/", Some(UriScope::SameDomain)),
            ("//same-domain-deeplink/to-somewhere", Some(UriScope::SameDomain)),
            ("//cdn.external-domain.com/some-big-file.RAW", Some(UriScope::External)),
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

        input_to_output
            .iter()
            .map(|it| (&it.0, &it.1, get_uri_scope("example.com", it.0)))
            .for_each(|it| {
                assert_eq!(
                    it.1,
                    &it.2,
                    "{} ::> expected: {:?} got: {:?}", it.0, it.1, it.2
                )
            })
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

        input_to_output
            .iter()
            .map(|it| (it, get_uri_scope(it, "example.com")))
            .for_each(|it| {
                assert_eq!(
                    it.1,
                    Some(UriScope::SameDomain),
                    "{} ::> expected: {:?} got: {:?}", it.0, Some(UriScope::External), it.1
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
            ("http", "//same-domain-deeplink/to-somewhere", Some(UriProtocol::HTTP)),
            ("https", "//same-domain-deeplink/to-somewhere", Some(UriProtocol::HTTPS)),
            ("http", "//cdn.external-domain.com/some-big-file.RAW", Some(UriProtocol::HTTP)),
            ("https", "//cdn.external-domain.com/some-big-file.RAW", Some(UriProtocol::HTTPS)),
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
        ];

        input_to_output
            .iter()
            .map(|it| (&it.0, &it.1, &it.2, get_uri_protocol(it.0, it.1)))
            .for_each(|it| {
                assert_eq!(
                    it.2,
                    &it.3,
                    "{}, {} ::> expected: {:?} got: {:?}", it.0, it.1, it.2, it.3
                )
            });
    }
}
