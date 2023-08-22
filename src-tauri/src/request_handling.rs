use std::fs;
use std::io::{Read};
use std::str::FromStr;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Method;
use crate::{postman_lib, replace_vars_in_url};
use crate::postman_lib::v2_1_0::{Auth, AuthType, HeaderUnion, Host, Items, Mode, RequestUnion, Spec, UrlPath};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use serde_json::Value;
use crate::postman_lib::v2_1_0::PathElement::{PathClass, String as PString};

pub async fn handle_request(url: RequestUnion, collection: &Spec,
                            client: reqwest::ClientBuilder,
                            map: &mut HeaderMap<HeaderValue>,
                            item: Items) -> reqwest::RequestBuilder {
    let mut built_client;

    if let RequestUnion::RequestClass(request) = url {
        if let Some(header) = request.header.clone() {
            if let HeaderUnion::HeaderArray(request) = header {
                for header in request {
                    if header.disabled.is_some() && !header.disabled.unwrap() {
                        map.insert(HeaderName::from_str(&header.key).unwrap(), header.value.parse().unwrap());
                    } else if header.disabled.is_none() {
                        map.insert(HeaderName::from_str(&header.key).unwrap(), header.value.parse().unwrap());
                    }
                }
            }
            match request.body.clone() {
                Some(body)=>{
                    match body.mode{
                        Some(mode) => {
                            match mode {
                                Mode::File => {
                                    let file = body.file.unwrap();
                                }
                                Mode::Formdata => {}
                                Mode::Raw => {}
                                Mode::Urlencoded => {}
                            }
                        }
                        None => {}
                    }
                }
                None => {

                }
            }
        }

        match request.method.clone() {
            Some(method) => {
                let url = request.url.clone().unwrap();

                let method = Method::from_str(&method).unwrap();
                if let postman_lib::v2_1_0::Url::UrlClass(url) = url {
                    let mut url_to_use:String = "".to_string();

                    match url.raw{
                        Some(raw) => {
                            url_to_use = raw;
                        }
                        None => {

                            match url.protocol {
                                Some(protocol) => {
                                    match protocol.as_str() {
                                        "http" => {
                                            url_to_use.push_str("http://");
                                        }
                                        "https" => {
                                            url_to_use.push_str("https://");
                                        }
                                        _ => {
                                            url_to_use.push_str("http://");
                                        }
                                    }
                                }
                                None => {
                                    url_to_use = "http://".to_string();;
                                }
                            }

                            match url.host {
                                Some(host) => {
                                    match host {
                                        Host::String(host) => {
                                            url_to_use = host;
                                        }
                                        Host::StringArray(host) => {
                                            url_to_use = host[0].clone()
                                        }
                                    }
                                }
                                None => {
                                    url_to_use = "".to_string();
                                }
                            }

                            if let Some(port) = url.port {
                                url_to_use.push_str(&format!(":{}", port));
                            }

                            if let Some(path) = url.path {
                                match path {
                                    UrlPath::String(s) => {
                                        url_to_use.push_str(&s);
                                    }
                                    UrlPath::UnionArray(arr) => {
                                        for part in arr {
                                            match part {
                                                PathClass(arr) => {
                                                        url_to_use.push_str(&format!("/{}",&arr.value.unwrap()));
                                                }
                                                PString(path) => {
                                                    url_to_use.push_str(&format!("/{}",&path));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let replaced_url  = replace_vars_in_url(url_to_use, collection.variable.clone());

                    built_client = client
                        .build()
                        .unwrap()
                        .request(method, replaced_url)
                        .body(convert_respective_body(RequestUnion::RequestClass(request.clone())).unwrap());
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
    match value {
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


fn convert_respective_body(collection:RequestUnion) ->Result<String, ()> {
    return match collection {
        RequestUnion::RequestClass(url) => {
            match url.body {
                Some(body) => {
                    match body.mode {
                        Some(mode) => {
                            match mode {
                                Mode::File => {
                                    let file = body.file.unwrap();

                                    return match file.content {
                                        Some(content) => {
                                            Ok(content)
                                        }
                                        None => {
                                             fs::read_to_string(file.src.unwrap()).map_err(|_|())
                                        }
                                    }
                                }
                                Mode::Formdata => {
                                    let form_data = body.formdata.unwrap();
                                    let mut form_data_string:String = "".to_string();
                                   form_data.iter().filter(|x|!x.disabled.unwrap_or(false))
                                       .for_each(|x|{
                                           form_data_string.push_str(&format!("{}={}&",x.key
                                               .clone(),urlencoding::encode(&x.value.clone().unwrap())));
                                   });
                                    Ok(form_data_string)
                                }
                                Mode::Raw => {
                                    Ok((body.raw.unwrap()))
                                }
                                Mode::Urlencoded => {
                                    let form_data = body.urlencoded.unwrap();
                                    let mut form_data_string:String = "".to_string();
                                    form_data.iter().filter(|x|!x.disabled.unwrap_or(false))
                                        .for_each(|x|{
                                            form_data_string.push_str(&format!("{}={}&",x.key,urlencoding::encode(&x.value.clone().unwrap())));
                                        });
                                    Ok(form_data_string)
                                }
                            }
                        }
                        None => {
                            return Err(())
                        }
                    }
                }
                None => {
                    return Err(())
                }
            }
        }
        RequestUnion::String(url) => {
            Err(())
        }
    }
}
