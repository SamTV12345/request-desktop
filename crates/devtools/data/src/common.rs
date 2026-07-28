//! Helpers shared by the tools in this crate.

use crate::{Args, Tool, arg};
use anyhow::{Context, Result, bail};

pub(crate) fn json(args: &Args, tool: &Tool, key: &str) -> Result<serde_json::Value> {
    let raw = arg(args, tool, key).trim();
    if raw.is_empty() {
        bail!("no JSON input");
    }
    serde_json::from_str(raw).context("invalid JSON")
}

/// Parse YAML, naming the place it went wrong.
///
/// it-tools PR #957: "invalid YAML" on its own leaves you scanning a hundred
/// lines by eye. The parser already knows the position, so quote it. Some
/// errors (`JSON number out of range`) carry a location but do not print one,
/// which is why it is spelled out here rather than left to `Display`.
pub(crate) fn yaml_str(raw: &str) -> Result<serde_json::Value> {
    serde_norway::from_str(raw).map_err(|e| match e.location() {
        Some(l) if !e.to_string().contains("line") => {
            anyhow::anyhow!(
                "invalid YAML at line {}, column {}: {e}",
                l.line(),
                l.column()
            )
        }
        _ => anyhow::anyhow!("invalid YAML: {e}"),
    })
}

pub(crate) fn yaml(args: &Args, tool: &Tool, key: &str) -> Result<serde_json::Value> {
    let raw = arg(args, tool, key).trim();
    if raw.is_empty() {
        bail!("no YAML input");
    }
    yaml_str(raw)
}

pub(crate) fn toml_value(args: &Args, tool: &Tool, key: &str) -> Result<serde_json::Value> {
    let raw = arg(args, tool, key).trim();
    if raw.is_empty() {
        bail!("no TOML input");
    }
    toml::from_str(raw).context("invalid TOML")
}

pub(crate) fn to_yaml(v: &serde_json::Value) -> Result<String> {
    serde_norway::to_string(v).context("cannot represent this as YAML")
}

pub(crate) fn to_toml(v: &serde_json::Value) -> Result<String> {
    // TOML has no null and no scalar document root.
    toml::to_string_pretty(v).context("cannot represent this as TOML")
}

/// it-tools' case-converter renders all of these at once, in this order, with
/// these labels. `Paramcase` is change-case's name for kebab-case.
pub(crate) const CASES: &[(&str, &str)] = &[
    ("Lowercase", "lower"),
    ("Uppercase", "upper"),
    ("Camelcase", "camel"),
    ("Capitalcase", "capital"),
    ("Constantcase", "constant"),
    ("Dotcase", "dot"),
    ("Headercase", "header"),
    ("Nocase", "no"),
    ("Paramcase", "kebab"),
    ("Pascalcase", "pascal"),
    ("Pathcase", "path"),
    ("Sentencecase", "sentence"),
    ("Snakecase", "snake"),
    ("Mockingcase", "mock"),
];

pub(crate) fn xml_reader(s: &str) -> quick_xml::Reader<&[u8]> {
    let mut r = quick_xml::Reader::from_str(s);
    let c = r.config_mut();
    c.trim_text_start = true;
    c.trim_text_end = true;
    r
}

pub(crate) fn attrs_of(e: &quick_xml::events::BytesStart<'_>) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for a in e.attributes() {
        let a = a.context("malformed XML attribute")?;
        let raw = String::from_utf8_lossy(a.value.as_ref()).into_owned();
        let value = quick_xml::escape::unescape(&raw)
            .map(|c| c.into_owned())
            .unwrap_or(raw);
        out.push((String::from_utf8_lossy(a.key.as_ref()).into_owned(), value));
    }
    Ok(out)
}

/// Rebuild every object with its keys in sorted order.
///
/// Goes through a `BTreeMap` rather than calling `Map::sort_keys`, so it
/// compiles whether or not `serde_json/preserve_order` is on.
pub(crate) fn sorted(v: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(o) => {
            let ordered: std::collections::BTreeMap<&String, &Value> = o.iter().collect();
            Value::Object(
                ordered
                    .into_iter()
                    .map(|(k, x)| (k.clone(), sorted(x)))
                    .collect(),
            )
        }
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        scalar => scalar.clone(),
    }
}
