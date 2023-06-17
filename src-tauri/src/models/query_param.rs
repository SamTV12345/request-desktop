use serde::{Deserialize, Serialize};


#[derive(Serialize, Deserialize)]
pub struct QueryParam {
    pub key: String,
    pub value: String,
    pub disabled: Option<bool>,
    pub description: Option<String>,
}