use serde::{Deserialize, Serialize};
use crate::models::url::Url;

#[derive(Serialize, Deserialize)]
pub struct Script{
    pub id: Option<String>,
    pub r#type: Option<String>,
    pub exec: Option<Vec<String>>,
    pub src: Option<Url>,
    pub name: Option<String>,
}