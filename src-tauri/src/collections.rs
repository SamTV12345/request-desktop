use std::io::{BufReader, Error};
use serde_json::Value;
use uuid::Uuid;
use crate::{COLLECTION_PREFIX, get_database};
use crate::postman_lib::lib::from_reader;
use crate::postman_lib::v2_1_0::{Items, Spec};

#[tauri::command]
pub async fn get_collections(app_handle: tauri::AppHandle) -> Vec<Spec> {
    let mut collections = vec![];
    let db = get_database(app_handle);


    for kv in db.iter() {
        let key = kv.get_key();
        if key.starts_with(COLLECTION_PREFIX) {
            let collection = kv.get_value::<String>();
            if collection.is_some() {
                collections.push(serde_json::from_str::<Spec>(&collection.unwrap()).unwrap());
            }
        }
    }

    collections.sort_by(|a,b|{
        a.info.name.cmp(&b.info.name)
    });

    collections
}

#[tauri::command]
pub async fn insert_collection(mut collection: Spec, app_handle: tauri::AppHandle) ->Result<Spec,()> {
    let mut db = get_database(app_handle);
    let mut key = collection.info.postman_id.clone();

    key = Option::from(Uuid::new_v4().to_string());
    collection.info.postman_id = key.clone();

    collection.item.iter_mut().for_each(|item|{
        let id = Uuid::new_v4().to_string();
        item.id = Some(id);
        if item.item.is_some(){
            item.item = Some(assign_id_to_every_item(item));
        }
    });

    let mut collection_string = COLLECTION_PREFIX.clone().to_string();
    collection_string.push_str(&collection.info.postman_id.clone().unwrap());
    let value = serde_json::to_string(&collection).unwrap();
    db.set(&collection_string, &value).unwrap();
    Ok(collection)
}


fn assign_id_to_every_item(collection: &Items) -> Vec<Items> {
    // Create code that assigns to every item in a postman collection an id

    let mut items = vec![];
    for item in collection.item.clone().unwrap(){
        let mut item = item.clone();
        let id = Uuid::new_v4().to_string();
        item.id = Some(id);
        if item.item.is_some(){
            item.item = Some(assign_id_to_every_item(&item));
        }
        items.push(item);
    }
    items
}


#[tauri::command]
pub async fn insert_collection_from_openapi(collection: String,  app_handle: tauri::AppHandle) {
    let mut db = get_database(app_handle);
    let value = serde_json::to_string(&collection).unwrap();
    db.set("test", &value).unwrap();
    println!("Value {}",db.get::<String>("test").unwrap());
}



#[tauri::command]
pub async fn update_collection(collection: Spec, app_handle: tauri::AppHandle){
    let mut db = get_database(app_handle);
    let mut collection_string = COLLECTION_PREFIX.clone().to_string();
    collection_string.push_str(&collection.info.postman_id.clone().unwrap());
    let value = serde_json::to_string(&collection).unwrap();
    db.set(&collection_string, &value).unwrap();
}


#[tauri::command]
pub async fn update_collection_in_backend(collection: Spec,  app_handle: tauri::AppHandle){
    let mut db = get_database(app_handle);
    let mut collection_string = COLLECTION_PREFIX.clone().to_string();
    collection_string.push_str(&collection.info.postman_id.clone().unwrap());
    let value = serde_json::to_string(&collection).unwrap();
    db.set(&collection_string, &value).unwrap();
}

#[tauri::command]
pub async fn download_from_url(url: String) -> Result<Value, String> {
    let response = reqwest::get(url).await.map_err(|e|{
        e.to_string()
    })?.text().await.map_err(|e|{
        e.to_string()
    })?;

    Ok(serde_json::from_str(&response).unwrap())
}

#[tauri::command]
pub async fn delete_collection_by_id(id: String, app_handle: tauri::AppHandle) -> Result<(), ()> {
    let mut db = get_database(app_handle);
    let mut collection_string = COLLECTION_PREFIX.clone().to_string();
    collection_string.push_str(&id);
    db.rem(&collection_string).unwrap();
    Ok(())
}
