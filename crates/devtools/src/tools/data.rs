//! Data-format converters, ported from it-tools `src/tools/*`.
//!
//! ponytail: every entry is `fn(&Args) -> Result<String>` plus one `static`.
//! No traits, no macros.
//!
//! `serde_json/preserve_order` is on workspace-wide, so object keys keep their
//! input order and the `sortKeys` switches (json-viewer, yaml-viewer) go through
//! [`sorted`] when the user asks for them.
//!
//! Where it-tools renders several panels at once — base64, text-to-binary,
//! text-to-unicode, case-converter, integer-base, roman numerals — so do we, one
//! labelled block each via `crate::section`, and no `mode` select anywhere.
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use base64::engine::general_purpose as b64;
use num_bigint::BigUint;
use std::fmt::Write as _;

// ─── helpers ────────────────────────────────────────────────────────────────

fn flag(args: &Args, tool: &Tool, key: &str) -> bool {
    matches!(arg(args, tool, key).trim(), "true" | "1" | "yes" | "on")
}

fn num(args: &Args, tool: &Tool, key: &str) -> Result<i64> {
    let raw = arg(args, tool, key).trim();
    raw.parse::<i64>()
        .with_context(|| format!("`{key}` must be an integer, got {raw:?}"))
}

fn json(args: &Args, tool: &Tool, key: &str) -> Result<serde_json::Value> {
    let raw = arg(args, tool, key).trim();
    if raw.is_empty() {
        bail!("no JSON input");
    }
    serde_json::from_str(raw).context("invalid JSON")
}

fn yaml(args: &Args, tool: &Tool, key: &str) -> Result<serde_json::Value> {
    let raw = arg(args, tool, key).trim();
    if raw.is_empty() {
        bail!("no YAML input");
    }
    serde_norway::from_str(raw).context("invalid YAML")
}

fn toml_value(args: &Args, tool: &Tool, key: &str) -> Result<serde_json::Value> {
    let raw = arg(args, tool, key).trim();
    if raw.is_empty() {
        bail!("no TOML input");
    }
    toml::from_str(raw).context("invalid TOML")
}

fn to_yaml(v: &serde_json::Value) -> Result<String> {
    serde_norway::to_string(v).context("cannot represent this as YAML")
}

fn to_toml(v: &serde_json::Value) -> Result<String> {
    // TOML has no null and no scalar document root.
    toml::to_string_pretty(v).context("cannot represent this as TOML")
}

// ─── base64-string-converter ────────────────────────────────────────────────

/// Decode one base64 string the way it-tools' `base64ToText` does.
///
/// Two deliberate points of behaviour, both copied from js-base64:
/// * the `data:<mime>;base64,` prefix is stripped first;
/// * the result is *lossy* UTF-8. `TESTtest` is perfectly good base64 over six
///   binary bytes, and it-tools shows those bytes rather than refusing — a hard
///   `from_utf8` error here was the original bug report.
///
/// `urlSafe` is honoured rather than guessed at: on, `-_` are translated and
/// every other stray character dropped (js-base64's `unURI`); off, only
/// whitespace is removed, so a `-` really is a decode error.
fn b64_decode(raw: &str, url_safe: bool) -> Result<String> {
    let s = raw.trim();
    let s = s
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(";base64,"))
        .map_or(s, |(_, body)| body);
    let cleaned: String = if url_safe {
        s.chars()
            .map(|c| match c {
                '-' => '+',
                '_' => '/',
                c => c,
            })
            .filter(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '/')
            .collect()
    } else {
        s.split_whitespace().collect()
    };
    if cleaned.is_empty() {
        bail!("no base64 input");
    }
    let bytes = b64::STANDARD_NO_PAD_INDIFFERENT
        .decode(&cleaned)
        .context("Invalid base64 string")?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// it-tools renders "String to base64" and "Base64 to string" side by side, each
/// with its own URL-safe switch — there is no mode to be in the wrong one of.
fn base64_string(args: &Args) -> Result<String> {
    let t = &TOOLS[0];
    let text = arg(args, t, "input");
    let b64_in = arg(args, t, "base64");
    let mut out = Vec::new();
    if !text.is_empty() {
        let encoded = if flag(args, t, "encodeUrlSafe") {
            b64::URL_SAFE_NO_PAD.encode(text)
        } else {
            b64::STANDARD.encode(text)
        };
        out.push(crate::section("Base64 of string", Ok(encoded)));
    }
    if !b64_in.trim().is_empty() {
        out.push(crate::section(
            "Decoded string",
            b64_decode(b64_in, flag(args, t, "decodeUrlSafe")),
        ));
    }
    if out.is_empty() {
        bail!("no input");
    }
    Ok(out.join("\n\n"))
}

fn base64_file(_args: &Args) -> Result<String> {
    bail!(
        "needs the file panel: pick or drop a file to get its data: URI, or paste          a data: URI to save it back out — with an image preview either way"
    )
}

// ─── json-to-csv ────────────────────────────────────────────────────────────

/// it-tools' own (non-RFC) escaping: backslash-escape newlines and quotes, then
/// wrap in quotes only when a comma is present. Hand-rolled rather than using
/// the `csv` crate because the ported test vectors depend on exactly this.
fn csv_cell(v: Option<&serde_json::Value>) -> String {
    let v = match v {
        None => return String::new(),
        Some(serde_json::Value::Null) => return "null".into(),
        Some(v) => v,
    };
    let s = match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let s = s
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('"', "\\\"");
    if s.contains(',') {
        format!("\"{s}\"")
    } else {
        s
    }
}

fn json_to_csv(args: &Args) -> Result<String> {
    let t = &TOOLS[2];
    let arr = match json(args, t, "input")? {
        serde_json::Value::Array(a) => a,
        _ => bail!("expected a JSON array of objects"),
    };
    let mut headers: Vec<String> = Vec::new();
    for item in &arr {
        let obj = item
            .as_object()
            .ok_or_else(|| anyhow!("every array item must be an object"))?;
        for k in obj.keys() {
            if !headers.iter().any(|h| h == k) {
                headers.push(k.clone());
            }
        }
    }
    let mut out = headers.join(",");
    for item in &arr {
        let obj = item.as_object().unwrap();
        out.push('\n');
        let row: Vec<String> = headers.iter().map(|h| csv_cell(obj.get(h))).collect();
        out.push_str(&row.join(","));
    }
    Ok(out)
}

// ─── json ⇄ toml / xml / yaml, minify, viewer, diff ─────────────────────────

fn json_to_toml(args: &Args) -> Result<String> {
    to_toml(&json(args, &TOOLS[3], "input")?)
}

fn json_to_xml(args: &Args) -> Result<String> {
    let v = json(args, &TOOLS[4], "input")?;
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
fn write_element(name: &str, value: &serde_json::Value, out: &mut String) -> Result<()> {
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

fn scalar_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn json_to_yaml(args: &Args) -> Result<String> {
    to_yaml(&json(args, &TOOLS[5], "input")?)
}

fn json_minify(args: &Args) -> Result<String> {
    Ok(json(args, &TOOLS[6], "input")?.to_string())
}

fn json_diff(args: &Args) -> Result<String> {
    let t = &TOOLS[7];
    let left = serde_json::to_string_pretty(&json(args, t, "left")?)?;
    let right = serde_json::to_string_pretty(&json(args, t, "right")?)?;
    if left == right {
        return Ok("the two documents are equal".into());
    }
    Ok(similar::TextDiff::from_lines(&left, &right)
        .unified_diff()
        .header("left", "right")
        .to_string())
}

/// Rebuild every object with its keys in sorted order.
///
/// Goes through a `BTreeMap` rather than calling `Map::sort_keys`, so it
/// compiles whether or not `serde_json/preserve_order` is on.
fn sorted(v: &serde_json::Value) -> serde_json::Value {
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

fn json_viewer(args: &Args) -> Result<String> {
    let t = &TOOLS[8];
    let mut v = json(args, t, "input")?;
    if flag(args, t, "sortKeys") {
        v = sorted(&v);
    }
    let indent = num(args, t, "indent")?.clamp(0, 16) as usize;
    if indent == 0 {
        return Ok(v.to_string());
    }
    let pad = vec![b' '; indent];
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(
        &mut buf,
        serde_json::ser::PrettyFormatter::with_indent(&pad),
    );
    serde::Serialize::serialize(&v, &mut ser)?;
    Ok(String::from_utf8(buf)?)
}

// ─── toml / yaml ────────────────────────────────────────────────────────────

fn toml_to_json(args: &Args) -> Result<String> {
    Ok(serde_json::to_string_pretty(&toml_value(
        args, &TOOLS[9], "input",
    )?)?)
}

fn toml_to_yaml(args: &Args) -> Result<String> {
    to_yaml(&toml_value(args, &TOOLS[10], "input")?)
}

fn yaml_to_json(args: &Args) -> Result<String> {
    Ok(serde_json::to_string_pretty(&yaml(
        args, &TOOLS[11], "input",
    )?)?)
}

fn yaml_to_toml(args: &Args) -> Result<String> {
    to_toml(&yaml(args, &TOOLS[12], "input")?)
}

fn yaml_viewer(args: &Args) -> Result<String> {
    let t = &TOOLS[13];
    let v = yaml(args, t, "input")?;
    to_yaml(&if flag(args, t, "sortKeys") {
        sorted(&v)
    } else {
        v
    })
}

// ─── xml ────────────────────────────────────────────────────────────────────

fn xml_reader(s: &str) -> quick_xml::Reader<&[u8]> {
    let mut r = quick_xml::Reader::from_str(s);
    let c = r.config_mut();
    c.trim_text_start = true;
    c.trim_text_end = true;
    r
}

fn attrs_of(e: &quick_xml::events::BytesStart<'_>) -> Result<Vec<(String, String)>> {
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

fn insert_child(
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

fn xml_to_json(args: &Args) -> Result<String> {
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
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
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
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
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
                .push_str(&e.decode().unwrap_or_default()),
            Event::CData(e) => stack
                .last_mut()
                .unwrap()
                .2
                .push_str(&String::from_utf8_lossy(e.as_ref())),
            Event::GeneralRef(e) => {
                let name = e.decode().map(|s| s.into_owned()).unwrap_or_default();
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

fn xml_formatter(args: &Args) -> Result<String> {
    let t = &TOOLS[15];
    let raw = arg(args, t, "input").trim();
    if raw.is_empty() {
        bail!("no XML input");
    }
    let indent = num(args, t, "indent")?.clamp(0, 16) as usize;
    let collapse = flag(args, t, "collapseContent");
    let mut r = xml_reader(raw);
    let mut out: Vec<String> = Vec::new();
    // For each open element: where its opening tag landed in `out`, and whether
    // it has so far seen nothing but text. ponytail: xml-formatter's
    // `collapseContent` also collapses mixed text+element content; text-only is
    // the case anyone actually hits, and getting it wrong is invisible.
    let mut open: Vec<(usize, bool)> = Vec::new();
    let mut depth = 0usize;
    let mut saw_element = false;
    let push = |depth: usize, s: String, out: &mut Vec<String>| {
        out.push(format!("{}{}", " ".repeat(depth * indent), s));
    };
    let tag = |e: &quick_xml::events::BytesStart<'_>| -> Result<String> {
        let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
        let mut s = name;
        for (k, v) in attrs_of(e)? {
            let _ = write!(s, " {k}=\"{}\"", quick_xml::escape::escape(&v));
        }
        Ok(s)
    };

    use quick_xml::events::Event;
    loop {
        match r.read_event().context("invalid XML")? {
            Event::Start(e) => {
                saw_element = true;
                if let Some(p) = open.last_mut() {
                    p.1 = false;
                }
                open.push((out.len(), true));
                push(depth, format!("<{}>", tag(&e)?), &mut out);
                depth += 1;
            }
            Event::Empty(e) => {
                saw_element = true;
                if let Some(p) = open.last_mut() {
                    p.1 = false;
                }
                push(depth, format!("<{}/>", tag(&e)?), &mut out);
            }
            Event::End(e) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| anyhow!("unbalanced XML"))?;
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let (start_idx, text_only) = open.pop().ok_or_else(|| anyhow!("unbalanced XML"))?;
                if collapse && text_only && out.len() <= start_idx + 2 {
                    let text = if out.len() == start_idx + 2 {
                        out.pop().unwrap_or_default()
                    } else {
                        String::new()
                    };
                    out[start_idx].push_str(text.trim_start());
                    let _ = write!(out[start_idx], "</{name}>");
                } else {
                    push(depth, format!("</{name}>"), &mut out);
                }
            }
            Event::Text(e) => {
                let s = e.decode().unwrap_or_default();
                if !s.trim().is_empty() {
                    push(
                        depth,
                        quick_xml::escape::escape(s.trim()).into_owned(),
                        &mut out,
                    );
                }
            }
            Event::CData(e) => push(
                depth,
                format!("<![CDATA[{}]]>", String::from_utf8_lossy(e.as_ref())),
                &mut out,
            ),
            Event::Comment(e) => push(
                depth,
                format!("<!--{}-->", e.decode().unwrap_or_default()),
                &mut out,
            ),
            Event::Decl(e) => {
                let inner = String::from_utf8_lossy(e.as_ref()).into_owned();
                push(depth, format!("<?{}?>", inner.trim()), &mut out);
            }
            Event::PI(e) => {
                let inner = String::from_utf8_lossy(&e.into_inner()).into_owned();
                push(depth, format!("<?{inner}?>"), &mut out);
            }
            Event::DocType(e) => push(
                depth,
                format!("<!DOCTYPE {}>", e.decode().unwrap_or_default()),
                &mut out,
            ),
            Event::GeneralRef(e) => push(
                depth,
                format!("&{};", e.decode().unwrap_or_default()),
                &mut out,
            ),
            Event::Eof => break,
        }
    }
    if depth != 0 {
        bail!("unbalanced XML: unclosed tag");
    }
    if !saw_element {
        bail!("no XML element found");
    }
    Ok(out.join("\n"))
}

// ─── list-converter ─────────────────────────────────────────────────────────

fn list_converter(args: &Args) -> Result<String> {
    let t = &TOOLS[16];
    let raw = arg(args, t, "input");
    let keep_breaks = flag(args, t, "keepLineBreaks");
    let line_break = if keep_breaks { "\n" } else { "" };

    let lowered = if flag(args, t, "lowerCase") {
        raw.to_lowercase()
    } else {
        raw.to_string()
    };
    let mut parts: Vec<String> = lowered.split('\n').map(str::to_string).collect();

    if flag(args, t, "removeDuplicates") {
        let mut seen = std::collections::HashSet::new();
        parts.retain(|p| seen.insert(p.clone()));
    }
    if flag(args, t, "reverseList") {
        parts.reverse();
    }
    match arg(args, t, "sortList") {
        "asc" => parts.sort(),
        "desc" => {
            parts.sort();
            parts.reverse();
        }
        _ => {}
    }
    if flag(args, t, "trimItems") {
        parts = parts.iter().map(|p| p.trim().to_string()).collect();
    }
    parts.retain(|p| !p.is_empty());

    let prefix = arg(args, t, "itemPrefix");
    let suffix = arg(args, t, "itemSuffix");
    let body = parts
        .iter()
        .map(|p| format!("{prefix}{p}{suffix}"))
        .collect::<Vec<_>>()
        .join(&format!("{}{line_break}", arg(args, t, "separator")));

    Ok([
        arg(args, t, "listPrefix"),
        &body,
        arg(args, t, "listSuffix"),
    ]
    .join(line_break))
}

// ─── markdown-to-html / sql-prettify ────────────────────────────────────────

fn markdown_to_html(args: &Args) -> Result<String> {
    let input = arg(args, &TOOLS[17], "input");
    if input.trim().is_empty() {
        bail!("no markdown input");
    }
    let mut opts = pulldown_cmark::Options::empty();
    opts.insert(pulldown_cmark::Options::ENABLE_TABLES);
    opts.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    opts.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
    opts.insert(pulldown_cmark::Options::ENABLE_FOOTNOTES);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, pulldown_cmark::Parser::new_ext(input, opts));
    Ok(html)
}

fn sql_prettify(args: &Args) -> Result<String> {
    let t = &TOOLS[18];
    let input = arg(args, t, "input");
    if input.trim().is_empty() {
        bail!("no SQL input");
    }
    let opts = sqlformat::FormatOptions {
        indent: sqlformat::Indent::Spaces(num(args, t, "indent")?.clamp(1, 16) as u8),
        uppercase: match arg(args, t, "keywordCase") {
            "lower" => Some(false),
            "preserve" => None,
            _ => Some(true),
        },
        ..Default::default()
    };
    Ok(sqlformat::format(
        input,
        &sqlformat::QueryParams::None,
        &opts,
    ))
}

// ─── case-converter ─────────────────────────────────────────────────────────

/// Split on any non-alphanumeric run plus camelCase / acronym boundaries.
/// ponytail: `change-case`'s `stripRegexp` also treats digits as separators;
/// keeping them inside words is the less surprising behaviour here.
fn words(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        let prev = if cur.is_empty() {
            None
        } else {
            chars.get(i - 1).copied()
        };
        let next = chars.get(i + 1).copied();
        let boundary = match prev {
            Some(p) if c.is_uppercase() && !p.is_uppercase() => true,
            // HTTPServer -> HTTP | Server
            Some(p) if p.is_uppercase() && c.is_uppercase() => {
                next.is_some_and(|n| n.is_lowercase())
            }
            _ => false,
        };
        if boundary && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn capitalize(w: &str) -> String {
    let mut cs = w.chars();
    match cs.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + &cs.as_str().to_lowercase(),
    }
}

/// Unknown keys are impossible: the only caller iterates [`CASES`], because
/// it-tools shows every case at once instead of offering a picker.
fn convert_case(input: &str, case: &str) -> String {
    let ws = words(input);
    let lower: Vec<String> = ws.iter().map(|w| w.to_lowercase()).collect();
    match case {
        "lower" => input.to_lowercase(),
        "upper" => input.to_uppercase(),
        "mock" => input
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if i % 2 == 0 {
                    c.to_uppercase().to_string()
                } else {
                    c.to_lowercase().to_string()
                }
            })
            .collect(),
        "camel" => lower
            .iter()
            .enumerate()
            .map(|(i, w)| if i == 0 { w.clone() } else { capitalize(w) })
            .collect(),
        "pascal" => lower.iter().map(|w| capitalize(w)).collect(),
        "snake" => lower.join("_"),
        "constant" => lower.join("_").to_uppercase(),
        "kebab" => lower.join("-"),
        // change-case's `noCase`: words, lowercased, space separated.
        "no" => lower.join(" "),
        "header" => lower
            .iter()
            .map(|w| capitalize(w))
            .collect::<Vec<_>>()
            .join("-"),
        "dot" => lower.join("."),
        "path" => lower.join("/"),
        "capital" => lower
            .iter()
            .map(|w| capitalize(w))
            .collect::<Vec<_>>()
            .join(" "),
        "sentence" => lower
            .iter()
            .enumerate()
            .map(|(i, w)| if i == 0 { capitalize(w) } else { w.clone() })
            .collect::<Vec<_>>()
            .join(" "),
        other => unreachable!("unknown case {other:?}"),
    }
}

fn case_converter(args: &Args) -> Result<String> {
    let t = &TOOLS[19];
    let input = arg(args, t, "input");
    if input.is_empty() {
        bail!("no input");
    }
    // One line per case, like `hash-text`: nothing here can fail, so no
    // `section` wrapping is needed.
    Ok(CASES
        .iter()
        .map(|(label, case)| format!("{label}: {}", convert_case(input, case)))
        .collect::<Vec<_>>()
        .join("\n"))
}

// ─── text-to-binary / text-to-unicode ───────────────────────────────────────

fn binary_to_text(input: &str) -> Result<String> {
    let bits: String = input.chars().filter(|c| *c == '0' || *c == '1').collect();
    if bits.is_empty() || !bits.len().is_multiple_of(8) {
        bail!("Binary should be a valid ASCII binary string with multiples of 8 bits");
    }
    Ok(bits
        .as_bytes()
        .chunks(8)
        .map(|o| {
            let n = o.iter().fold(0u32, |a, b| a * 2 + (b - b'0') as u32);
            char::from_u32(n).unwrap_or('\u{fffd}')
        })
        .collect())
}

/// Two panels, as in it-tools: "Text to ASCII binary" and "ASCII binary to text".
fn text_to_binary(args: &Args) -> Result<String> {
    let t = &TOOLS[20];
    let text = arg(args, t, "input");
    let binary = arg(args, t, "binary");
    let mut out = Vec::new();
    if !text.trim().is_empty() {
        let sep = arg(args, t, "separator");
        let bits = text
            .chars()
            .map(|c| format!("{:08b}", c as u32))
            .collect::<Vec<_>>()
            .join(sep);
        out.push(crate::section("Binary from your text", Ok(bits)));
    }
    if !binary.trim().is_empty() {
        out.push(crate::section(
            "Text from your binary",
            binary_to_text(binary),
        ));
    }
    if out.is_empty() {
        bail!("no input");
    }
    Ok(out.join("\n\n"))
}

/// Two panels, as in it-tools: "Text to Unicode" and "Unicode to Text".
fn text_to_unicode(args: &Args) -> Result<String> {
    let t = &TOOLS[21];
    let text = arg(args, t, "input");
    let unicode = arg(args, t, "unicode");
    let mut out = Vec::new();
    if !text.trim().is_empty() {
        out.push(crate::section(
            "Unicode from your text",
            Ok(text.chars().map(|c| format!("&#{};", c as u32)).collect()),
        ));
    }
    if !unicode.trim().is_empty() {
        let re = regex::Regex::new(r"&#(\d+);").unwrap();
        let decoded = re
            .replace_all(unicode, |c: &regex::Captures| {
                c[1].parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .map(String::from)
                    .unwrap_or_default()
            })
            .into_owned();
        out.push(crate::section("Text from your Unicode", Ok(decoded)));
    }
    if out.is_empty() {
        bail!("no input");
    }
    Ok(out.join("\n\n"))
}

// ─── integer-base-converter / roman-numeral-converter ───────────────────────

const BASE_ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ+/";

fn convert_base(value: &str, from: u32, to: u32) -> Result<String> {
    if !(2..=64).contains(&from) || !(2..=64).contains(&to) {
        bail!("bases must be between 2 and 64");
    }
    let from_range = &BASE_ALPHABET[..from as usize];
    let mut acc = BigUint::ZERO;
    let base = BigUint::from(from);
    for ch in value.chars() {
        let b = u8::try_from(ch as u32)
            .ok()
            .filter(|b| from_range.contains(b));
        let digit = b.ok_or_else(|| anyhow!("Invalid digit \"{ch}\" for base {from}."))?;
        let idx = from_range.iter().position(|c| *c == digit).unwrap();
        acc = acc * &base + BigUint::from(idx);
    }
    if acc == BigUint::ZERO {
        return Ok("0".into());
    }
    let to_big = BigUint::from(to);
    let mut digits = Vec::new();
    while acc > BigUint::ZERO {
        let rem: u32 = (&acc % &to_big).try_into().unwrap();
        digits.push(BASE_ALPHABET[rem as usize]);
        acc /= &to_big;
    }
    digits.reverse();
    Ok(String::from_utf8(digits).unwrap())
}

/// it-tools shows one input base and *every* interesting output base at once
/// (2, 8, 10, 16, 64 plus one custom), so there is no output-base picker to
/// get wrong — `toBase` is only the custom row.
fn integer_base_converter(args: &Args) -> Result<String> {
    let t = &TOOLS[22];
    let value = arg(args, t, "input").trim();
    if value.is_empty() {
        bail!("no input");
    }
    let from = u32::try_from(num(args, t, "fromBase")?).unwrap_or(0);
    let custom = u32::try_from(num(args, t, "toBase")?).unwrap_or(0);
    // A bad input base or a stray digit fails every row alike; it-tools puts that
    // in an error alert above the (blank) rows, we make it the whole output.
    convert_base(value, from, 10)?;
    let rows = [
        ("Binary (2)".to_string(), 2u32),
        ("Octal (8)".to_string(), 8),
        ("Decimal (10)".to_string(), 10),
        ("Hexadecimal (16)".to_string(), 16),
        ("Base64 (64)".to_string(), 64),
        (format!("Base {custom}"), custom),
    ];
    Ok(rows
        .iter()
        .map(|(label, to)| match convert_base(value, from, *to) {
            Ok(s) => format!("{label}: {s}"),
            Err(e) => format!("{label}: ({e})"),
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

fn arabic_to_roman(input: &str) -> Result<String> {
    let f: f64 = input.parse().context("not a number")?;
    // it-tools range-checks the raw value (3999.1 is rejected) then truncates (1.9 -> I).
    if !(1.0..=3999.0).contains(&f) {
        bail!("We can only convert numbers between 1 and 3,999");
    }
    roman::to(f.floor() as i32)
        .ok_or_else(|| anyhow!("We can only convert numbers between 1 and 3,999"))
}

fn roman_to_arabic(input: &str) -> Result<String> {
    let re =
        regex::Regex::new(r"^M{0,3}(CM|CD|D?C{0,3})(XC|XL|L?X{0,3})(IX|IV|V?I{0,3})$").unwrap();
    let up = input.to_uppercase();
    if up.is_empty() || !re.is_match(&up) {
        bail!("The input you entered is not a valid roman number");
    }
    let n = roman::from(&up)
        .ok_or_else(|| anyhow!("The input you entered is not a valid roman number"))?;
    Ok(n.to_string())
}

/// it-tools stacks "Arabic to roman" and "Roman to arabic", each with its own
/// input box — no mode select.
fn roman_numeral(args: &Args) -> Result<String> {
    let t = &TOOLS[23];
    let arabic = arg(args, t, "arabic").trim();
    let roman_in = arg(args, t, "roman").trim();
    let mut out = Vec::new();
    if !arabic.is_empty() {
        out.push(crate::section("Arabic to roman", arabic_to_roman(arabic)));
    }
    if !roman_in.is_empty() {
        out.push(crate::section("Roman to arabic", roman_to_arabic(roman_in)));
    }
    if out.is_empty() {
        bail!("no input");
    }
    Ok(out.join("\n\n"))
}

// ─── registry ───────────────────────────────────────────────────────────────

/// it-tools' case-converter renders all of these at once, in this order, with
/// these labels. `Paramcase` is change-case's name for kebab-case.
const CASES: &[(&str, &str)] = &[
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

pub static TOOLS: &[Tool] = &[
    Tool {
        id: "base64-string-converter",
        name: "Base64 string encoder/decoder",
        description: "Encode text to base64 or decode a base64 string back to text.",
        category: Category::Converter,
        keywords: &["base64", "encode", "decode", "string"],
        fields: &[
            Field::area("input", "String to encode"),
            Field::bool("encodeUrlSafe", "Encode URL safe", "false"),
            Field::area("base64", "Base64 string to decode"),
            Field::bool("decodeUrlSafe", "Decode URL safe", "false"),
        ],
        kind: Kind::Pure,
        run: base64_string,
    },
    Tool {
        id: "base64-file-converter",
        name: "Base64 file converter",
        description: "Convert a file to and from its base64 representation.",
        category: Category::Converter,
        keywords: &["base64", "file", "upload", "data-url"],
        fields: &[],
        // Needs a file picker and a download target; the generic form cannot express it.
        kind: Kind::Custom,
        run: base64_file,
    },
    Tool {
        id: "json-to-csv",
        name: "JSON to CSV",
        description: "Convert a JSON array of objects to CSV.",
        category: Category::Converter,
        keywords: &["json", "csv", "table", "convert"],
        fields: &[Field::area("input", "JSON array")],
        kind: Kind::Pure,
        run: json_to_csv,
    },
    Tool {
        id: "json-to-toml",
        name: "JSON to TOML",
        description: "Convert JSON to TOML.",
        category: Category::Converter,
        keywords: &["json", "toml", "convert"],
        fields: &[Field::area("input", "JSON")],
        kind: Kind::Pure,
        run: json_to_toml,
    },
    Tool {
        id: "json-to-xml",
        name: "JSON to XML",
        description: "Convert JSON to XML (xml-js compact convention).",
        category: Category::Converter,
        keywords: &["json", "xml", "convert"],
        fields: &[Field::area("input", "JSON")],
        kind: Kind::Pure,
        run: json_to_xml,
    },
    Tool {
        id: "json-to-yaml-converter",
        name: "JSON to YAML",
        description: "Convert JSON to YAML.",
        category: Category::Converter,
        keywords: &["json", "yaml", "yml", "convert"],
        fields: &[Field::area("input", "JSON")],
        kind: Kind::Pure,
        run: json_to_yaml,
    },
    Tool {
        id: "json-minify",
        name: "JSON minify",
        description: "Strip all insignificant whitespace from a JSON document.",
        category: Category::Data,
        keywords: &["json", "minify", "compress"],
        fields: &[Field::area("input", "JSON")],
        kind: Kind::Pure,
        run: json_minify,
    },
    Tool {
        id: "json-diff",
        name: "JSON diff",
        description: "Compare two JSON documents as a unified diff.",
        category: Category::Data,
        keywords: &["json", "diff", "compare"],
        fields: &[
            Field::area("left", "Left JSON"),
            Field::area("right", "Right JSON"),
        ],
        kind: Kind::Pure,
        run: json_diff,
    },
    Tool {
        id: "json-viewer",
        name: "JSON prettify and validate",
        description: "Format and validate a JSON document.",
        category: Category::Data,
        keywords: &["json", "viewer", "prettify", "format", "validate"],
        fields: &[
            Field::area("input", "JSON"),
            // it-tools' defaults: 3-space indent, keys sorted.
            Field::number("indent", "Indent size", "3"),
            Field::bool("sortKeys", "Sort keys", "true"),
        ],
        kind: Kind::Pure,
        run: json_viewer,
    },
    Tool {
        id: "toml-to-json",
        name: "TOML to JSON",
        description: "Convert TOML to JSON.",
        category: Category::Converter,
        keywords: &["toml", "json", "convert"],
        fields: &[Field::area("input", "TOML")],
        kind: Kind::Pure,
        run: toml_to_json,
    },
    Tool {
        id: "toml-to-yaml",
        name: "TOML to YAML",
        description: "Convert TOML to YAML.",
        category: Category::Converter,
        keywords: &["toml", "yaml", "yml", "convert"],
        fields: &[Field::area("input", "TOML")],
        kind: Kind::Pure,
        run: toml_to_yaml,
    },
    Tool {
        id: "yaml-to-json-converter",
        name: "YAML to JSON",
        description: "Convert YAML to JSON.",
        category: Category::Converter,
        keywords: &["yaml", "yml", "json", "convert"],
        fields: &[Field::area("input", "YAML")],
        kind: Kind::Pure,
        run: yaml_to_json,
    },
    Tool {
        id: "yaml-to-toml",
        name: "YAML to TOML",
        description: "Convert YAML to TOML.",
        category: Category::Converter,
        keywords: &["yaml", "yml", "toml", "convert"],
        fields: &[Field::area("input", "YAML")],
        kind: Kind::Pure,
        run: yaml_to_toml,
    },
    Tool {
        id: "yaml-viewer",
        name: "YAML prettify and validate",
        description: "Format and validate a YAML document.",
        category: Category::Data,
        keywords: &["yaml", "yml", "viewer", "prettify", "validate"],
        fields: &[
            Field::area("input", "YAML"),
            Field::bool("sortKeys", "Sort keys", "false"),
        ],
        kind: Kind::Pure,
        run: yaml_viewer,
    },
    Tool {
        id: "xml-to-json",
        name: "XML to JSON",
        description: "Convert XML to JSON (xml-js compact convention).",
        category: Category::Converter,
        keywords: &["xml", "json", "convert"],
        fields: &[Field::area("input", "XML")],
        kind: Kind::Pure,
        run: xml_to_json,
    },
    Tool {
        id: "xml-formatter",
        name: "XML formatter",
        description: "Indent an XML document and report syntax errors.",
        category: Category::Data,
        keywords: &["xml", "format", "prettify", "indent"],
        fields: &[
            Field::area("input", "XML"),
            Field::number("indent", "Indent size", "2"),
            Field::bool("collapseContent", "Collapse content", "true"),
        ],
        kind: Kind::Pure,
        run: xml_formatter,
    },
    Tool {
        id: "list-converter",
        name: "List converter",
        description: "Join, sort, dedupe and decorate the lines of a list.",
        category: Category::Data,
        keywords: &["list", "join", "sort", "dedupe", "convert"],
        fields: &[
            Field::area("input", "List (one item per line)"),
            // it-tools defaults the separator to ", " and dedupes by default.
            Field {
                key: "separator",
                label: "Separator",
                kind: crate::FieldKind::Text,
                default: ", ",
                options: &[],
            },
            Field::text("itemPrefix", "Item prefix"),
            Field::text("itemSuffix", "Item suffix"),
            Field::text("listPrefix", "List prefix"),
            Field::text("listSuffix", "List suffix"),
            Field::bool("trimItems", "Trim items", "true"),
            Field::bool("removeDuplicates", "Remove duplicates", "true"),
            Field::bool("reverseList", "Reverse list", "false"),
            Field::select("sortList", "Sort", &["none", "asc", "desc"], "none"),
            Field::bool("lowerCase", "Lowercase", "false"),
            Field::bool("keepLineBreaks", "Keep line breaks", "false"),
        ],
        kind: Kind::Pure,
        run: list_converter,
    },
    Tool {
        id: "markdown-to-html",
        name: "Markdown to HTML",
        description: "Render Markdown (with tables, task lists and footnotes) to HTML.",
        category: Category::Converter,
        keywords: &["markdown", "md", "html", "convert"],
        fields: &[Field::area("input", "Markdown")],
        kind: Kind::Pure,
        run: markdown_to_html,
    },
    Tool {
        id: "sql-prettify",
        name: "SQL prettify and format",
        description: "Indent and normalise the keyword case of a SQL query.",
        category: Category::Development,
        keywords: &["sql", "format", "prettify", "query"],
        fields: &[
            Field::area("input", "SQL query"),
            Field::select(
                "keywordCase",
                "Keyword case",
                &["upper", "lower", "preserve"],
                "upper",
            ),
            Field::number("indent", "Indent size", "2"),
        ],
        kind: Kind::Pure,
        run: sql_prettify,
    },
    Tool {
        id: "case-converter",
        name: "Case converter",
        description: "Show a string in camel, snake, kebab and every other case at once.",
        category: Category::Text,
        keywords: &["case", "camel", "snake", "kebab", "pascal", "convert"],
        fields: &[Field::area("input", "Your string")],
        kind: Kind::Pure,
        run: case_converter,
    },
    Tool {
        id: "text-to-binary",
        name: "Text to ASCII binary",
        description: "Convert text to its binary representation and back.",
        category: Category::Converter,
        keywords: &["text", "binary", "ascii", "convert"],
        fields: &[
            Field::area("input", "Text to convert to binary"),
            Field {
                key: "separator",
                label: "Octet separator",
                kind: crate::FieldKind::Text,
                default: " ",
                options: &[],
            },
            Field::area("binary", "Binary to convert to text"),
        ],
        kind: Kind::Pure,
        run: text_to_binary,
    },
    Tool {
        id: "text-to-unicode",
        name: "Text to Unicode",
        description: "Convert text to Unicode HTML entities and back.",
        category: Category::Converter,
        keywords: &["text", "unicode", "entity", "convert"],
        fields: &[
            Field::area("input", "Text to convert to unicode"),
            Field::area("unicode", "Unicode to convert to text"),
        ],
        kind: Kind::Pure,
        run: text_to_unicode,
    },
    Tool {
        id: "integer-base-converter",
        name: "Integer base converter",
        description: "Show an arbitrarily large integer in base 2, 8, 10, 16, 64 and one of your choice.",
        category: Category::Converter,
        keywords: &["integer", "base", "binary", "hex", "radix", "convert"],
        fields: &[
            Field {
                key: "input",
                label: "Input number",
                kind: crate::FieldKind::Text,
                default: "42",
                options: &[],
            },
            Field::number("fromBase", "Input base", "10"),
            Field::number("toBase", "Custom output base", "42"),
        ],
        kind: Kind::Pure,
        run: integer_base_converter,
    },
    Tool {
        id: "roman-numeral-converter",
        name: "Roman numeral converter",
        description: "Convert between arabic numbers (1-3999) and roman numerals.",
        category: Category::Converter,
        keywords: &["roman", "numeral", "arabic", "convert"],
        fields: &[
            Field {
                key: "arabic",
                label: "Arabic number to convert",
                kind: crate::FieldKind::Text,
                default: "42",
                options: &[],
            },
            Field {
                key: "roman",
                label: "Roman number to convert",
                kind: crate::FieldKind::Text,
                default: "XLII",
                options: &[],
            },
        ],
        kind: Kind::Pure,
        run: roman_numeral,
    },
];

// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn args(pairs: &[(&str, &str)]) -> Args {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn run(id: &str, pairs: &[(&str, &str)]) -> Result<String> {
        let t = TOOLS.iter().find(|t| t.id == id).expect("tool exists");
        (t.run)(&args(pairs))
    }

    fn ok(id: &str, pairs: &[(&str, &str)]) -> String {
        run(id, pairs).unwrap_or_else(|e| panic!("{id} failed: {e:#}"))
    }

    // ── registry contract ───────────────────────────────────────────────

    #[test]
    fn ids_are_unique_and_sane() {
        let mut ids: Vec<&str> = TOOLS.iter().map(|t| t.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "duplicate tool id");
        assert_eq!(n, 24);
    }

    #[test]
    fn empty_args_never_panic() {
        let empty = Args::new();
        for t in TOOLS {
            let _ = (t.run)(&empty);
        }
    }

    #[test]
    fn only_the_file_converter_is_custom() {
        let custom: Vec<&str> = TOOLS
            .iter()
            .filter(|t| t.kind == Kind::Custom)
            .map(|t| t.id)
            .collect();
        assert_eq!(custom, vec!["base64-file-converter"]);
        assert!(run("base64-file-converter", &[]).is_err());
    }

    #[test]
    fn select_defaults_are_listed_options() {
        for t in TOOLS {
            for f in t.fields {
                if f.kind == crate::FieldKind::Select {
                    assert!(f.options.contains(&f.default), "{}/{}", t.id, f.key);
                }
            }
        }
    }

    // ── base64 ──────────────────────────────────────────────────────────

    /// The `mode` select is gone: it-tools renders both directions at once, so
    /// these assert on the two labelled panels instead.
    #[test]
    fn base64_round_trip() {
        assert_eq!(
            ok("base64-string-converter", &[("input", "Hello, World!")]),
            "Base64 of string:\nSGVsbG8sIFdvcmxkIQ=="
        );
        assert_eq!(
            ok(
                "base64-string-converter",
                &[("base64", "SGVsbG8sIFdvcmxkIQ==")]
            ),
            "Decoded string:\nHello, World!"
        );
        // url-safe encoding drops padding and uses -_
        let url = ok(
            "base64-string-converter",
            &[("input", "\u{f8}\u{ff}?"), ("encodeUrlSafe", "true")],
        );
        let url = url.strip_prefix("Base64 of string:\n").unwrap().to_string();
        assert!(
            !url.contains('=') && !url.contains('+') && !url.contains('/'),
            "{url}"
        );
        assert_eq!(
            ok(
                "base64-string-converter",
                &[("base64", &url), ("decodeUrlSafe", "true")]
            ),
            "Decoded string:\n\u{f8}\u{ff}?"
        );
        // `decodeUrlSafe` is wired, not decorative: `-_` are only legal with it on.
        assert!(
            run("base64-string-converter", &[("base64", "-_-_")])
                .unwrap()
                .contains("Invalid base64 string")
        );
        // and it-tools strips a data-URL prefix before decoding
        assert_eq!(
            ok(
                "base64-string-converter",
                &[("base64", "data:text/plain;base64,aGk=")]
            ),
            "Decoded string:\nhi"
        );
    }

    /// The original bug report: `TESTtest` is valid base64 over six *binary*
    /// bytes. it-tools shows the (lossy) decode; we must not error out.
    #[test]
    fn base64_shows_both_panels_for_binary_payload() {
        let out = ok(
            "base64-string-converter",
            &[("input", "TESTtest"), ("base64", "TESTtest")],
        );
        assert!(out.contains("Base64 of string:\nVEVTVHRlc3Q="), "{out}");
        assert!(out.contains("Decoded string:\n"), "{out}");
        // Six bytes came out, however unprintable they are.
        assert!(!out.ends_with("Decoded string:\n"), "{out}");
    }

    /// Garbage in one panel must never blank the other.
    #[test]
    fn base64_one_bad_panel_keeps_the_other() {
        let out = ok(
            "base64-string-converter",
            &[("input", "hello"), ("base64", "!!!!")],
        );
        assert!(out.contains("Base64 of string:\naGVsbG8="), "{out}");
        assert!(out.contains("Invalid base64 string"), "{out}");
    }

    #[test]
    fn base64_rejects_only_a_wholly_empty_form() {
        assert!(run("base64-string-converter", &[]).is_err());
        assert!(run("base64-string-converter", &[("input", "")]).is_err());
        // one filled panel is enough
        assert!(run("base64-string-converter", &[("base64", "aGk=")]).is_ok());
    }

    // ── json-to-csv (vectors from json-to-csv.service.test.ts) ──────────

    #[test]
    fn json_to_csv_vectors() {
        assert_eq!(
            ok(
                "json-to-csv",
                &[("input", r#"[{"a":1,"b":2},{"a":3,"b":4}]"#)]
            ),
            "a,b\n1,2\n3,4"
        );
        assert_eq!(
            ok(
                "json-to-csv",
                &[("input", r#"[{"a":1,"b":2},{"a":3,"c":4}]"#)]
            ),
            "a,b,c\n1,2,\n3,,4"
        );
        assert_eq!(
            ok("json-to-csv", &[("input", r#"[{"a":null,"b":2}]"#)]),
            "a,b\nnull,2"
        );
        assert_eq!(
            ok("json-to-csv", &[("input", r#"[{"b":2},{"b":3}]"#)]),
            "b\n2\n3"
        );
        assert_eq!(
            ok(
                "json-to-csv",
                &[("input", r#"[{"a":"hello, world","b":2}]"#)]
            ),
            "a,b\n\"hello, world\",2"
        );
        assert_eq!(
            ok(
                "json-to-csv",
                &[("input", r#"[{"a":"hello \"world\"","b":2}]"#)]
            ),
            "a,b\nhello \\\"world\\\",2"
        );
        assert_eq!(ok("json-to-csv", &[("input", "[]")]), "");
    }

    #[test]
    fn json_to_csv_rejects_non_arrays() {
        assert!(run("json-to-csv", &[("input", "{}")]).is_err());
        assert!(run("json-to-csv", &[("input", "[1,2]")]).is_err());
        assert!(run("json-to-csv", &[("input", "[{")]).is_err());
        assert!(run("json-to-csv", &[("input", "")]).is_err());
    }

    // ── json / yaml / toml round trips ──────────────────────────────────

    const J: &str = r#"{"hello":["world"],"n":1}"#;

    #[test]
    fn json_yaml_round_trip() {
        let y = ok("json-to-yaml-converter", &[("input", J)]);
        assert!(y.contains("hello:"), "{y}");
        let back = ok("yaml-to-json-converter", &[("input", &y)]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&back).unwrap(),
            serde_json::from_str::<serde_json::Value>(J).unwrap()
        );
    }

    #[test]
    fn json_toml_round_trip() {
        let t = ok("json-to-toml", &[("input", J)]);
        assert!(t.contains("n = 1"), "{t}");
        let back = ok("toml-to-json", &[("input", &t)]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&back).unwrap(),
            serde_json::from_str::<serde_json::Value>(J).unwrap()
        );
    }

    #[test]
    fn toml_yaml_round_trip() {
        let y = ok("toml-to-yaml", &[("input", "a = 1\nb = \"x\"\n")]);
        assert!(y.contains("a: 1"), "{y}");
        let t = ok("yaml-to-toml", &[("input", &y)]);
        assert_eq!(
            ok("toml-to-json", &[("input", &t)]),
            "{\n  \"a\": 1,\n  \"b\": \"x\"\n}"
        );
    }

    #[test]
    fn json_minify_and_viewer() {
        assert_eq!(
            ok(
                "json-minify",
                &[("input", "{\n\t\"hello\": [\n\t\t\"world\"\n\t]\n}")]
            ),
            r#"{"hello":["world"]}"#
        );
        // it-tools' defaults are a 3-space indent with keys sorted.
        assert_eq!(
            ok("json-viewer", &[("input", r#"{"b":1,"a":2}"#)]),
            "{\n   \"a\": 2,\n   \"b\": 1\n}"
        );
        assert_eq!(
            ok(
                "json-viewer",
                &[("input", r#"{"b":1,"a":2}"#), ("sortKeys", "false")]
            ),
            "{\n   \"b\": 1,\n   \"a\": 2\n}"
        );
        assert_eq!(
            ok(
                "json-viewer",
                &[("input", r#"{"b":{"d":1,"c":2}}"#), ("indent", "2")]
            ),
            "{\n  \"b\": {\n    \"c\": 2,\n    \"d\": 1\n  }\n}"
        );
        assert_eq!(
            ok("json-viewer", &[("input", r#"{"a":1}"#), ("indent", "0")]),
            r#"{"a":1}"#
        );
        assert_eq!(
            ok("json-viewer", &[("input", r#"{"a":1}"#), ("indent", "4")]),
            "{\n    \"a\": 1\n}"
        );
    }

    #[test]
    fn json_diff_reports_changes_and_equality() {
        assert_eq!(
            ok(
                "json-diff",
                &[("left", r#"{"a":1}"#), ("right", r#"{"a":1}"#)]
            ),
            "the two documents are equal"
        );
        let d = ok(
            "json-diff",
            &[("left", r#"{"a":1}"#), ("right", r#"{"a":2}"#)],
        );
        assert!(d.contains("-  \"a\": 1"), "{d}");
        assert!(d.contains("+  \"a\": 2"), "{d}");
        assert!(run("json-diff", &[("left", "{"), ("right", "{}")]).is_err());
        assert!(run("json-diff", &[("left", "{}")]).is_err());
    }

    #[test]
    fn malformed_documents_return_err() {
        for id in [
            "json-to-toml",
            "json-to-xml",
            "json-to-yaml-converter",
            "json-minify",
            "json-viewer",
        ] {
            assert!(
                run(id, &[("input", "{\"a\":")]).is_err(),
                "{id} accepted half-typed JSON"
            );
            assert!(
                run(id, &[("input", "")]).is_err(),
                "{id} accepted empty input"
            );
        }
        assert!(run("toml-to-json", &[("input", "a = ")]).is_err());
        assert!(run("toml-to-yaml", &[("input", "= 1")]).is_err());
        assert!(run("yaml-to-json-converter", &[("input", "{a: 1")]).is_err());
        assert!(run("yaml-to-toml", &[("input", "1")]).is_err()); // scalar root is not a TOML table
        assert!(run("yaml-viewer", &[("input", "")]).is_err());
        for id in [
            "toml-to-json",
            "toml-to-yaml",
            "yaml-to-json-converter",
            "yaml-to-toml",
            "yaml-viewer",
        ] {
            assert!(
                run(id, &[("input", "")]).is_err(),
                "{id} accepted empty input"
            );
        }
    }

    #[test]
    fn yaml_viewer_normalises() {
        let out = ok("yaml-viewer", &[("input", "b:   2\na: 1")]);
        assert!(out.starts_with("b: 2"), "{out}");
        let sorted = ok(
            "yaml-viewer",
            &[("input", "b:   2\na: 1"), ("sortKeys", "true")],
        );
        assert!(sorted.starts_with("a: 1"), "{sorted}");
    }

    // ── xml ─────────────────────────────────────────────────────────────

    #[test]
    fn xml_json_round_trip() {
        // it-tools default sample for xml-to-json
        let j = ok("xml-to-json", &[("input", "<a x=\"1.234\" y=\"It's\"/>")]);
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["a"]["_attributes"]["x"], "1.234");
        assert_eq!(v["a"]["_attributes"]["y"], "It's");
        // and its json-to-xml counterpart
        assert_eq!(
            ok(
                "json-to-xml",
                &[("input", r#"{"a":{"_attributes":{"x":"1.234","y":"It's"}}}"#)]
            ),
            "<a x=\"1.234\" y=\"It&apos;s\"/>"
        );
        assert_eq!(
            ok(
                "json-to-xml",
                &[("input", r#"{"hello":{"world":["foo","bar"]}}"#)]
            ),
            "<hello><world>foo</world><world>bar</world></hello>"
        );
        let back = ok(
            "xml-to-json",
            &[(
                "input",
                "<hello><world>foo</world><world>bar</world></hello>",
            )],
        );
        let v: serde_json::Value = serde_json::from_str(&back).unwrap();
        assert_eq!(v["hello"]["world"][0]["_text"], "foo");
        assert_eq!(v["hello"]["world"][1]["_text"], "bar");
    }

    /// Vector from xml-formatter.service.test.ts, which calls the service with no
    /// options (4-space indent, no collapsing). The *UI* defaults differ, so they
    /// are passed explicitly here and asserted separately below.
    #[test]
    fn xml_formatter_vector() {
        assert_eq!(
            ok(
                "xml-formatter",
                &[
                    (
                        "input",
                        "<hello><world>foo</world><world>bar</world></hello>"
                    ),
                    ("indent", "4"),
                    ("collapseContent", "false"),
                ]
            ),
            "<hello>\n    <world>\n        foo\n    </world>\n    <world>\n        bar\n    </world>\n</hello>"
        );
        assert_eq!(
            ok(
                "xml-formatter",
                &[("input", "<a><b>c</b></a>"), ("collapseContent", "false")]
            ),
            "<a>\n  <b>\n    c\n  </b>\n</a>"
        );
    }

    /// it-tools' UI defaults: 2-space indent with `collapseContent` on, so a
    /// text-only element stays on one line.
    #[test]
    fn xml_formatter_collapses_text_content_by_default() {
        assert_eq!(
            ok(
                "xml-formatter",
                &[(
                    "input",
                    "<hello><world>foo</world><world>bar</world></hello>"
                )]
            ),
            "<hello>\n  <world>foo</world>\n  <world>bar</world>\n</hello>"
        );
        assert_eq!(ok("xml-formatter", &[("input", "<a></a>")]), "<a></a>");
    }

    #[test]
    fn xml_rejects_non_xml() {
        for id in ["xml-formatter", "xml-to-json"] {
            assert!(
                run(id, &[("input", "hello world")]).is_err(),
                "{id} accepted plain text"
            );
            assert!(
                run(id, &[("input", "<a><b></a>")]).is_err(),
                "{id} accepted mismatched tags"
            );
            assert!(
                run(id, &[("input", "<a>")]).is_err(),
                "{id} accepted unclosed tag"
            );
            assert!(
                run(id, &[("input", "")]).is_err(),
                "{id} accepted empty input"
            );
        }
    }

    // ── list-converter (vectors from list-converter.models.test.ts) ─────

    #[test]
    fn list_converter_vectors() {
        let base = [
            ("separator", ", "),
            ("trimItems", "true"),
            ("removeDuplicates", "true"),
            ("itemPrefix", "\""),
            ("itemSuffix", "\""),
        ];
        let mut a = base.to_vec();
        a.push((
            "input",
            "\n        1\n        2\n        \n        3\n        3\n        4\n        ",
        ));
        assert_eq!(ok("list-converter", &a), "\"1\", \"2\", \"3\", \"4\"");

        assert_eq!(
            ok("list-converter", &[("input", ""), ("trimItems", "true")]),
            ""
        );

        assert_eq!(
            ok(
                "list-converter",
                &[
                    ("input", "\n        1\n        2\n        3\n        "),
                    ("separator", ""),
                    ("trimItems", "true"),
                    ("itemPrefix", "<li>"),
                    ("itemSuffix", "</li>"),
                    ("listPrefix", "<ul>"),
                    ("listSuffix", "</ul>"),
                    ("keepLineBreaks", "true"),
                ]
            ),
            "<ul>\n<li>1</li>\n<li>2</li>\n<li>3</li>\n</ul>"
        );
    }

    #[test]
    fn list_converter_sorts_reverses_and_lowercases() {
        let input = "b\nA\nc";
        assert_eq!(
            ok(
                "list-converter",
                &[
                    ("input", input),
                    ("separator", ","),
                    ("sortList", "asc"),
                    ("lowerCase", "true")
                ]
            ),
            "a,b,c"
        );
        assert_eq!(
            ok(
                "list-converter",
                &[
                    ("input", input),
                    ("separator", ","),
                    ("sortList", "desc"),
                    ("lowerCase", "true")
                ]
            ),
            "c,b,a"
        );
        assert_eq!(
            ok(
                "list-converter",
                &[
                    ("input", input),
                    ("separator", ","),
                    ("reverseList", "true")
                ]
            ),
            "c,A,b"
        );
    }

    // ── markdown / sql ──────────────────────────────────────────────────

    #[test]
    fn markdown_to_html_renders() {
        assert_eq!(
            ok("markdown-to-html", &[("input", "# hi")]),
            "<h1>hi</h1>\n"
        );
        let t = ok("markdown-to-html", &[("input", "| a |\n|---|\n| 1 |")]);
        assert!(t.contains("<table>"), "{t}");
        assert!(run("markdown-to-html", &[("input", "  ")]).is_err());
    }

    #[test]
    fn sql_prettify_formats() {
        let out = ok(
            "sql-prettify",
            &[("input", "select field1,field2 from my_table where a=1;")],
        );
        assert!(out.starts_with("SELECT"), "{out}");
        assert!(out.contains("FROM"), "{out}");
        let lower = ok(
            "sql-prettify",
            &[("input", "SELECT a FROM b"), ("keywordCase", "lower")],
        );
        assert!(lower.starts_with("select"), "{lower}");
        assert!(run("sql-prettify", &[("input", "")]).is_err());
        assert!(run("sql-prettify", &[("input", "select 1"), ("indent", "x")]).is_err());
    }

    // ── case-converter ──────────────────────────────────────────────────

    /// it-tools has no case picker — it lists every case at once, so this now
    /// asserts on the lines of one output rather than on a `case` field.
    #[test]
    fn every_case_is_covered() {
        let out = ok("case-converter", &[("input", "lorem ipsum dolor sit amet")]);
        let expect = [
            "Lowercase: lorem ipsum dolor sit amet",
            "Uppercase: LOREM IPSUM DOLOR SIT AMET",
            "Camelcase: loremIpsumDolorSitAmet",
            "Capitalcase: Lorem Ipsum Dolor Sit Amet",
            "Constantcase: LOREM_IPSUM_DOLOR_SIT_AMET",
            "Dotcase: lorem.ipsum.dolor.sit.amet",
            "Headercase: Lorem-Ipsum-Dolor-Sit-Amet",
            "Nocase: lorem ipsum dolor sit amet",
            "Paramcase: lorem-ipsum-dolor-sit-amet",
            "Pascalcase: LoremIpsumDolorSitAmet",
            "Pathcase: lorem/ipsum/dolor/sit/amet",
            "Sentencecase: Lorem ipsum dolor sit amet",
            "Snakecase: lorem_ipsum_dolor_sit_amet",
            "Mockingcase: LoReM IpSuM DoLoR SiT AmEt",
        ];
        assert_eq!(out.lines().collect::<Vec<_>>(), expect);
        assert_eq!(CASES.len(), expect.len());
    }

    #[test]
    fn case_converter_splits_camel_and_acronyms() {
        let out = ok("case-converter", &[("input", "fooBarBaz")]);
        assert!(out.contains("Snakecase: foo_bar_baz"), "{out}");
        let out = ok("case-converter", &[("input", "HTTPServer")]);
        assert!(out.contains("Paramcase: http-server"), "{out}");
        let out = ok("case-converter", &[("input", "foo-bar_baz qux")]);
        assert!(out.contains("Camelcase: fooBarBazQux"), "{out}");
        assert!(run("case-converter", &[("input", "")]).is_err());
    }

    // ── text-to-binary (vectors from text-to-binary.models.test.ts) ─────

    /// Both directions at once, as in it-tools — the `mode` select is gone.
    #[test]
    fn text_to_binary_vectors() {
        let enc = |pairs: &[(&str, &str)]| {
            ok("text-to-binary", pairs)
                .strip_prefix("Binary from your text:\n")
                .unwrap()
                .to_string()
        };
        let dec = |pairs: &[(&str, &str)]| {
            ok("text-to-binary", pairs)
                .strip_prefix("Text from your binary:\n")
                .unwrap()
                .to_string()
        };
        assert_eq!(enc(&[("input", "A"), ("separator", " ")]), "01000001");
        assert_eq!(
            enc(&[("input", "hello"), ("separator", " ")]),
            "01101000 01100101 01101100 01101100 01101111"
        );
        assert_eq!(
            enc(&[("input", "hello"), ("separator", "")]),
            "0110100001100101011011000110110001101111"
        );
        assert_eq!(
            dec(&[("binary", "01101000 01100101 01101100 01101100 01101111")]),
            "hello"
        );
        assert_eq!(dec(&[("binary", "01000001")]), "A");
        // input is cleaned before decoding
        assert_eq!(dec(&[("binary", "  01000 001garbage")]), "A");
        // a bad panel degrades to a note in that panel only
        let out = ok("text-to-binary", &[("input", "A"), ("binary", "010000011")]);
        assert!(out.contains("Binary from your text:\n01000001"), "{out}");
        assert!(out.contains("multiples of 8 bits"), "{out}");
        assert!(
            run("text-to-binary", &[("binary", "1")])
                .unwrap()
                .contains("multiples of 8 bits")
        );
        assert!(run("text-to-binary", &[("input", "")]).is_err());
        assert!(run("text-to-binary", &[]).is_err());
    }

    // ── text-to-unicode (vectors from text-to-unicode.service.test.ts) ──

    const UNI: &str = "&#108;&#105;&#110;&#107;&#101;&#32;&#116;&#104;&#101;&#32;&#115;&#116;&#114;&#105;&#110;&#103;&#32;&#99;&#111;&#110;&#118;&#101;&#114;&#116;&#32;&#116;&#111;&#32;&#117;&#110;&#105;&#99;&#111;&#100;&#101;";

    #[test]
    fn text_to_unicode_vectors() {
        assert_eq!(
            ok("text-to-unicode", &[("input", "A")]),
            "Unicode from your text:\n&#65;"
        );
        assert_eq!(
            ok(
                "text-to-unicode",
                &[("input", "linke the string convert to unicode")]
            ),
            format!("Unicode from your text:\n{UNI}")
        );
        assert_eq!(
            ok("text-to-unicode", &[("unicode", "&#65;")]),
            "Text from your Unicode:\nA"
        );
        assert_eq!(
            ok("text-to-unicode", &[("unicode", UNI)]),
            "Text from your Unicode:\nlinke the string convert to unicode"
        );
        // both panels render together
        let out = ok("text-to-unicode", &[("input", "A"), ("unicode", "&#66;")]);
        assert_eq!(
            out,
            "Unicode from your text:\n&#65;\n\nText from your Unicode:\nB"
        );
        assert!(run("text-to-unicode", &[("input", "")]).is_err());
        assert!(run("text-to-unicode", &[]).is_err());
    }

    // ── integer-base-converter (vectors from the model test) ────────────

    #[test]
    fn integer_base_converter_vectors() {
        let cases = [
            ("0", 2, 11, "0"),
            ("0", 5, 2, "0"),
            ("0", 10, 16, "0"),
            ("10100101", 2, 16, "a5"),
            ("192654", 10, 8, "570216"),
            ("zz", 64, 10, "2275"),
            (
                "42540766411283223938465490632011909384",
                10,
                10,
                "42540766411283223938465490632011909384",
            ),
            (
                "42540766411283223938465490632011909384",
                10,
                16,
                "20010db8000085a300000000ac1f8908",
            ),
            (
                "20010db8000085a300000000ac1f8908",
                16,
                10,
                "42540766411283223938465490632011909384",
            ),
        ];
        // The output-base picker is gone: it-tools lists 2/8/10/16/64 plus the
        // custom base at once, so `toBase` is only the custom row.
        for (value, from, to, want) in cases {
            let out = ok(
                "integer-base-converter",
                &[
                    ("input", value),
                    ("fromBase", &from.to_string()),
                    ("toBase", &to.to_string()),
                ],
            );
            assert!(
                out.lines().any(|l| l == format!("Base {to}: {want}")),
                "{value} base {from} -> {to}:\n{out}"
            );
        }
        // all six rows, always
        let out = ok("integer-base-converter", &[("input", "255")]);
        assert!(out.contains("Binary (2): 11111111"), "{out}");
        assert!(out.contains("Octal (8): 377"), "{out}");
        assert!(out.contains("Decimal (10): 255"), "{out}");
        assert!(out.contains("Hexadecimal (16): ff"), "{out}");
        assert!(out.contains("Base64 (64): 3/"), "{out}");
        assert_eq!(out.lines().count(), 6, "{out}");
    }

    #[test]
    fn integer_base_converter_rejects_bad_input() {
        // A stray digit or a bad *input* base kills every row, so it errors.
        assert!(
            run(
                "integer-base-converter",
                &[("input", "12"), ("fromBase", "2")]
            )
            .is_err()
        );
        assert!(
            run(
                "integer-base-converter",
                &[("input", "1"), ("fromBase", "1")]
            )
            .is_err()
        );
        assert!(
            run(
                "integer-base-converter",
                &[("input", "1"), ("fromBase", "abc")]
            )
            .is_err()
        );
        // An out-of-range custom base only spoils its own row.
        let out = ok(
            "integer-base-converter",
            &[("input", "1"), ("toBase", "65")],
        );
        assert!(out.contains("Decimal (10): 1"), "{out}");
        assert!(
            out.contains("Base 65: (bases must be between 2 and 64)"),
            "{out}"
        );
        assert!(run("integer-base-converter", &[("input", "")]).is_err());
    }

    // ── roman-numeral-converter (vectors from the service test) ─────────

    #[test]
    fn roman_numeral_vectors() {
        let cases = [
            (1, "I"),
            (2, "II"),
            (3, "III"),
            (4, "IV"),
            (5, "V"),
            (6, "VI"),
            (7, "VII"),
            (8, "VIII"),
            (9, "IX"),
            (10, "X"),
            (14, "XIV"),
            (19, "XIX"),
            (24, "XXIV"),
            (29, "XXIX"),
            (40, "XL"),
            (50, "L"),
            (90, "XC"),
            (400, "CD"),
            (500, "D"),
            (900, "CM"),
            (999, "CMXCIX"),
            (1000, "M"),
            (2000, "MM"),
            (3999, "MMMCMXCIX"),
        ];
        // Two panels, one input each — no `mode` select. Both fields carry
        // it-tools' prefilled defaults, so the unused one is blanked here.
        for (n, want) in cases {
            assert_eq!(
                ok(
                    "roman-numeral-converter",
                    &[("arabic", &n.to_string()), ("roman", "")]
                ),
                format!("Arabic to roman:\n{want}"),
                "{n}"
            );
            assert_eq!(
                ok(
                    "roman-numeral-converter",
                    &[("arabic", ""), ("roman", want)]
                ),
                format!("Roman to arabic:\n{n}")
            );
        }
        // floating point truncates down
        for (input, want) in [("1.9", "I"), ("17.6", "XVII"), ("29.999", "XXIX")] {
            assert_eq!(
                ok(
                    "roman-numeral-converter",
                    &[("arabic", input), ("roman", "")]
                ),
                format!("Arabic to roman:\n{want}")
            );
        }
        // both at once, and one bad side does not hide the good one
        assert_eq!(
            ok(
                "roman-numeral-converter",
                &[("arabic", "42"), ("roman", "X")]
            ),
            "Arabic to roman:\nXLII\n\nRoman to arabic:\n10"
        );
        let out = ok(
            "roman-numeral-converter",
            &[("arabic", "42"), ("roman", "IIII")],
        );
        assert!(out.contains("Arabic to roman:\nXLII"), "{out}");
        assert!(out.contains("not a valid roman number"), "{out}");
    }

    #[test]
    fn roman_numeral_out_of_range_and_garbage() {
        for bad in ["0", "0.9", "-42", "4000", "10000", "3999.1", "abc"] {
            let out = ok("roman-numeral-converter", &[("arabic", bad), ("roman", "")]);
            assert!(out.contains('('), "{bad:?} accepted: {out}");
        }
        for bad in ["IIII", "ABC", "XM"] {
            let out = ok("roman-numeral-converter", &[("arabic", ""), ("roman", bad)]);
            assert!(
                out.contains("not a valid roman number"),
                "{bad:?} accepted: {out}"
            );
        }
        // nothing typed anywhere is the only hard error
        assert!(run("roman-numeral-converter", &[("arabic", ""), ("roman", "")]).is_err());
    }
}
