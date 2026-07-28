//! `json-to-xml`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Result, anyhow};
use std::fmt::Write as _;

pub(crate) fn json_to_xml(args: &Args) -> Result<String> {
    let v = json(args, TOOLS[4], "input")?;
    let obj = v
        .as_object()
        .ok_or_else(|| anyhow!("XML needs a JSON object at the root"))?;
    let mut out = String::new();
    for (k, child) in obj {
        write_element(k, child, &mut out)?;
    }
    Ok(out)
}

/// xml-js "compact" convention: `_attributes` becomes attributes, `_text` the
/// text node, every other key a child element (arrays repeat the element).
pub(crate) fn write_element(name: &str, value: &serde_json::Value, out: &mut String) -> Result<()> {
    if name == "_attributes" || name == "_text" {
        return Ok(());
    }
    if let serde_json::Value::Array(items) = value {
        for item in items {
            write_element(name, item, out)?;
        }
        return Ok(());
    }
    let esc = |s: &str| quick_xml::escape::escape(s).into_owned();
    let obj = match value {
        serde_json::Value::Object(o) => o,
        scalar => {
            let text = scalar_text(scalar);
            let _ = write!(out, "<{name}>{}</{name}>", esc(&text));
            return Ok(());
        }
    };
    let _ = write!(out, "<{name}");
    if let Some(serde_json::Value::Object(attrs)) = obj.get("_attributes") {
        for (k, v) in attrs {
            let _ = write!(out, " {k}=\"{}\"", esc(&scalar_text(v)));
        }
    }
    let mut body = String::new();
    if let Some(text) = obj.get("_text") {
        body.push_str(&esc(&scalar_text(text)));
    }
    for (k, child) in obj {
        write_element(k, child, &mut body)?;
    }
    if body.is_empty() {
        out.push_str("/>");
    } else {
        let _ = write!(out, ">{body}</{name}>");
    }
    Ok(())
}

pub(crate) fn scalar_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub static TOOL: Tool = Tool {
    id: "json-to-xml",
    name: "JSON to XML",
    description: "Convert JSON to XML (xml-js compact convention).",
    category: Category::Converter,
    keywords: &["json", "xml", "convert"],
    fields: &[Field::area("input", "JSON")],
    kind: Kind::Pure,
    run: json_to_xml,
};
