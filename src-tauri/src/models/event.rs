use crate::models::script::Script;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Event {
    pub id: Option<String>,
    pub listen: String,
    pub script: Script,
    pub disabled: Option<bool>,
}