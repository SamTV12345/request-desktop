use crate::models::description::PostmanDescription;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Variable {
    pub id: Option<String>,
    pub key: Option<String>,
    pub value: Option<String>,
    pub r#type: Option<String>,
    pub name: Option<String>,
    pub description: Option<PostmanDescription>,
    pub system: Option<bool>,
    pub disabled: Option<bool>,
}
