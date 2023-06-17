use serde::{Deserialize, Serialize};
use crate::models::query_param::QueryParam;
use crate::models::variable::Variable;

#[derive(Serialize, Deserialize)]
pub struct Url{
    pub raw: String,
    pub protocol: Option<String>,
    pub host: Vec<String>,
    pub port: Option<String>,
    pub path: Vec<String>,
    pub query: Option<Vec<QueryParam>>,
    pub hash: Option<String>,
    pub variable: Option<Vec<Variable>>,
}