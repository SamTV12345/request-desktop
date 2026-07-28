//! `har-sanitizer`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, section};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

// it-tools PR #1531. The one tool here that is properly at home in an HTTP
// client: a HAR you can attach to a bug report without leaking your session.

#[rustfmt::skip]
pub(crate) const SECRET_HEADERS: &[&str] = &[
    "authorization", "proxy-authorization", "cookie", "set-cookie", "x-api-key",
    "x-auth-token", "api-key", "x-csrf-token", "x-xsrf-token", "authentication",
];
#[rustfmt::skip]
pub(crate) const SECRET_PARAMS: &[&str] = &[
    "token", "access_token", "refresh_token", "id_token", "api_key", "apikey",
    "key", "password", "passwd", "secret", "code", "client_secret", "signature",
    "sig", "auth",
];

pub(crate) const REDACTED: &str = "[REDACTED]";

pub(crate) fn redact_name_value_list(list: Option<&mut Value>, names: &[&str], count: &mut usize) {
    let Some(Value::Array(items)) = list else {
        return;
    };
    for item in items {
        let Some(obj) = item.as_object_mut() else {
            continue;
        };
        let hit = obj
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|n| names.contains(&n.to_lowercase().as_str()));
        if hit && obj.get("value").is_some_and(|v| v != REDACTED) {
            obj.insert("value".into(), json!(REDACTED));
            *count += 1;
        }
    }
}

pub(crate) fn har_sanitizer(a: &Args) -> Result<String> {
    let raw = s(a, "input", "").trim();
    if raw.is_empty() {
        bail!("paste a HAR file (the JSON your browser's network tab exports)");
    }
    let mut har: Value = serde_json::from_str(raw).context("not valid JSON")?;
    let drop_bodies = b(a, "dropBodies", true);

    let entries = har
        .get_mut("log")
        .and_then(|l| l.get_mut("entries"))
        .and_then(Value::as_array_mut)
        .context("no `log.entries` — this does not look like a HAR file")?;

    let (mut headers, mut cookies, mut params, mut bodies) = (0, 0, 0, 0);
    for entry in entries.iter_mut() {
        for side in ["request", "response"] {
            let Some(msg) = entry.get_mut(side).and_then(Value::as_object_mut) else {
                continue;
            };
            redact_name_value_list(msg.get_mut("headers"), SECRET_HEADERS, &mut headers);
            // Every cookie is a credential; the names are not a fixed list.
            if let Some(Value::Array(items)) = msg.get_mut("cookies") {
                for item in items {
                    if let Some(obj) = item.as_object_mut()
                        && obj.get("value").is_some_and(|v| v != REDACTED)
                    {
                        obj.insert("value".into(), json!(REDACTED));
                        cookies += 1;
                    }
                }
            }
            redact_name_value_list(msg.get_mut("queryString"), SECRET_PARAMS, &mut params);
            if let Some(post) = msg.get_mut("postData").and_then(Value::as_object_mut) {
                redact_name_value_list(post.get_mut("params"), SECRET_PARAMS, &mut params);
                if drop_bodies && post.contains_key("text") {
                    post.insert("text".into(), json!(REDACTED));
                    bodies += 1;
                }
            }
            if drop_bodies
                && let Some(content) = msg.get_mut("content").and_then(Value::as_object_mut)
                && content.contains_key("text")
            {
                content.insert("text".into(), json!(REDACTED));
                bodies += 1;
            }
        }
    }

    Ok([
        section(
            "Redacted",
            Ok(format!(
                "{headers} header(s), {cookies} cookie(s), {params} parameter(s), {bodies} body/bodies\n\
                 across {} entries\n",
                entries.len()
            )),
        ),
        section("Sanitised HAR", serde_json::to_string_pretty(&har).map_err(Into::into)),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "har-sanitizer",
    name: "HAR sanitiser",
    description: "Strip cookies, auth headers and token parameters out of a HAR before sharing it.",
    category: Category::Data,
    keywords: &[
        "har", "network", "sanitize", "redact", "secret", "share", "devtools",
    ],
    fields: &[
        Field::area("input", "HAR file"),
        Field::bool("dropBodies", "Drop request and response bodies", "true"),
    ],
    kind: Kind::Pure,
    run: har_sanitizer,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;
    #[test]
    fn har_sanitizer_redacts_both_directions_and_counts_them() {
        let har = r#"{"log":{"entries":[{
            "request":{"headers":[{"name":"Authorization","value":"Bearer abc"},
                                  {"name":"Accept","value":"application/json"}],
                       "cookies":[{"name":"sid","value":"s3cret"}],
                       "queryString":[{"name":"api_key","value":"k"},{"name":"page","value":"2"}],
                       "postData":{"text":"{\"a\":1}"}},
            "response":{"headers":[{"name":"Set-Cookie","value":"sid=s3cret"}],
                        "content":{"text":"body"}}}]}}"#;
        let out = run("har-sanitizer", &[("input", har)]);
        assert!(!out.contains("Bearer abc"), "{out}");
        assert!(!out.contains("s3cret"), "{out}");
        assert!(out.contains("application/json"), "harmless headers survive");
        assert!(out.contains("\"page\""), "harmless params survive");
        assert!(
            out.contains("2 header(s), 1 cookie(s), 1 parameter(s)"),
            "{out}"
        );
    }
}
