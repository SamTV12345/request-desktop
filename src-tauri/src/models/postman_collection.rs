use postman_collection::v2_1_0::Spec;
use serde::{Deserialize, Serialize};
use crate::models::auth::Auth;
use crate::models::folder::Folder;
use crate::models::info::Info;
use crate::models::postman_item::Item;

#[derive(Serialize, Deserialize)]
pub struct PostmanCollection {
    pub info: Info,
    pub item: Vec<FolderOrItem>,
    pub auth: Option<Auth>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum FolderOrItem{
    Folder(Folder),
    Item(Item),
}



#[derive(Serialize, Deserialize)]
pub struct ProtocolProfileBehavior{
    pub disableBodyPruning: Option<bool>,
}


impl PostmanCollection {
    pub async fn save(collection: Spec) -> Result<(), Box<dyn std::error::Error>> {
        let file = std::fs::File::create("./migrations/".to_owned()+&collection.info.postman_id.clone()
            .unwrap())?;
        serde_json::to_writer_pretty(file, &collection)?;
        Ok(())
    }
}