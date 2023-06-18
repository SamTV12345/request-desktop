// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::fs;
use std::fs::FileType;
use std::iter::Map;
use std::str::FromStr;
use std::time::Instant;
use postman_collection::v2_1_0::{HeaderUnion, Items, RequestUnion, Spec};
use reqwest::{ClientBuilder, Method};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use rusty_leveldb::{DB, LdbIterator};
use crate::models::postman_collection::PostmanCollection;
use crate::models::response_from_call::ResponseFromCall;

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

    let mut db = get_database();
    let mut iterator = db.new_iter().unwrap();
    while iterator.valid() {
        let (mut k, mut v) = (vec![], vec![]);
        iterator.current(&mut k, &mut v);
        iterator.advance();
        let value = std::str::from_utf8(&*v);
        let spec:Spec = serde_json::from_str(value.unwrap()).unwrap();
        collections.push(spec);
    }

    return collections
}

#[tauri::command]
async fn insert_collection(collection: Spec) {
    let mut db = get_database();
    let key = collection.info.postman_id.clone().unwrap();
    let value = serde_json::to_string(&collection).unwrap();
    db.put(key.as_bytes(), value.as_bytes()).unwrap();
}

#[tauri::command]
async fn do_request(item: Items) -> ResponseFromCall {
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
                let method = reqwest::Method::from_str(&method).unwrap();
                if let postman_collection::v2_1_0::Url::UrlClass(url) = url {
                    let url = url.raw.unwrap();
                    built_client = client
                        .build()
                        .unwrap()
                        .request(method, url)
                }
                else if let postman_collection::v2_1_0::Url::String(url) = url{
                    built_client = client
                        .build()
                        .unwrap()
                        .request(method, url)
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
    get_database();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet, get_collections, do_request,insert_collection])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}


pub fn get_database() -> DB {
    let opt = rusty_leveldb::Options::default();
    let mut db = DB::open("../database", opt).unwrap();
    db
}