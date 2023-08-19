use std::collections::HashMap;


use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ResponseFromCall {
    pub status: String,
    pub body: String,
    pub headers: HashMap<String, String>,
    pub cookies: HashMap<String, String>,
    pub duration: TimeMeasures,
}

#[derive(Serialize, Deserialize)]
pub struct TimeMeasures{
    pub response_duration: String,
    pub duration: String,
}