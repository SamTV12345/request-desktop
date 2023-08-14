use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct OAuth2Response {
    access_token: String,
    token_type: String,
    expires_in: u32,
    scope: String,
    token_name:String
}
