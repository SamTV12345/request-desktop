// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::fs;
use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Instant};
use oauth2::basic::BasicTokenResponse;
use pickledb::{PickleDb, PickleDbDumpPolicy, SerializationMethod};
use reqwest::{ClientBuilder};
use reqwest::header::{HeaderMap,};
use serde_json::Value;
use tauri::Window;
use crate::models::response_from_call::ResponseFromCall;
use uuid::Uuid;
use crate::models::postman_collection::PostmanCollection;
use crate::oauth::{handle_oauth, OAuth2Type};
use crate::oauth2_error::OAuth2Error;
use crate::postman_lib::v2_1_0::{Items, Spec};
use crate::request_handling::handle_request;
use crate::collections::*;
mod postman_lib;


mod models;
mod request_handling;
mod oauth;
mod oauth2_error;
mod collections;

static COLLECTION_PREFIX: &str = "collection_";
static TOKEN_PREFIX: &str = "token_";

#[tauri::command]
async fn check_parser(collection: Value){
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
async fn get_oauth2_token(window: Window, config: OAuth2Type, app_state: tauri::AppHandle) -> Result<BasicTokenResponse, OAuth2Error> {
    handle_oauth(&window, config, app_state).await
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
        .invoke_handler(tauri::generate_handler![get_collections, do_request, insert_collection,
            update_collection, check_parser, get_oauth2_token, get_postman_files_from_dir, update_collection_in_backend, download_from_url])
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
            key_string.push_str(&v.key.clone().unwrap());
            key_string.push_str("}}");
            let val = v.value.clone().unwrap();
            if let Value::String(request) = val {
                url_to_return = url_to_return.replace(&key_string, &request);
            }

        })
    }
    url_to_return
}

#[tauri::command]
async fn get_postman_files_from_dir(path:String) ->Result<Vec<Spec>, ()>{
    let files = recurse_files(path).unwrap();
    let mut collections = vec![];
    for file in files{
        let content = fs::read_to_string(file);
        if let Ok(content_of_var) = content {
                        let collection = serde_json::from_str::<Spec>(&content_of_var);
                       if let Ok(converted_collection) = collection {
                            collections.push(converted_collection);
                    }
        }
    }
    Ok(collections)
}

fn recurse_files(path: impl AsRef<Path>) -> std::io::Result<Vec<PathBuf>> {
    let mut buf = vec![];
    let entries = read_dir(path)?;

    for entry in entries {
        let entry = entry?;
        let meta = entry.metadata()?;

        if meta.is_dir() {
            let mut subdir = recurse_files(entry.path())?;
            buf.append(&mut subdir);
        }

        if meta.is_file() {
            buf.push(entry.path());
        }
    }

    Ok(buf)
}
