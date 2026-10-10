//! `xml-to-json`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Context, Result, anyhow, bail};

pub(crate) fn insert_child(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: String,
    v: serde_json::Value,
) {
    match map.get_mut(&key) {
        Some(serde_json::Value::Array(a)) => a.push(v),
        Some(slot) => {
            let old = slot.take();
            *slot = serde_json::Value::Array(vec![old, v]);
        }
        None => {
            map.insert(key, v);
        }
    }
}

pub(crate) fn xml_to_json(args: &Args) -> Result<String> {
    let t = &TOOLS[14];
    let raw = arg(args, t, "input").trim();
    if raw.is_empty() {
        bail!("no XML input");
    }
    let mut r = xml_reader(raw);
    // (element name, own object, accumulated text)
    let mut stack: Vec<(String, serde_json::Map<String, serde_json::Value>, String)> =
        vec![(String::new(), serde_json::Map::new(), String::new())];
    let mut saw_element = false;

    use quick_xml::events::Event;
    loop {
        match r.read_event().context("invalid XML")? {
            Event::Start(e) => {
                saw_element = true;
                let name: String = e.name().into_inner().into();
                let mut obj = serde_json::Map::new();
                let attrs = attrs_of(&e)?;
                if !attrs.is_empty() {
                    obj.insert(
                        "_attributes".into(),
                        attrs
                            .into_iter()
                            .map(|(k, v)| (k, serde_json::Value::String(v)))
                            .collect(),
                    );
                }
                stack.push((name, obj, String::new()));
            }
            Event::Empty(e) => {
                saw_element = true;
                let name: String = e.name().into_inner().into();
                let mut obj = serde_json::Map::new();
                let attrs = attrs_of(&e)?;
                if !attrs.is_empty() {
                    obj.insert(
                        "_attributes".into(),
                        attrs
                            .into_iter()
                            .map(|(k, v)| (k, serde_json::Value::String(v)))
                            .collect(),
                    );
                }
                let parent = stack.last_mut().unwrap();
                insert_child(&mut parent.1, name, serde_json::Value::Object(obj));
            }
            Event::Text(e) => stack
                .last_mut()
                .unwrap()
                .2
                .push_str(e.into_inner().as_ref()),
            Event::CData(e) => stack
                .last_mut()
                .unwrap()
                .2
                .push_str(e.into_inner().as_ref()),
            Event::GeneralRef(e) => {
                let name: String = e.into_inner().into();
                let resolved = quick_xml::escape::unescape(&format!("&{name};"))
                    .map(|s| s.into_owned())
                    .unwrap_or_else(|_| format!("&{name};"));
                stack.last_mut().unwrap().2.push_str(&resolved);
            }
            Event::End(_) => {
                let (name, mut obj, text) = stack.pop().ok_or_else(|| anyhow!("unbalanced XML"))?;
                if stack.is_empty() {
                    bail!("unbalanced XML: unexpected closing tag");
                }
                let text = text.trim();
                if !text.is_empty() {
                    obj.insert("_text".into(), text.into());
                }
                let parent = stack.last_mut().unwrap();
                insert_child(&mut parent.1, name, serde_json::Value::Object(obj));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if stack.len() != 1 {
        bail!("unbalanced XML: unclosed tag");
    }
    if !saw_element {
        bail!("no XML element found");
    }
    Ok(serde_json::to_string_pretty(&serde_json::Value::Object(
        stack.pop().unwrap().1,
    ))?)
}

pub static TOOL: Tool = Tool {
    id: "xml-to-json",
    name: "XML to JSON",
    description: "Convert XML to JSON (xml-js compact convention).",
    category: Category::Converter,
    keywords: &["xml", "json", "convert"],
    fields: &[Field::area("input", "XML")],
    kind: Kind::Pure,
    run: xml_to_json,
};
