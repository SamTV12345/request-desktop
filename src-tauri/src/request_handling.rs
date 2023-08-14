use std::iter::Map;
use std::str::FromStr;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Method;
use serde_json::Value;
use crate::{postman_lib, replace_vars_in_url};
use crate::postman_lib::v2_1_0::{Auth, AuthType, HeaderUnion, Items, RequestUnion, Spec};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
pub async fn handle_request(url: RequestUnion, collection: &Spec,
                            client: reqwest::ClientBuilder,
                            map: &mut HeaderMap<HeaderValue>,
                            item: Items) -> reqwest::RequestBuilder {
    let mut built_client;

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

        match request.method {
            Some(method) => {
                let url = request.url.unwrap();

                let method = Method::from_str(&method).unwrap();
                if let postman_lib::v2_1_0::Url::UrlClass(url) = url {
                    let url = url.raw.unwrap();
                    let replaced_url  = replace_vars_in_url(url, collection.variable.clone());

                    built_client = client
                        .build()
                        .unwrap()
                        .request(method, replaced_url)
                }
                else if let postman_lib::v2_1_0::Url::String(url) = url{
                    let replaced_url  = replace_vars_in_url(url, collection.variable.clone());
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

    let map = add_auth_headers(collection, item, map);


    built_client = built_client.headers(map.clone());

    built_client
}

pub fn add_auth_headers(collection: &Spec, item: Items,map: &mut HeaderMap<HeaderValue>) -> HeaderMap<HeaderValue>{
    match item.auth {
        Some(auth) => {
            insert_auth(auth, map.clone())
        }
        None => {
            match collection.auth.clone() {
                Some(auth) => {
                    insert_auth(auth, map.clone())
                }
                None => {
                    map.clone()
                }
            }
        }
    }
}


fn insert_auth(auth: Auth, mut map: HeaderMap<HeaderValue>) -> HeaderMap<HeaderValue>{
    return match auth.auth_type {
                AuthType::Awsv4 => {

                    HeaderMap::new()
                }
                AuthType::Apikey => {
                    HeaderMap::new()
                }
                AuthType::Basic => {
                    return match auth.basic {
                        Some(basic) => {
                            let username = basic.iter().find(|&x| x.key == "username").unwrap().value.clone().unwrap();
                            let password = basic.iter().find(|&x| x.key == "password").unwrap().value.clone().unwrap();
                            let formatted_string = format!("{}:{}", username, password);
                            let encoded = STANDARD_NO_PAD.encode(formatted_string.as_bytes());
                            map.insert(HeaderName::from_str("Authorization").unwrap(), HeaderValue::from_str(&format!("Basic {}", encoded)).unwrap());
                            map.clone()
                        },
                        None => {
                            HeaderMap::new()
                        }
                    }
                }
                AuthType::Bearer => {

                    return HeaderMap::new()

                }
                AuthType::Digest => {
                    return HeaderMap::new()

                }
                AuthType::Hawk => {
                    return HeaderMap::new()
                }
                AuthType::Noauth => {
                    return HeaderMap::new()
                }
                AuthType::Ntlm => {
                    return HeaderMap::new()

                }
                AuthType::Oauth1 => {
                    return HeaderMap::new()

                }
                AuthType::Oauth2 => {
                    return HeaderMap::new()

                }
            }
}
