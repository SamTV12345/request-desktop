//! `curl-to-http` — it-tools PRs #1171 / #1811, bent towards this app.
//!
//! Upstream turns a cURL command into JavaScript. Here the useful target is a
//! request you can put in the editor next door, so the output is the IntelliJ
//! `.http` syntax the app already opens, sends and stores.
//!
//! The input is whatever the browser's "Copy as cURL" put on the clipboard, so
//! the tokeniser has to survive all four shapes of it: POSIX single quotes,
//! `$'…'` ANSI-C quoting for anything binary, Windows `^` line continuations
//! with doubled quotes, and backslash-newline.

use crate::*;
use anyhow::{Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use std::fmt::Write as _;

/// Split a command line into words the way a shell would.
///
/// Deliberately not a shell: no expansion, no globbing, no pipelines. It only
/// has to undo quoting, because everything inside a copied cURL is a literal.
pub(crate) fn tokenize(input: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut has_word = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // Line continuations: `\` + newline (POSIX) and `^` + newline (cmd).
            '\\' | '^' if matches!(chars.peek(), Some('\n' | '\r')) => {
                while matches!(chars.peek(), Some('\n' | '\r')) {
                    chars.next();
                }
            }
            '\\' => {
                if let Some(next) = chars.next() {
                    word.push(next);
                    has_word = true;
                }
            }
            '\'' => {
                has_word = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    word.push(c);
                }
            }
            '"' => {
                has_word = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        // Inside double quotes a backslash only escapes these.
                        '\\' if matches!(chars.peek(), Some('"' | '\\' | '$' | '`')) => {
                            word.push(chars.next().expect("peeked"));
                        }
                        _ => word.push(c),
                    }
                }
            }
            // `$'…'`: ANSI-C quoting, what Chrome emits for a body with newlines.
            '$' if chars.peek() == Some(&'\'') => {
                chars.next();
                has_word = true;
                while let Some(c) = chars.next() {
                    match c {
                        '\'' => break,
                        '\\' => match chars.next() {
                            Some('n') => word.push('\n'),
                            Some('r') => word.push('\r'),
                            Some('t') => word.push('\t'),
                            Some(other) => word.push(other),
                            None => break,
                        },
                        _ => word.push(c),
                    }
                }
            }
            c if c.is_whitespace() => {
                if has_word {
                    out.push(std::mem::take(&mut word));
                    has_word = false;
                }
            }
            c => {
                word.push(c);
                has_word = true;
            }
        }
    }
    if has_word {
        out.push(word);
    }
    if out.is_empty() {
        bail!("nothing to convert");
    }
    Ok(out)
}

#[derive(Default)]
pub(crate) struct Request {
    pub(crate) method: Option<String>,
    pub(crate) url: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) data: Vec<String>,
    pub(crate) form: Vec<String>,
    pub(crate) query_only: bool,
    pub(crate) notes: Vec<String>,
}

impl Request {
    fn header(&mut self, name: &str, value: impl Into<String>) {
        // A flag never wins over an explicit -H the user also passed.
        if !self
            .headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(name))
        {
            self.headers.push((name.to_string(), value.into()));
        }
    }
}

/// Flags that change nothing about the request as written down: transport,
/// output and verbosity. Listed so they are *reported* rather than ignored.
#[rustfmt::skip]
const TRANSPORT_FLAGS: &[(&str, &str)] = &[
    ("-L", "follows redirects"), ("--location", "follows redirects"),
    ("-k", "skips certificate verification"), ("--insecure", "skips certificate verification"),
    ("-s", "silent"), ("--silent", "silent"), ("-v", "verbose"), ("--verbose", "verbose"),
    ("-i", "prints response headers"), ("--include", "prints response headers"),
    ("-f", "fails on HTTP errors"), ("--fail", "fails on HTTP errors"),
    ("-g", "globbing off"), ("--globoff", "globbing off"),
    ("--http1.1", "forces HTTP/1.1"), ("--http2", "forces HTTP/2"), ("--http3", "forces HTTP/3"),
];

pub(crate) fn parse_curl(tokens: &[String]) -> Result<Request> {
    let mut r = Request::default();
    let mut it = tokens.iter().peekable();
    if it.peek().is_some_and(|t| t.ends_with("curl")) {
        it.next();
    }
    let next = |it: &mut std::iter::Peekable<std::slice::Iter<String>>, flag: &str| {
        it.next()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
    };
    while let Some(token) = it.next() {
        // `--header=value` is as legal as `--header value`.
        let (flag, inline) = match token.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (token.clone(), None),
        };
        let value = |it: &mut std::iter::Peekable<std::slice::Iter<String>>| match &inline {
            Some(v) => Ok(v.clone()),
            None => next(it, &flag),
        };
        match flag.as_str() {
            "-X" | "--request" => r.method = Some(value(&mut it)?.to_uppercase()),
            "-H" | "--header" => {
                let raw = value(&mut it)?;
                match raw.split_once(':') {
                    Some((n, v)) => r.headers.push((n.trim().into(), v.trim().into())),
                    None => r.notes.push(format!("dropped malformed header {raw:?}")),
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-ascii" => {
                r.data.push(value(&mut it)?);
            }
            "--data-urlencode" => {
                let raw = value(&mut it)?;
                r.data.push(match raw.split_once('=') {
                    Some((n, v)) => {
                        format!("{n}={}", utf8_percent_encode(v, NON_ALPHANUMERIC))
                    }
                    None => utf8_percent_encode(&raw, NON_ALPHANUMERIC).to_string(),
                });
            }
            "--json" => {
                r.data.push(value(&mut it)?);
                r.header("Content-Type", "application/json");
                r.header("Accept", "application/json");
            }
            "-F" | "--form" | "--form-string" => r.form.push(value(&mut it)?),
            "-G" | "--get" => r.query_only = true,
            "-I" | "--head" => r.method = Some("HEAD".into()),
            "-u" | "--user" => {
                let creds = value(&mut it)?;
                r.header(
                    "Authorization",
                    format!("Basic {}", STANDARD.encode(&creds)),
                );
            }
            "-A" | "--user-agent" => {
                let v = value(&mut it)?;
                r.header("User-Agent", v);
            }
            "-e" | "--referer" => {
                let v = value(&mut it)?;
                r.header("Referer", v);
            }
            "-b" | "--cookie" => {
                let v = value(&mut it)?;
                r.header("Cookie", v);
            }
            "--compressed" => r.header("Accept-Encoding", "gzip, deflate, br"),
            "--url" => r.url = value(&mut it)?,
            "-o" | "--output" | "-O" | "--remote-name" | "--max-time" | "-m"
            | "--connect-timeout" | "--retry" | "--proxy" | "-x" | "--cacert" | "--cert"
            | "--key" | "--resolve" => {
                let v = if flag == "-O" || flag == "--remote-name" {
                    String::new()
                } else {
                    value(&mut it)?
                };
                r.notes
                    .push(format!("dropped `{flag} {v}`").trim_end().into());
            }
            other => {
                if let Some((_, what)) = TRANSPORT_FLAGS.iter().find(|(f, _)| *f == other) {
                    r.notes
                        .push(format!("`{other}` ({what}) has no place in a .http file"));
                } else if other.starts_with('-') && other.len() > 1 {
                    r.notes.push(format!("unknown flag `{other}`, ignored"));
                } else if r.url.is_empty() {
                    r.url = other.to_string();
                } else {
                    r.notes.push(format!("ignored extra argument {other:?}"));
                }
            }
        }
    }
    if r.url.is_empty() {
        bail!("no URL in that command");
    }
    if !r.url.contains("://") {
        r.url = format!("https://{}", r.url);
    }
    Ok(r)
}

/// `-G` means "send the data as a query string" even though `-d` was used.
fn fold_query(r: &mut Request) {
    if !r.query_only || r.data.is_empty() {
        return;
    }
    let joined = std::mem::take(&mut r.data).join("&");
    let sep = if r.url.contains('?') { '&' } else { '?' };
    r.url = format!("{}{sep}{joined}", r.url);
}

const BOUNDARY: &str = "WebAppBoundary";

fn render(r: &Request) -> String {
    let body = r.data.join("&");
    let method = r.method.clone().unwrap_or_else(|| {
        if body.is_empty() && r.form.is_empty() {
            "GET".into()
        } else {
            "POST".into()
        }
    });
    let mut out = format!("{method} {}\n", r.url);
    let content_type = r
        .headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.clone());
    for (name, value) in &r.headers {
        let _ = writeln!(out, "{name}: {value}");
    }
    if !r.form.is_empty() {
        if content_type.is_none() {
            let _ = writeln!(
                out,
                "Content-Type: multipart/form-data; boundary={BOUNDARY}"
            );
        }
        out.push('\n');
        for part in &r.form {
            let (name, value) = part.split_once('=').unwrap_or((part.as_str(), ""));
            let _ = writeln!(out, "--{BOUNDARY}");
            // `@file` and `<file` are cURL's "read from disk"; `.http` spells
            // that `< ./file`, so the request stays runnable after the import.
            match value.strip_prefix('@').or_else(|| value.strip_prefix('<')) {
                Some(path) => {
                    let _ = writeln!(
                        out,
                        "Content-Disposition: form-data; name=\"{name}\"; filename=\"{}\"\n\n< {path}",
                        path.rsplit(['/', '\\']).next().unwrap_or(path)
                    );
                }
                None => {
                    let _ = writeln!(
                        out,
                        "Content-Disposition: form-data; name=\"{name}\"\n\n{value}"
                    );
                }
            }
        }
        let _ = writeln!(out, "--{BOUNDARY}--");
        return out;
    }
    if body.is_empty() {
        return out;
    }
    if content_type.is_none() {
        // cURL's own default for -d, and the source of many a 415.
        let _ = writeln!(out, "Content-Type: application/x-www-form-urlencoded");
    }
    out.push('\n');
    // A JSON body arrives on one line; the editor is nicer with it laid out.
    let pretty = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .filter(|_| content_type.is_none_or(|c| c.contains("json")))
        .and_then(|v| serde_json::to_string_pretty(&v).ok());
    out.push_str(&pretty.unwrap_or(body));
    out.push('\n');
    out
}

fn curl_to_http(a: &Args) -> Result<String> {
    let input = s(a, "curl", DEFAULT_CURL);
    if input.trim().is_empty() {
        bail!("paste a cURL command — the browser's \"Copy as cURL\" works");
    }
    let mut req = parse_curl(&tokenize(input)?)?;
    fold_query(&mut req);
    let notes = if req.notes.is_empty() {
        "nothing was dropped\n".to_string()
    } else {
        req.notes.join("\n") + "\n"
    };
    Ok([
        section("Request (.http)", Ok(render(&req))),
        section("Notes", Ok(notes)),
    ]
    .join("\n\n"))
}

const DEFAULT_CURL: &str = "curl 'https://api.example.com/v1/things?page=2' \\\n  \
     -H 'Authorization: Bearer eyJhbGciOi' \\\n  \
     -H 'Content-Type: application/json' \\\n  \
     --data-raw '{\"name\":\"a thing\",\"tags\":[\"one\",\"two\"]}' \\\n  \
     --compressed";

pub static TOOL: Tool = Tool {
    id: "curl-to-http",
    name: "cURL to .http request",
    description: "Paste what the browser copied, get a request you can drop into an .http file.",
    category: Category::Web,
    keywords: &[
        "curl", "http", "request", "import", "devtools", "convert", "rest", "har",
    ],
    fields: &[Field::area("curl", "cURL command").with_default(DEFAULT_CURL)],
    kind: Kind::Pure,
    run: curl_to_http,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn the_shape_chrome_copies() {
        let out = ok("curl-to-http", &[]);
        assert!(
            out.contains("POST https://api.example.com/v1/things?page=2"),
            "{out}"
        );
        assert!(out.contains("Authorization: Bearer eyJhbGciOi"), "{out}");
        assert!(out.contains("Accept-Encoding: gzip, deflate, br"), "{out}");
        // A JSON body gets laid out rather than left on one line.
        assert!(out.contains("\"name\": \"a thing\""), "{out}");
        assert!(out.contains("nothing was dropped"), "{out}");
    }

    #[test]
    fn quoting_survives_every_dialect() {
        // POSIX single quotes, ANSI-C quoting, Windows doubled quotes and `^`.
        assert_eq!(
            tokenize(r#"curl 'a b' "c\"d" $'e\nf'"#).unwrap(),
            ["curl", "a b", "c\"d", "e\nf"]
        );
        assert_eq!(
            tokenize("curl ^\n  \"http://x\" ^\n  -H \"A: b\"").unwrap(),
            ["curl", "http://x", "-H", "A: b"]
        );
        // Backslash-newline is the usual POSIX continuation.
        assert_eq!(
            tokenize("curl \\\n  http://x").unwrap(),
            ["curl", "http://x"]
        );
        assert!(tokenize("   ").is_err());
    }

    #[test]
    fn methods_bodies_and_forms() {
        // -d without -X is a POST, and gets cURL's own default content type.
        let out = ok("curl-to-http", &[("curl", "curl http://x -d a=1 -d b=2")]);
        assert!(out.contains("POST http://x\n"), "{out}");
        assert!(
            out.contains("Content-Type: application/x-www-form-urlencoded"),
            "{out}"
        );
        assert!(out.contains("a=1&b=2"), "{out}");
        // -G moves the same data into the query string, and back to GET.
        let get = ok(
            "curl-to-http",
            &[("curl", "curl -G http://x -d a=1 -d b=2")],
        );
        assert!(get.contains("GET http://x?a=1&b=2"), "{get}");
        assert!(!get.contains("Content-Type"), "{get}");
        // -I is HEAD, -u becomes a real Basic header.
        let head = ok("curl-to-http", &[("curl", "curl -I -u me:pw http://x")]);
        assert!(head.contains("HEAD http://x"), "{head}");
        assert!(head.contains("Authorization: Basic bWU6cHc="), "{head}");
        // A file upload keeps working, in .http's own include syntax.
        let form = ok(
            "curl-to-http",
            &[("curl", "curl http://x -F file=@/tmp/a.png -F kind=avatar")],
        );
        assert!(
            form.contains("multipart/form-data; boundary=WebAppBoundary"),
            "{form}"
        );
        assert!(
            form.contains("filename=\"a.png\"") && form.contains("< /tmp/a.png"),
            "{form}"
        );
        assert!(form.contains("name=\"kind\""), "{form}");
        assert!(
            form.contains("--WebAppBoundary--"),
            "the closing boundary: {form}"
        );
    }

    #[test]
    fn what_cannot_be_carried_over_is_said_out_loud() {
        let out = ok(
            "curl-to-http",
            &[("curl", "curl -k -L --max-time 30 -o out.json http://x")],
        );
        assert!(
            out.contains("`-k` (skips certificate verification)"),
            "{out}"
        );
        assert!(out.contains("`-L` (follows redirects)"), "{out}");
        assert!(out.contains("dropped `--max-time 30`"), "{out}");
        assert!(out.contains("dropped `-o out.json`"), "{out}");
        // --header=value and --data-urlencode are the awkward spellings.
        let eq = ok(
            "curl-to-http",
            &[(
                "curl",
                "curl http://x --header='A: b' --data-urlencode 'q=a b&c'",
            )],
        );
        assert!(eq.contains("A: b"), "{eq}");
        assert!(eq.contains("q=a%20b%26c"), "{eq}");
        assert!(
            run("curl-to-http", &[("curl", "curl -X POST")]).is_err(),
            "no URL"
        );
        // A bare host gets https, the way every copied command means it.
        assert!(
            ok("curl-to-http", &[("curl", "curl example.com/x")])
                .contains("GET https://example.com/x")
        );
    }
}
