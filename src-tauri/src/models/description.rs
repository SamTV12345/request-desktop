use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct PostmanDescription {
    pub content: Option<String>,
    pub version: Option<String>,
    pub r#type: Option<String>,
}