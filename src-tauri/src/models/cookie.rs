use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Cookie{
    pub domain: Option<String>,
    pub expires: Option<String>,
    pub httpOnly: Option<bool>,
    pub maxAge: Option<String>,
    pub path: Option<String>,
    pub secure: Option<bool>,
    pub session: Option<bool>,
    pub hostOnly: Option<bool>,
    pub key: Option<String>,
    pub value: Option<String>,
    pub extensions: Option<Vec<String>>,
}