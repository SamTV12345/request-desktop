use crate::models::event::Event;
use crate::models::postman_collection::ProtocolProfileBehavior;
use crate::models::request::Request;
use crate::models::response::PostmanResponse;
use crate::models::variable::Variable;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Item{
    pub id: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub variable: Option<Vec<Variable>>,
    pub event: Option<Vec<Event>>,
    pub request: Request,
    pub response: Option<Vec<PostmanResponse>>,
    #[serde(rename = "protocolProfileBehavior")]
    pub protocol_profile_behavior: Option<ProtocolProfileBehavior>,
}
