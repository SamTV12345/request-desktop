use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Cookie{
    pub domain: Option<String>,
    pub expires: Option<String>,
    #[serde(rename = "httpOnly")]
    pub http_only: Option<bool>,
    #[serde(rename = "maxAge")]
    pub max_age: Option<String>,
    pub path: Option<String>,
    pub secure: Option<bool>,
    pub session: Option<bool>,
    #[serde(rename = "hostOnly")]
    pub host_only: Option<bool>,
    pub key: Option<String>,
    pub value: Option<String>,
    pub extensions: Option<Vec<String>>,
}
