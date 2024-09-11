use serde::{Deserialize, Serialize};
use crate::{get_database, COLLECTION_PREFIX, ENVIRONMENT_PREFIX};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    id: String,
    r#type: String,
    initial_value: String,
    current_value: String
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentWrapper {
    name: String,
    env_vars: Vec<Environment>
}


#[tauri::command]
pub async fn get_environments(app_tauri: tauri::AppHandle) -> Vec<EnvironmentWrapper> {
    let db = get_database(app_tauri);
    let mut environments = vec![];


    for kv in db.iter() {
        let key = kv.get_key();
        if !key.starts_with(ENVIRONMENT_PREFIX){
            continue
        }
        let str_val = kv.get_value::<String>().unwrap();
        environments.push(serde_json::from_str::<EnvironmentWrapper>(&str_val).unwrap())
    }

    environments
}


#[tauri::command]
pub async fn update_environments(env_wrapper: EnvironmentWrapper, app_tauri: tauri::AppHandle) {
    let mut db = get_database(app_tauri);
    let mut env_string = ENVIRONMENT_PREFIX.to_string();
    env_string.push_str(&env_wrapper.name);

    let value = serde_json::to_string(&env_wrapper).unwrap();
    db.set(&env_string, &value).unwrap();
}