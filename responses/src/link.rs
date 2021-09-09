use serde::Serialize;

use crate::uri_protocol::UriProtocol;
use crate::uri_scope::UriScope;

#[derive(Debug, Clone, Serialize)]
pub struct Link {
    pub uri: String,
    pub scope: Option<UriScope>,
    pub protocol: Option<UriProtocol>,
    pub source_tag: Option<String>,
}

impl Link {
    pub fn from_str(s: &str) -> Link {
        Link {
            uri: s.trim().to_string(),
            scope: None,
            protocol: None,
            source_tag: None,
        }
    }

    pub fn from_str_with_scope(s: &str, scope: Option<UriScope>) -> Link {
        Link {
            uri: s.trim().to_string(),
            scope,
            protocol: None,
            source_tag: None,
        }
    }
}

unsafe impl Send for Link {}

unsafe impl Sync for Link {}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        self.uri == other.uri
    }

    fn ne(&self, other: &Self) -> bool {
        !self.eq(other)
    }
}