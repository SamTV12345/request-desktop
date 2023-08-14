use std::str::FromStr;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Method;
use crate::{postman_lib, replace_vars_in_url};
use crate::postman_lib::v2_1_0::{Auth, AuthType, HeaderUnion, Items, RequestUnion, Spec};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use serde_json::Value;

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

                    map
                }
                AuthType::Apikey => {
                    map
                }
                AuthType::Basic => {
                    return match auth.basic {
                        Some(basic) => {
                            let username = basic.iter().find(|&x| x.key == "username").unwrap().value.clone().unwrap();
                            let password = basic.iter().find(|&x| x.key == "password").unwrap().value.clone().unwrap();
                            let formatted_string = format!("{}:{}", convert_to_string(username), convert_to_string(password));
                            let encoded = STANDARD_NO_PAD.encode(formatted_string.as_bytes());
                            map.insert(HeaderName::from_str("Authorization").unwrap(),
                                       HeaderValue::from_str(&format!("Basic {}", encoded)).unwrap());
                            map.clone()
                        },
                        None => {
                            map
                        }
                    }
                }
                AuthType::Bearer => {
                    return match auth.bearer{
                        Some(bearer) => {
                            let token = bearer.iter().find(|&x| x.key == "token").unwrap().value.clone().unwrap();
                            map.insert(HeaderName::from_str("Authorization").unwrap(),
                                       HeaderValue::from_str(&format!("Bearer {}", convert_to_string(token))).unwrap());
                            map.clone()
                        },
                        None => {
                            map
                        }
                    }
                }
                AuthType::Digest => {
                    map

                }
                AuthType::Hawk => {
                    map
                }
                AuthType::Noauth => {
                    map
                }
                AuthType::Ntlm => {
                    map

                }
                AuthType::Oauth1 => {
                    map

                }
                AuthType::Oauth2 => {
                    map

                }
    }
}

fn convert_to_string(value: Value) ->String{
    return match value {
        Value::String(s)=>{
            s
        },
        Value::Bool(b)=>{
            b.to_string()
        },
        Value::Number(n)=>{
            n.to_string()
        },
        Value::Null=>{
            "".to_string()
        },
        _ => {
            "".to_string()
        }
    }
}
