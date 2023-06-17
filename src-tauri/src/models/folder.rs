use crate::models::auth::Auth;
use crate::models::event::Event;
use crate::models::postman_collection::ProtocolProfileBehavior;
use crate::models::postman_item::Item;
use crate::models::variable::Variable;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Folder{
    pub id: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub variable: Option<Vec<Variable>>,
    pub event: Option<Vec<Event>>,
    pub item: Option<Vec<Item>>,
    pub auth: Option<Auth>,
    pub protocolProfileBehavior: Option<ProtocolProfileBehavior>,
}