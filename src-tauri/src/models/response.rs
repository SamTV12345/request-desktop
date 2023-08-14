use crate::models::cookie::Cookie;
use crate::models::header::Header;
use crate::models::request::Request;
use crate::models::request_body::RequestBody;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]

pub struct PostmanResponse{
    pub id: Option<String>,
    #[serde(rename = "originalRequest")]
    pub original_request: Option<Request>,
    #[serde(rename = "responseTime")]
    pub response_time: Option<i32>,
    pub timings: Option<Vec<String>>,
    pub header: Option<Vec<Header>>,
    pub cookie: Option<Vec<Cookie>>,
    pub body: Option<RequestBody>,
    pub status: Option<String>,
    pub code: Option<i32>,
    pub _postman_previewlanguage: Option<String>,
    pub _postman_previewtype: Option<String>,
    pub _postman_previewcode: Option<String>,
    pub _postman_previewerred: Option<bool>,
    pub _postman_previeweractive: Option<bool>,
    pub _postman_previewerdisplayed: Option<bool>,
}
