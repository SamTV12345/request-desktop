use crate::models::postman_version::PostmanVersion;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Info {
    pub name: String,
    pub schema: String,
    pub _postman_id: Option<String>,
    pub version: Option<PostmanVersion>,
    pub description: Option<String>,
}