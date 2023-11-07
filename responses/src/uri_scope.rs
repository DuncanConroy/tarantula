use serde::Serialize;

#[derive(Debug, PartialEq, Eq, Clone, Serialize)]
pub enum UriScope {
    // /
    Root,
    // example.com/deeplink | deeplink | /deeplink
    SameDomain,
    // diffsub.example.com/deeplink
    DifferentSubDomain,
    // https://www.end-of-the-internet.com/
    External,
    // #somewhere
    Anchor,
    // mailto:foo.bar@example.com
    Mailto,
    // data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGP6AgAA+gD3odZZSQAAAABJRU5ErkJggg==
    EmbeddedImage,
    // javascript:function foo(){}
    Code,
    // somespecial:anycode
    UnknownPrefix,
}