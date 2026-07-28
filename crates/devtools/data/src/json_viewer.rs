//! `json-viewer`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::Result;

/// Replace every string that is itself a JSON document with the document.
///
/// it-tools PRs #1542/#1539/#1643: logs and message queues are full of
/// `{"payload":"{\"a\":1}"}`, and reading that in a pretty printer means
/// counting backslashes by hand. Returns `true` when anything was unwrapped, so
/// the caller can leave plain documents alone instead of showing an identical
/// second panel.
///
/// Only objects and arrays are unwrapped. A string holding `1`, `true` or
/// `null` is far more often a string that happens to look like a number than a
/// nested document, and turning `"null"` into `null` would be a lie.
///
/// `depth` bounds the "a string inside the string" case; `serde_json` itself
/// stops at 128 levels, and each unwrapping step consumes real characters, so
/// this is belt and braces rather than the only guard.
fn unescape_nested(v: &mut serde_json::Value, depth: u32) -> bool {
    use serde_json::Value;
    match v {
        Value::String(s) => {
            if depth == 0 {
                return false;
            }
            let trimmed = s.trim();
            if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
                return false;
            }
            match serde_json::from_str::<Value>(trimmed) {
                Ok(mut inner) if inner.is_object() || inner.is_array() => {
                    unescape_nested(&mut inner, depth - 1);
                    *v = inner;
                    true
                }
                _ => false,
            }
        }
        // `fold`, not `any`: `any` short-circuits, and every element has to be
        // visited so the whole document is unwrapped, not just up to the first
        // hit. Clippy's suggestion would be a bug here.
        #[allow(clippy::unnecessary_fold)]
        Value::Array(a) => a
            .iter_mut()
            .fold(false, |hit, x| unescape_nested(x, depth) || hit),
        #[allow(clippy::unnecessary_fold)]
        Value::Object(o) => o
            .iter_mut()
            .fold(false, |hit, (_, x)| unescape_nested(x, depth) || hit),
        _ => false,
    }
}

fn render(v: &serde_json::Value, indent: usize) -> Result<String> {
    if indent == 0 {
        return Ok(v.to_string());
    }
    let pad = vec![b' '; indent];
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(
        &mut buf,
        serde_json::ser::PrettyFormatter::with_indent(&pad),
    );
    serde::Serialize::serialize(v, &mut ser)?;
    Ok(String::from_utf8(buf)?)
}

pub(crate) fn json_viewer(args: &Args) -> Result<String> {
    let t = &TOOLS[8];
    let mut v = json(args, t, "input")?;
    if flag(args, t, "sortKeys") {
        v = sorted(&v);
    }
    let indent = num(args, t, "indent")?.clamp(0, 16) as usize;
    let pretty = render(&v, indent)?;

    // The second panel only appears when there is something in it. Escaped
    // payloads are the exception, not the rule, and a duplicate panel on every
    // ordinary document would be noise. `\uXXXX` escapes need no work of their
    // own: parsing decodes them and `serde_json` writes non-ASCII literally, so
    // the resolved panel shows the characters.
    let mut unwrapped = v.clone();
    if !unescape_nested(&mut unwrapped, 32) {
        return Ok(pretty);
    }
    Ok([
        crate::section("Formatted", Ok(pretty)),
        crate::section("Escaped JSON resolved", render(&unwrapped, indent)),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "json-viewer",
    name: "JSON prettify and validate",
    description: "Format and validate a JSON document, resolving JSON embedded as escaped strings.",
    category: Category::Data,
    keywords: &[
        "json", "viewer", "prettify", "format", "validate", "escaped", "unescape",
    ],
    fields: &[
        Field::area("input", "JSON"),
        // it-tools' defaults: 3-space indent, keys sorted.
        Field::number("indent", "Indent size", "3"),
        Field::bool("sortKeys", "Sort keys", "true"),
    ],
    kind: Kind::Pure,
    run: json_viewer,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn json_viewer_leaves_plain_documents_unlabelled() {
        // One panel, no header, exactly as before the escaped-JSON panel existed.
        assert_eq!(
            ok("json-viewer", &[("input", r#"{"a":1}"#), ("indent", "0")]),
            r#"{"a":1}"#
        );
    }

    /// PRs #1542/#1539/#1643.
    #[test]
    fn json_viewer_resolves_escaped_json() {
        let out = ok(
            "json-viewer",
            &[("input", r#"{"payload":"{\"a\":1}"}"#), ("indent", "0")],
        );
        assert_eq!(
            out,
            "Formatted:\n{\"payload\":\"{\\\"a\\\":1}\"}\n\n\
             Escaped JSON resolved:\n{\"payload\":{\"a\":1}}"
        );
        // Nested twice, and inside an array.
        let out = ok(
            "json-viewer",
            &[("input", r#"["[1,2]","{\"b\":\"[3]\"}"]"#), ("indent", "0")],
        );
        assert!(
            out.ends_with("Escaped JSON resolved:\n[[1,2],{\"b\":[3]}]"),
            "{out}"
        );
        // Unicode escapes come out readable in the resolved panel.
        let out = ok(
            "json-viewer",
            &[("input", r#"{"p":"{\"a\":\"\\u00e4\"}"}"#), ("indent", "0")],
        );
        assert!(
            out.ends_with("Escaped JSON resolved:\n{\"p\":{\"a\":\"ä\"}}"),
            "{out}"
        );
        // A string that merely looks scalar is not touched, so no second panel.
        assert_eq!(
            ok(
                "json-viewer",
                &[("input", r#"{"a":"null"}"#), ("indent", "0")]
            ),
            r#"{"a":"null"}"#
        );
    }
}
