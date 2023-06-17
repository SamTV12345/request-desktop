use crate::models::query_param::QueryParam;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct RequestBody {
    pub mode: String,
    pub raw: String,
    pub urlencoded: Option<Vec<QueryParam>>,
    pub formdata: Option<Vec<QueryParam>>,
    pub file: Option<Vec<QueryParam>>,
    pub options: Option<Vec<QueryParam>>,
    pub graphql: Option<Vec<QueryParam>>,
    pub disabled: Option<bool>,
    pub description: Option<String>,
}