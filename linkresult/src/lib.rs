use fancy_regex::Regex;
pub use uri_result::*;

pub mod uri_result;

pub fn get_uri_destination(source_domain: &str, uri: &str) -> Option<UriDestination> {

    match uri {
        uri if (uri.eq("/")) => Some(UriDestination::Root),
        uri if (uri.starts_with("mailto:")) => Some(UriDestination::Mailto),
        uri if (uri.starts_with("data:image/")) => Some(UriDestination::EmbeddedImage),
        uri if (Regex::new("^/?#").unwrap().is_match(&uri).unwrap()) => Some(UriDestination::Anchor),
        uri if (Regex::new(&format!("^//[^{}](?:/?(?:[^#].+\\.+.*))$", source_domain)).unwrap().is_match(&uri).unwrap()) => Some(UriDestination::External),
        uri if (Regex::new("^(?![a-zA-Z]+://)(?:/?(?:[^#].+))$").unwrap().is_match(&uri).unwrap()) => Some(UriDestination::SameDomain),
        uri if (Regex::new(&format!("^https?://{}", source_domain).to_owned()).unwrap().is_match(&uri).unwrap()) => { Some(UriDestination::SameDomain) }
        uri if (Regex::new(&format!("^https?://[^/=?]*\\.{}.*$", source_domain).to_owned()).unwrap().is_match(&uri).unwrap()) => { Some(UriDestination::DifferentSubDomain) }
        uri if (Regex::new(&format!("^https?://[^/=?]*\\.[^{}].*", source_domain).to_owned()).unwrap().is_match(&uri).unwrap()) => { Some(UriDestination::External) }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_destination_returns_correct_type() {
        let input_to_output = vec![
            ("/", Some(UriDestination::Root)),
            ("#", Some(UriDestination::Anchor)),
            ("#s-angle-down", Some(UriDestination::Anchor)),
            ("/#s-angle-down", Some(UriDestination::Anchor)),
            ("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGP6AgAA+gD3odZZSQAAAABJRU5ErkJggg==", Some(UriDestination::EmbeddedImage)),
            ("/account/login?redirect=https://t3n.de/", Some(UriDestination::SameDomain)),
            ("/agb/", Some(UriDestination::SameDomain)),
            ("/ausgabe/t3n-62-mindful-leadership/", Some(UriDestination::SameDomain)),
            ("//same-domain-deeplink/to-somewhere", Some(UriDestination::SameDomain)),
            ("//cdn.external-domain.com/some-big-file.RAW", Some(UriDestination::External)),
            ("somefile/some.txt", Some(UriDestination::SameDomain)),
            ("http://feeds.soundcloud.com/users/soundcloud:users:213461595/sounds.rss", Some(UriDestination::External)),
            ("https://d1quwwdmdfumn6.cloudfront.net/t3n/2018/images/icons/t3n-apple-touch-120x120.png", Some(UriDestination::External)),
            ("https://faq.t3n.de/", Some(UriDestination::DifferentSubDomain)),
            ("https://faq.t3n.de/deep-link?https://t3n.de", Some(UriDestination::DifferentSubDomain)),
            ("https://www.example.com?source=https%3A%2F%2F//faq.t3n.de/", Some(UriDestination::External)),
            ("https://www.example.com/?source=https://faq.t3n.de/", Some(UriDestination::External)),
            ("https://www.example.com?https://faq.t3n.de/", Some(UriDestination::External)),
            ("https://getpocket.com/edit.php?url=https%3A%2F%2Ft3n.de%2Fnews%2Fchangerider-karriereknick-fuer-1351665%2F%3Futm_source%3Dpocket%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons", Some(UriDestination::External)),
            ("https://medium.com/@t3nbackstageblog", Some(UriDestination::External)),
            ("https://t3n.de/ausgabe/t3n-59-straight-outta-office/", Some(UriDestination::SameDomain)),
            ("https://t3n.de/rss.xml", Some(UriDestination::SameDomain)),
            ("https://t3n.de/team", Some(UriDestination::SameDomain)),
            ("https://twitter.com/intent/tweet?text=Googles%20Mobile-First-Indexing%3A%20Das%20sollten%20SEO-Experten%20unbedingt%20beachten&url=https%3A%2F%2Ft3n.de%2Fmagazin%2Fgoogles-mobile-first-indexing-250229%2F%3Futm_source%3Dtwitter.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons&via=t3n&lang=de", Some(UriDestination::External)),
            ("https://twitter.com/intent/tweet?text=Segway-Ninebot%3A%20Den%20neuen%20E-Scooter%20Ninebot%20S%20Max%20kannst%20du%20zum%20Gokart%20machen&url=https%3A%2F%2Ft3n.de%2Fnews%2Fsegway-ninebot-s-max-gokart-1351854%2F%3Futm_source%3Dtwitter.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons&via=t3n&lang=de", Some(UriDestination::External)),
            ("https://www.kununu.com/de/t3n/", Some(UriDestination::External)),
            ("https://www.linkedin.com/shareArticle?mini=true&url=https%3A%2F%2Ft3n.de%2Fnews%2Feu-leistungsschutzrecht-frankreich-publisher-google-news-1351802%2F%3Futm_source%3Dlinkedin.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons", Some(UriDestination::External)),
            ("https://www.xing.com/spi/shares/new?url=https%3A%2F%2Ft3n.de%2Fmagazin%2Fgoogles-mobile-first-indexing-250229%2F%3Futm_source%3Dxing.com%26utm_medium%3Dsocial%26utm_campaign%3Dsocial-buttons", Some(UriDestination::External)),
            ("mailto:support@t3n.de", Some(UriDestination::Mailto)),
            ("", None),
        ];

        input_to_output
            .iter()
            .map(|it| (&it.0, &it.1, get_uri_destination("t3n.de", it.0)))
            .for_each(|it| {
                assert_eq!(
                    it.1,
                    &it.2,
                    "{} ::> expected: {:?} got: {:?}", it.0, it.1, it.2
                )
            })
    }
}
