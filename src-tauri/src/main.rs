// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::time::{Instant};
use oauth2::basic::BasicTokenResponse;
use pickledb::{PickleDb, PickleDbDumpPolicy, SerializationMethod};
use reqwest::{ClientBuilder, Method};
use reqwest::header::{HeaderMap, HeaderName,};
use serde_json::Value;
use tauri::Window;
use crate::models::response_from_call::ResponseFromCall;
use uuid::Uuid;
use crate::models::postman_collection::PostmanCollection;
use crate::oauth::{handle_oauth, OAuth2Type};
use crate::oauth2_error::OAuth2Error;
use crate::postman_lib::v2_1_0::{HeaderUnion, Items, RequestUnion, Spec};
use crate::request_handling::handle_request;

mod postman_lib;


mod models;
mod request_handling;
mod oauth;
mod oauth2_error;

static COLLECTION_PREFIX: &str = "collection_";
static TOKEN_PREFIX: &str = "token_";

#[tauri::command]
async fn check_parser(collection: serde_json::Value){
    let serialized_val = serde_json::to_string(&collection).unwrap();
    // Some Deserializer.
    let jd = &mut serde_json::Deserializer::from_str(&serialized_val);

    let result: Result<Spec, _> = serde_path_to_error::deserialize(jd);
    match result {
        Ok(_) => println!("Everything worked as expected"),
        Err(err) => {
            let path = err.path().to_string();
            assert_eq!(path, "dependencies.serde.version");
        }
    }
}

#[tauri::command]
async fn get_oauth2_token(window: Window, config: OAuth2Type) -> Result<BasicTokenResponse, OAuth2Error> {
    handle_oauth(&window, config).await
}


// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
async fn greet(name: Spec) -> String {
    PostmanCollection::save(name).await.expect("TODO: panic message");
    return "OK".to_string()
}

#[tauri::command]
async fn get_collections(app_handle: tauri::AppHandle) -> Vec<Spec> {
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
    return collections
}

#[tauri::command]
async fn insert_collection(mut collection: Spec, app_handle: tauri::AppHandle) ->Result<Spec,()> {
    let mut db = get_database(app_handle);
    let mut key = collection.info.postman_id.clone();
    if key.is_none(){
        key = Option::from(Uuid::new_v4().to_string());
        collection.info.postman_id = key.clone();
    }

    collection.item.iter_mut().for_each(|item|{
        let id = Uuid::new_v4().to_string();
        item.id = Some(id);
        if item.item.is_some(){
            item.item = Some(assign_id_to_every_item(&item));
        }
    });

    let mut collection_string = COLLECTION_PREFIX.clone().to_string();
    collection_string.push_str(&collection.info.postman_id.clone().unwrap());
    let value = serde_json::to_string(&collection).unwrap();
    db.set(&collection_string, &value).unwrap();
    Ok(collection)
}


fn assign_id_to_every_item(mut collection: &Items) -> Vec<Items> {
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
async fn update_collection(collection: Spec, app_handle: tauri::AppHandle){
    let mut db = get_database(app_handle);
    let mut collection_string = COLLECTION_PREFIX.clone().to_string();
    collection_string.push_str(&collection.info.postman_id.clone().unwrap());
    let value = serde_json::to_string(&collection).unwrap();
    db.set(&collection_string, &value).unwrap();
}

#[tauri::command]
async fn insert_collection_from_openapi(collection: String,  app_handle: tauri::AppHandle) {
    let mut db = get_database(app_handle);
    let value = serde_json::to_string(&collection).unwrap();
    db.set(&"test", &value).unwrap();
    println!("Value {}",db.get::<String>(&"test").unwrap());
}

#[tauri::command]
async fn update_collection_in_backend(collection: Spec,  app_handle: tauri::AppHandle){
    let mut db = get_database(app_handle);
    let mut collection_string = COLLECTION_PREFIX.clone().to_string();
    collection_string.push_str(&collection.info.postman_id.clone().unwrap());
    let value = serde_json::to_string(&collection).unwrap();
    db.set(&collection_string, &value).unwrap();
}


#[tauri::command]
async fn do_request(item: Items, collection: Spec) -> ResponseFromCall {
    let mut map = HeaderMap::new();
    let client = ClientBuilder::new();
    let url = item.request.clone().unwrap();

    let built_client = handle_request(url, &collection, client, &mut map, item.clone()).await;

    let response_start_time = Instant::now();

    let res_wrapped = built_client
        .send()
        .await;
    let response_end_time = Instant::now();
    let res = res_wrapped.unwrap();
    let response_duration = response_end_time.duration_since(response_start_time);

    let mut map = HashMap::new();
    let mut cookie_map = HashMap::new();

    res.headers().clone().iter().for_each(|(key, value)| {
        map.insert(key.to_string(), value.to_str().unwrap().to_string());
    });


    let cookies = res.cookies();
    for cookie in cookies{
        cookie_map.insert(cookie.name().to_string(), cookie.value().to_string());
    }

    let start_time = Instant::now();
    let status = res.status().as_u16().to_string();
    let text = res.text().await.unwrap();
    let end_time = Instant::now();
    let duration = end_time.duration_since(start_time);
    let time_measures = models::response_from_call::TimeMeasures{
        response_duration: response_duration.as_millis().to_string(),
        duration: duration.as_millis().to_string(),
    };
    ResponseFromCall{
        status ,
        body: text,
        headers: map,
        cookies: cookie_map,
        duration: time_measures,
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet, get_collections, do_request, insert_collection, update_collection, check_parser, get_oauth2_token])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn get_database(app_handle: tauri::AppHandle) -> PickleDb {
    let path_buffer = app_handle.path_resolver().app_data_dir().unwrap_or("..".parse().unwrap()).as_path().join("request.db");
    let path = path_buffer.as_path();
    match Path::new(path).exists() {
        true=>{
                PickleDb::load(path, PickleDbDumpPolicy::AutoDump,
                               SerializationMethod::Json).unwrap()
        },
        false=>{
            PickleDb::new(path, PickleDbDumpPolicy::AutoDump,
                          SerializationMethod::Json)
        }
    }
}


pub fn replace_vars_in_url(url:String, variables: Option<Vec<postman_lib::v2_1_0::Variable>>) -> String{
    let mut url_to_return = url.clone();
    if let Some(variables) = variables{
        variables.iter().for_each(|v|{
            let mut key_string = "{{".to_string();
            key_string.push_str(&*v.key.clone().unwrap());
            key_string.push_str("}}");
            let val = v.value.clone().unwrap();
            match  val {
                Value::String(request) => {
                    url_to_return = url_to_return.replace(&key_string, &request);
                },
                _ =>{}
            }
        });
    }
    url_to_return
}
