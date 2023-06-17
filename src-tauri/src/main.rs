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
    let paths = fs::read_dir("./migrations").unwrap();
    let mut collections = vec![];

    for path in paths{
        let actual_path = path.unwrap().path();
        let cloned_path = actual_path.clone();
        let extension = cloned_path.extension();
        if extension.is_none() || extension.unwrap() != "json"{
            continue;
        }
            let file = fs::File::open(actual_path).unwrap();
            let collection: Spec = serde_json::from_reader(file).unwrap();
            collections.push(collection);
    }
    return collections
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
                map.insert(HeaderName::from_str(&*header.key).unwrap(), header.value.parse().unwrap());
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
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet, get_collections, do_request])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
