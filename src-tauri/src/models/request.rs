use crate::models::header::Header;
use crate::models::request_body::RequestBody;
use crate::models::url::Url;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]

pub struct Request {
    pub url: Url,
    pub method: String,
    pub header: Vec<Header>,
    pub body: RequestBody,
    pub description: Option<String>,
}