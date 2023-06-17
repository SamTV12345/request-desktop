use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct PostmanVersion{
    pub major: i32,
    pub minor: i32,
    pub patch: i32,
    pub identifier: Option<String>,
    pub meta: Option<String>,
}