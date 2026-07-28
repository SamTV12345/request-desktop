//! `json-to-schema`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

// it-tools PR #1166.

pub(crate) fn string_format(v: &str) -> Option<&'static str> {
    if v.len() == 36 && uuid::Uuid::parse_str(v).is_ok() {
        return Some("uuid");
    }
    if chrono::DateTime::parse_from_rfc3339(v).is_ok() {
        return Some("date-time");
    }
    if chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").is_ok() {
        return Some("date");
    }
    if v.starts_with("http://") || v.starts_with("https://") {
        return Some("uri");
    }
    // Deliberately crude: a schema hint, not an address validator.
    let (user, host) = v.split_once('@')?;
    (!user.is_empty() && host.contains('.') && !host.contains(' ')).then_some("email")
}

pub(crate) fn infer_schema(v: &Value) -> Value {
    match v {
        Value::Null => json!({ "type": "null" }),
        Value::Bool(_) => json!({ "type": "boolean" }),
        Value::Number(n) => json!({ "type": if n.is_f64() { "number" } else { "integer" } }),
        Value::String(s) => match string_format(s) {
            Some(f) => json!({ "type": "string", "format": f }),
            None => json!({ "type": "string" }),
        },
        Value::Array(items) => json!({
            "type": "array",
            // ponytail: the first element decides `items`. Merging every
            // element's schema is a union type nobody asked for; a mixed array
            // is rare and still gets a usable starting point.
            "items": items.first().map(infer_schema).unwrap_or_else(|| json!({})),
        }),
        Value::Object(map) => json!({
            "type": "object",
            "properties": Value::Object(
                map.iter().map(|(k, v)| (k.clone(), infer_schema(v))).collect(),
            ),
            "required": map.keys().cloned().collect::<Vec<_>>(),
        }),
    }
}

pub(crate) fn json_to_schema(a: &Args) -> Result<String> {
    let raw = s(a, "input", "").trim();
    if raw.is_empty() {
        bail!("no JSON input");
    }
    let value: Value = serde_json::from_str(raw).context("invalid JSON")?;
    let mut schema = infer_schema(&value);
    if let Some(obj) = schema.as_object_mut() {
        obj.insert(
            "$schema".into(),
            json!("http://json-schema.org/draft-07/schema#"),
        );
    }
    Ok(serde_json::to_string_pretty(&schema)?)
}

pub static TOOL: Tool = Tool {
    id: "json-to-schema",
    name: "JSON to JSON Schema",
    description: "Infer a draft-07 JSON Schema from a sample document.",
    category: Category::Data,
    keywords: &["json", "schema", "draft", "infer", "validate", "types"],
    fields: &[Field::area("input", "Sample JSON")],
    kind: Kind::Pure,
    run: json_to_schema,
};
