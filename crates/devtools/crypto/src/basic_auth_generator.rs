//! `basic-auth-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

pub(crate) fn run_basic_auth(a: &Args) -> Result<String> {
    let credentials = format!("{}:{}", s(a, "username", ""), s(a, "password", ""));
    Ok(format!(
        "Authorization: Basic {}",
        STANDARD.encode(credentials)
    ))
}

pub static TOOL: Tool = Tool {
    id: "basic-auth-generator",
    name: "Basic auth generator",
    description: "Build a base64 basic authentication header from a user and password.",
    category: Category::Crypto,
    keywords: &["basic", "auth", "authorization", "header", "base64", "http"],
    fields: &[
        Field::text("username", "Username"),
        Field::text("password", "Password"),
    ],
    kind: Kind::Pure,
    run: run_basic_auth,
};
