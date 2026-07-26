//! Shared request/response model. Both the .bru and the IntelliJ .http parser
//! produce `Request`; the executor consumes it. Nothing else is a contract.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub name: String,
    pub method: String,
    pub url: String,
    /// Insertion-independent, but duplicates are allowed (Set-Cookie style).
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    pub body: Body,
    pub auth: Auth,
    /// `seq` in .bru, file order in .http. Used for sidebar ordering.
    pub seq: u32,
    pub pre_request_script: Option<String>,
    pub post_response_script: Option<String>,
    pub tests: Option<String>,
    pub asserts: Vec<Assert>,
    /// Anything the parser understood but the model has no field for.
    /// Keeps round-tripping honest instead of silently dropping user data.
    pub extra: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum Body {
    #[default]
    None,
    Json(String),
    Text(String),
    Xml(String),
    Sparql(String),
    Form(Vec<(String, String)>),
    MultipartForm(Vec<MultipartField>),
    GraphQl {
        query: String,
        variables: String,
    },
    File {
        path: String,
        content_type: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultipartField {
    pub name: String,
    pub value: String,
    /// Set when the field is a file upload rather than a plain value.
    pub file_path: Option<String>,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum Auth {
    #[default]
    None,
    Inherit,
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
    },
    ApiKey {
        key: String,
        value: String,
        placement: ApiKeyPlacement,
    },
    OAuth2(Box<OAuth2>),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum ApiKeyPlacement {
    #[default]
    Header,
    Query,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OAuth2 {
    pub grant_type: String,
    pub access_token_url: String,
    pub authorization_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scope: String,
    pub callback_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assert {
    pub expr: String,
    pub operator: String,
    pub value: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Response {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub duration_ms: u128,
    pub size_bytes: usize,
}

/// A folder of requests on disk. `.bru` collections and loose `.http` files
/// both flatten into this.
#[derive(Debug, Clone, Default)]
pub struct Collection {
    pub name: String,
    pub root: std::path::PathBuf,
    pub items: Vec<Item>,
    pub environments: Vec<Environment>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Request {
        path: std::path::PathBuf,
        /// Boxed because `Request` is large and a collection tree holds mostly
        /// `Folder`s; an unboxed variant made every `Item` pay for the biggest.
        request: Box<Request>,
    },
    Folder {
        name: String,
        items: Vec<Item>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct Environment {
    pub name: String,
    pub vars: Vec<(String, String, bool)>, // name, value, secret
}
