// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::fs;
use std::fs::FileType;
use std::iter::Map;
use std::path::Path;
use std::str::FromStr;
use std::time::{Duration, Instant};
use pickledb::{PickleDb, PickleDbDumpPolicy, SerializationMethod};
use reqwest::{ClientBuilder, Method};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use crate::models::response_from_call::ResponseFromCall;
use uuid::Uuid;
use crate::models::postman_collection;
use crate::models::postman_collection::PostmanCollection;
use crate::postman_lib::v2_1_0::{HeaderUnion, Items, RequestUnion, Spec};

mod postman_lib;


mod models;
// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
async fn greet(name: Spec) -> String {
    PostmanCollection::save(name).await.expect("TODO: panic message");
    return "OK".to_string()
}

#[tauri::command]
async fn get_collections() -> Vec<Spec> {
    let mut collections = vec![];
    let db = get_database();


    for kv in db.iter() {
        let collection = kv.get_value::<String>();
        if collection.is_some() {
            //collections.push(collection.unwrap()) // Doesnt work
            collections.push(serde_json::from_str::<Spec>(&collection.unwrap()).unwrap());
        }
    }
    return collections
}

#[tauri::command]
async fn insert_collection(mut collection: Spec) {
    let mut db = get_database();
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

    let value = serde_json::to_string(&collection).unwrap();
    db.set(&key.unwrap(), &value).unwrap();
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
async fn update_collection(collection: Spec){
    let mut db = get_database();
    let key = collection.info.postman_id.clone();
    let value = serde_json::to_string(&collection).unwrap();
    db.set(&key.unwrap(), &value).unwrap();
}

#[tauri::command]
async fn insert_collection_from_openapi(collection: String) {
    let mut db = get_database();
    let value = serde_json::to_string(&collection).unwrap();
    db.set(&"test", &value).unwrap();
    println!("Value {}",db.get::<String>(&"test").unwrap());
}

#[tauri::command]
async fn update_collection_in_backend(collection: Spec){
    let mut db = get_database();
    let key = collection.info.name.clone();
    let value = serde_json::to_string(&collection).unwrap();
    db.set(&key, &value).unwrap();
}


#[tauri::command]
async fn do_request(item: Items, collection: Spec) -> ResponseFromCall {
    let mut map = HeaderMap::new();
    let client = ClientBuilder::new();
    let mut built_client;
    let url = item.request.unwrap();
    if let RequestUnion::RequestClass(request) = url {
        if let HeaderUnion::HeaderArray(request) = request.header.unwrap() {
            for header in request {
                if header.disabled.is_some() && !header.disabled.unwrap() {
                    map.insert(HeaderName::from_str(&*header.key).unwrap(), header.value.parse().unwrap());
                }
                else if header.disabled.is_none(){
                    map.insert(HeaderName::from_str(&*header.key).unwrap(), header.value.parse().unwrap());
                }
            }
        }

        match request.method{
            Some(method) => {
                let url = request.url.unwrap();

                let method = Method::from_str(&method).unwrap();
                if let postman_lib::v2_1_0::Url::UrlClass(url) = url {
                    let url = url.raw.unwrap();
                    let replaced_url  = replace_vars_in_url(url, collection.variable);
                    println!("Replaced url {}", replaced_url);

                    built_client = client
                        .build()
                        .unwrap()
                        .request(method, replaced_url)
                }
                else if let postman_lib::v2_1_0::Url::String(url) = url{
                    let replaced_url  = replace_vars_in_url(url, collection.variable);
                    println!("Replaced url {}", replaced_url);
                    built_client = client
                        .build()
                        .unwrap()
                        .request(method, replaced_url)
                }
                else{
                    built_client = client
                        .build()
                        .unwrap()
                        .request(method, "https://google.com")
                }
            }
            None => {
                built_client = client
                    .build()
                    .unwrap()
                    .request(Method::GET, "https://google.com")
            }
        }
    }
    else{
        built_client = client
            .build()
            .unwrap()
            .request(Method::GET, "https://google.com")
    }

    let response_start_time = Instant::now();

    let res = built_client
        .send()
        .await
        .unwrap();
    let response_end_time = Instant::now();
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
        .invoke_handler(tauri::generate_handler![greet, get_collections, do_request, insert_collection, update_collection])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn get_database() -> PickleDb {
    match Path::new("../example.db").exists() {
        true=>{
                PickleDb::load("../example.db", PickleDbDumpPolicy::AutoDump,
                               SerializationMethod::Json).unwrap()
        },
        false=>{
            PickleDb::new("../example.db", PickleDbDumpPolicy::AutoDump,
                          SerializationMethod::Json)
        }
    }
}


pub fn replace_vars_in_url(url:String, variables: Option<Vec<postman_lib::v2_1_0::Variable>>) -> String{
    let mut url_to_return = url.clone();
    println!("Url to replace {:?}", variables);
    if let Some(variables) = variables{
        variables.iter().for_each(|v|{
            let mut key_string = "{{".to_string();
            key_string.push_str(&*v.key.clone().unwrap());
            key_string.push_str("}}");
            let val = v.value.clone().unwrap();
            match  val{
                Value::String(request) => {
                    url_to_return = url_to_return.replace(&key_string, &request);
                },
                _ =>{}
            }
        });
    }
    url_to_return
}
