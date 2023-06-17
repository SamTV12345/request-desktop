use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Header {
    pub key: String,
    pub value: String,
    pub description: Option<String>,
}