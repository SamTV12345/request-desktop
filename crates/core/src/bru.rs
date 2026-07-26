//! Bruno `.bru` parser/serializer. Ported from `bruno-lang/v2` (bruToJson /
//! jsonToBru / envToJson). The format is line oriented — blocks are
//! `name {` ... a body ... `}` where `}` sits at column 0 — so a scanner over
//! lines is all it takes.
//!
//! Anything the `Request` model has no field for is kept verbatim in
//! `Request::extra` so a parse -> serialize pass does not quietly delete the
//! user's file. Keys used there: `meta.type`, `meta.tags`, `http.body`,
//! `http.auth`, `auth.mode`, `auth:<kind>`, `auth:oauth2.<field>`, `docs`,
//! `params:path`, `settings`, `vars:pre-request`, `vars:post-response` and
//! `block.<name>` for blocks this parser does not model.
use crate::model::{
    ApiKeyPlacement, Assert, Auth, Body, Collection, Environment, Item, MultipartField, OAuth2,
    Request,
};
use anyhow::{Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// block scanner
// ---------------------------------------------------------------------------

struct Block {
    name: String,
    /// Raw lines between the header and the closing brace, joined with `\n`.
    body: String,
}

/// Splits a `.bru` file into its top level blocks. Errors on anything that is
/// not a block header or on a block that is never closed.
fn split_blocks(src: &str) -> Result<Vec<Block>> {
    let lines: Vec<&str> = src
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        // Ohm's built-in `space` is `\x00..\x20`, so Bruno skips a line made of
        // NULs or other control characters between blocks. Do the same.
        if line.chars().all(|c| c <= '\u{20}' || c.is_whitespace()) {
            i += 1;
            continue;
        }
        let Some((name, open)) = block_header(line) else {
            bail!("line {}: expected a block header, found `{}`", i + 1, line);
        };
        // `color: red` style single line block (environment files).
        if open == ':' {
            out.push(Block {
                name,
                body: line[line.find(':').unwrap() + 1..].trim().to_string(),
            });
            i += 1;
            continue;
        }
        let close = if open == '{' { "}" } else { "]" };
        let mut j = i + 1;
        while j < lines.len() && lines[j].trim_end() != close {
            j += 1;
        }
        if j >= lines.len() {
            bail!("line {}: block `{}` is never closed", i + 1, name);
        }
        out.push(Block {
            name,
            body: lines[i + 1..j].join("\n"),
        });
        i = j + 1;
    }
    Ok(out)
}

/// `meta {` -> ("meta", '{'), `vars:secret [` -> ("vars:secret", '['),
/// `color: red` -> ("color", ':').
fn block_header(line: &str) -> Option<(String, char)> {
    if line.starts_with(' ') || line.starts_with('\t') {
        return None;
    }
    let t = line.trim_end();
    let is_name = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-' | '.'))
    };
    if let Some(rest) = t.strip_suffix('{').or_else(|| t.strip_suffix('[')) {
        let open = t.chars().last().unwrap();
        let name = rest.trim_end();
        if is_name(name) {
            return Some((name.to_string(), open));
        }
        return None;
    }
    let (name, _) = t.split_once(':')?;
    if is_name(name) {
        return Some((name.to_string(), ':'));
    }
    None
}

// ---------------------------------------------------------------------------
// dictionary bodies
// ---------------------------------------------------------------------------

struct Pair {
    /// Still carries a leading `~` when the entry is disabled.
    key: String,
    value: String,
    content_type: Option<String>,
}

impl Pair {
    fn enabled(&self) -> bool {
        !self.key.starts_with('~')
    }
    fn name(&self) -> &str {
        self.key.strip_prefix('~').unwrap_or(&self.key)
    }
}

fn parse_dict(body: &str) -> Vec<Pair> {
    parse_dict_inner(body, false)
}

/// `assert` blocks write their value as `<operator> <value>`, so the multiline
/// `'''` marker can start after the operator token.
fn parse_assert_dict(body: &str) -> Vec<Pair> {
    parse_dict_inner(body, true)
}

fn parse_dict_inner(body: &str, assert: bool) -> Vec<Pair> {
    let lines: Vec<&str> = body.split('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        if t.is_empty() {
            i += 1;
            continue;
        }
        // ponytail: `@description(...)` / `@number` decorators are skipped —
        // the model has nowhere to put per-pair metadata.
        if is_annotation(t) {
            // Only scan forward when the `('''` was *not* closed on this same
            // line — otherwise every entry up to the next `''')` is swallowed.
            if t.split_once("('''")
                .is_some_and(|(_, a)| !a.contains("''')"))
            {
                i += 1;
                while i < lines.len() && !lines[i].trim().ends_with("''')") {
                    i += 1;
                }
            }
            i += 1;
            continue;
        }
        let Some((key, rest)) = split_key(t) else {
            i += 1;
            continue;
        };
        // In an assert the value is `<operator> <value>`, so a `'''` block opens
        // *after* the operator. Peel the operator off and put it back below.
        let (prefix, rest) = match rest.split_once(char::is_whitespace) {
            Some((op, r)) if assert && OPERATORS.contains(&op) => {
                (format!("{op} "), r.trim_start())
            }
            _ => (String::new(), rest),
        };
        let mut value;
        if let Some(after) = rest.strip_prefix("'''") {
            let (v, next) = read_multiline(&lines, i, after);
            value = format!("{prefix}{v}");
            i = next;
        } else if rest.starts_with('[') {
            let mut items = Vec::new();
            i += 1;
            while i < lines.len() && lines[i].trim() != "]" {
                let it = lines[i].trim();
                if !it.is_empty() {
                    items.push(it.trim_end_matches(',').to_string());
                }
                i += 1;
            }
            i += 1;
            value = format!("{prefix}{}", items.join(","));
        } else {
            value = format!("{prefix}{}", rest.trim());
            i += 1;
        }
        let content_type = extract_content_type(&mut value);
        out.push(Pair {
            key,
            value,
            content_type,
        });
    }
    out
}

fn is_annotation(t: &str) -> bool {
    let Some(after) = t.strip_prefix('@') else {
        return false;
    };
    match (after.find('('), after.find(':')) {
        (_, None) => true,
        (Some(p), Some(c)) => p < c,
        (None, Some(_)) => false,
    }
}

/// Splits `key: value`, honouring `"quoted keys"` and the `~` disable marker.
fn split_key(t: &str) -> Option<(String, &str)> {
    let disabled = t.starts_with('~');
    let s = if disabled { &t[1..] } else { t };
    if let Some(inner) = s.strip_prefix('"') {
        let mut key = String::new();
        let mut chars = inner.char_indices();
        let mut end = None;
        while let Some((idx, c)) = chars.next() {
            match c {
                '\\' => {
                    if let Some((_, esc)) = chars.next() {
                        key.push(esc);
                    }
                }
                '"' => {
                    end = Some(idx);
                    break;
                }
                _ => key.push(c),
            }
        }
        // An unterminated quote (or a quoted key with no `:` after it) is not a
        // quoted key at all: Bruno's `key = keychar*` reads it literally, quote
        // included. Falling through keeps the pair instead of dropping it.
        if let Some(rest) = end
            .map(|e| inner[e + 1..].trim_start())
            .and_then(|r| r.strip_prefix(':'))
        {
            return Some((
                if disabled { format!("~{key}") } else { key },
                rest.trim_start(),
            ));
        }
    }
    let (k, v) = t.split_once(':')?;
    Some((k.trim().to_string(), v.trim_start()))
}

/// Reads a `'''` delimited value. `after` is what followed the opening `'''`
/// on the same line. Returns the value plus the next line index.
fn read_multiline(lines: &[&str], start: usize, after: &str) -> (String, usize) {
    if let Some(idx) = after.find("'''") {
        // Opened and closed on the same line.
        let mut v = after[..idx].to_string();
        v.push_str(&after[idx + 3..]);
        return (v.trim().to_string(), start + 1);
    }
    let mut content = Vec::new();
    let mut i = start + 1;
    let mut tail = String::new();
    while i < lines.len() {
        let t = lines[i].trim_start();
        if let Some(rest) = t.strip_prefix("'''") {
            tail = rest.to_string();
            i += 1;
            break;
        }
        // Env and request files both indent multiline content by 4.
        content.push(strip_upto(lines[i], 4));
        i += 1;
    }
    let mut v = content.join("\n").trim().to_string();
    if !tail.trim().is_empty() {
        // Separator only when there is something to separate: an empty block
        // plus a tail would otherwise yield a leading space, which `serialize`
        // writes on one line and the next `parse` trims away again.
        if !v.is_empty() {
            v.push(' ');
        }
        v.push_str(tail.trim());
    }
    (v, i)
}

fn strip_upto(line: &str, n: usize) -> &str {
    let mut cut = 0;
    for c in line.chars().take(n) {
        if c == ' ' {
            cut += 1;
        } else {
            break;
        }
    }
    &line[cut..]
}

/// Pulls a trailing ` @contentType(...)` off a value.
fn extract_content_type(value: &mut String) -> Option<String> {
    let v = value.trim_end();
    if !v.ends_with(')') {
        return None;
    }
    let idx = v.rfind("@contentType(")?;
    let inner = &v[idx + "@contentType(".len()..v.len() - 1];
    if inner.contains(')') {
        return None;
    }
    let ct = inner.trim().to_string();
    *value = v[..idx].trim_end().to_string();
    Some(ct)
}

fn find<'a>(pairs: &'a [Pair], name: &str) -> Option<&'a Pair> {
    pairs.iter().find(|p| p.name() == name)
}

fn val(pairs: &[Pair], name: &str) -> String {
    find(pairs, name)
        .map(|p| p.value.clone())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// text bodies
// ---------------------------------------------------------------------------

/// Strips the two spaces `jsonToBru` adds. Mirrors `outdentString`.
fn outdent(s: &str) -> String {
    s.split('\n')
        .map(|l| strip_upto(l, 2))
        .collect::<Vec<_>>()
        .join("\n")
}

fn indent(s: &str) -> String {
    s.split('\n')
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Mirrors `getValueString`: multiline values get wrapped in `'''`.
fn value_str(v: &str) -> String {
    if v.contains('\n') {
        format!("'''\n{}\n'''", indent(v))
    } else {
        v.to_string()
    }
}

/// `value_str` for a value written inside an already 2-space-indented block
/// header (`meta {`, the verb block), where the first line carries the `key: `.
fn field_str(v: &str) -> String {
    indent(&value_str(v)).trim_start().to_string()
}

/// Mirrors `getKeyString`, keeping the `~` marker outside the quotes.
fn key_str(k: &str) -> String {
    let (prefix, name) = match k.strip_prefix('~') {
        Some(n) => ("~", n),
        None => ("", k),
    };
    // `\` has to be escaped too, and has to force quoting: `split_key` treats it
    // as the escape character, so an unescaped one would come back changed.
    if [':', '"', '{', '}', ' ', '\\']
        .iter()
        .any(|c| name.contains(*c))
    {
        format!(
            "{prefix}\"{}\"",
            name.replace('\\', "\\\\").replace('"', "\\\"")
        )
    } else {
        format!("{prefix}{name}")
    }
}

// ---------------------------------------------------------------------------
// parse
// ---------------------------------------------------------------------------

const VERBS: &[&str] = &[
    "get", "post", "put", "delete", "patch", "head", "options", "trace", "connect",
];

const OPERATORS: &[&str] = &[
    "eq",
    "neq",
    "gt",
    "gte",
    "lt",
    "lte",
    "in",
    "notIn",
    "contains",
    "notContains",
    "length",
    "matches",
    "notMatches",
    "startsWith",
    "endsWith",
    "between",
    "isEmpty",
    "isNotEmpty",
    "isNull",
    "isUndefined",
    "isDefined",
    "isTruthy",
    "isFalsy",
    "isJson",
    "isNumber",
    "isString",
    "isBoolean",
    "isArray",
];

pub fn parse(src: &str) -> Result<Request> {
    let blocks = split_blocks(src)?;
    let mut req = Request {
        seq: 1,
        ..Default::default()
    };
    let mut meta_type = "http".to_string();
    // Body blocks are collected first; which one wins is decided by the
    // `body:` mode declared in the verb block.
    let mut bodies: Vec<(String, Body)> = Vec::new();
    let mut graphql_query: Option<String> = None;
    let mut graphql_vars: Option<String> = None;
    let mut auths: Vec<(String, Vec<Pair>)> = Vec::new();
    // Raw text of every body block, so the ones the declared mode does not
    // select can be kept in `extra` instead of vanishing.
    let mut body_src: Vec<(String, String)> = Vec::new();

    for b in &blocks {
        let body = &b.body;
        if b.name.starts_with("body:") {
            body_src.push((b.name.clone(), outdent(body)));
        }
        match b.name.as_str() {
            "meta" => {
                let pairs = parse_dict(body);
                req.name = val(&pairs, "name");
                // Bruno: `if (!meta.type) meta.type = 'http'` — an empty value
                // falls back to the default rather than being kept.
                if let Some(t) = find(&pairs, "type").filter(|t| !t.value.is_empty()) {
                    meta_type = t.value.clone();
                }
                if let Some(s) = find(&pairs, "seq") {
                    req.seq = s.value.trim().parse().unwrap_or(1);
                }
                if let Some(t) = find(&pairs, "tags") {
                    req.extra.insert("meta.tags".into(), t.value.clone());
                }
            }
            v if VERBS.contains(&v) || v == "http" => {
                let pairs = parse_dict(body);
                req.method = if v == "http" {
                    val(&pairs, "method")
                } else {
                    v.to_string()
                }
                .to_uppercase();
                req.url = val(&pairs, "url");
                if let Some(p) = find(&pairs, "body") {
                    req.extra.insert("http.body".into(), p.value.clone());
                }
                if let Some(p) = find(&pairs, "auth") {
                    req.extra.insert("http.auth".into(), p.value.clone());
                }
            }
            "headers" => {
                for p in parse_dict(body) {
                    req.headers.push((p.key, p.value));
                }
            }
            "params:query" | "query" => {
                for p in parse_dict(body) {
                    req.query.push((p.key, p.value));
                }
            }
            "params:path" | "settings" | "vars:pre-request" | "vars:post-response" => {
                req.extra.insert(b.name.clone(), outdent(body));
            }
            "auth" => {
                let pairs = parse_dict(body);
                req.extra.insert("auth.mode".into(), val(&pairs, "mode"));
            }
            n if n.starts_with("auth:") => {
                auths.push((n["auth:".len()..].to_string(), parse_dict(body)));
            }
            "body:json" => bodies.push(("json".into(), Body::Json(outdent(body)))),
            "body:text" => bodies.push(("text".into(), Body::Text(outdent(body)))),
            "body:xml" => bodies.push(("xml".into(), Body::Xml(outdent(body)))),
            "body:sparql" => bodies.push(("sparql".into(), Body::Sparql(outdent(body)))),
            "body:graphql" => graphql_query = Some(outdent(body)),
            "body:graphql:vars" => graphql_vars = Some(outdent(body)),
            "body:form-urlencoded" => bodies.push((
                "formUrlEncoded".into(),
                Body::Form(
                    parse_dict(body)
                        .into_iter()
                        .map(|p| (p.key, p.value))
                        .collect(),
                ),
            )),
            "body:multipart-form" => bodies.push((
                "multipartForm".into(),
                Body::MultipartForm(parse_dict(body).into_iter().map(multipart_field).collect()),
            )),
            "body:file" => {
                let pairs = parse_dict(body);
                let p = pairs.iter().find(|p| p.enabled()).or(pairs.first());
                let (path, content_type) = match p {
                    Some(p) => (
                        file_ref(&p.value),
                        p.content_type.clone().unwrap_or_default(),
                    ),
                    None => (String::new(), String::new()),
                };
                bodies.push(("file".into(), Body::File { path, content_type }));
            }
            "script:pre-request" => req.pre_request_script = Some(outdent(body)),
            "script:post-response" => req.post_response_script = Some(outdent(body)),
            "tests" => req.tests = Some(outdent(body)),
            "docs" => {
                req.extra.insert("docs".into(), outdent(body));
            }
            "assert" | "asserts" => {
                for p in parse_assert_dict(body) {
                    let (operator, value) = split_operator(&p.value);
                    req.asserts.push(Assert {
                        expr: p.name().to_string(),
                        operator,
                        value,
                        enabled: p.enabled(),
                    });
                }
            }
            // Unknown block: keep it verbatim so nothing is lost.
            other => {
                req.extra.insert(format!("block.{other}"), outdent(body));
            }
        }
    }

    req.extra.insert("meta.type".into(), meta_type);

    if graphql_query.is_some() || graphql_vars.is_some() {
        bodies.push((
            "graphql".into(),
            Body::GraphQl {
                query: graphql_query.unwrap_or_default(),
                variables: graphql_vars.unwrap_or_default(),
            },
        ));
    }
    // Only the mode the verb block declares is promoted to `Request::body`.
    // With no verb block there is no mode, so — like Bruno — nothing is.
    let mode = req.extra.get("http.body").cloned().unwrap_or_default();
    let mut chosen = "";
    if let Some(idx) = bodies.iter().position(|(m, _)| *m == mode) {
        req.body = bodies.swap_remove(idx).1;
        chosen = mode_blocks(&mode);
    }
    for (name, text) in body_src {
        if !chosen.split(' ').any(|n| n == name) {
            req.extra.insert(format!("block.{name}"), text);
        }
    }

    req.auth = take_auth(&mut req, &auths);
    // Auth blocks the model cannot express stay verbatim.
    for (kind, pairs) in &auths {
        if !auth_consumed(&req, kind) {
            req.extra.insert(
                format!("auth:{kind}"),
                pairs
                    .iter()
                    .map(|p| format!("{}: {}", key_str(&p.key), value_str(&p.value)))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
    }
    Ok(req)
}

/// The block name(s), space separated, a `body:` mode is read from.
fn mode_blocks(mode: &str) -> &'static str {
    match mode {
        "json" => "body:json",
        "text" => "body:text",
        "xml" => "body:xml",
        "sparql" => "body:sparql",
        "graphql" => "body:graphql body:graphql:vars",
        "formUrlEncoded" => "body:form-urlencoded",
        "multipartForm" => "body:multipart-form",
        "file" => "body:file",
        _ => "",
    }
}

fn auth_mode(req: &Request) -> String {
    req.extra.get("http.auth").cloned().unwrap_or_default()
}

fn auth_consumed(req: &Request, kind: &str) -> bool {
    auth_mode(req) == kind && !matches!(req.auth, Auth::None)
}

fn take_auth(req: &mut Request, auths: &[(String, Vec<Pair>)]) -> Auth {
    let mode = auth_mode(req);
    if mode == "inherit" {
        return Auth::Inherit;
    }
    let Some((_, pairs)) = auths.iter().find(|(k, _)| *k == mode) else {
        return Auth::None;
    };
    match mode.as_str() {
        "basic" => Auth::Basic {
            username: val(pairs, "username"),
            password: val(pairs, "password"),
        },
        "bearer" => Auth::Bearer {
            token: val(pairs, "token"),
        },
        "apikey" => Auth::ApiKey {
            key: val(pairs, "key"),
            value: val(pairs, "value"),
            placement: if val(pairs, "placement") == "queryparams" {
                ApiKeyPlacement::Query
            } else {
                ApiKeyPlacement::Header
            },
        },
        "oauth2" => {
            // The model covers seven fields; the rest ride along in `extra`.
            const KNOWN: &[&str] = &[
                "grant_type",
                "access_token_url",
                "authorization_url",
                "client_id",
                "client_secret",
                "scope",
                "callback_url",
            ];
            for p in pairs.iter().filter(|p| !KNOWN.contains(&p.name())) {
                req.extra
                    .insert(format!("auth:oauth2.{}", p.key), p.value.clone());
            }
            Auth::OAuth2(Box::new(OAuth2 {
                grant_type: val(pairs, "grant_type"),
                access_token_url: val(pairs, "access_token_url"),
                authorization_url: val(pairs, "authorization_url"),
                client_id: val(pairs, "client_id"),
                client_secret: val(pairs, "client_secret"),
                scope: val(pairs, "scope"),
                callback_url: val(pairs, "callback_url"),
            }))
        }
        _ => Auth::None,
    }
}

fn multipart_field(p: Pair) -> MultipartField {
    let file_path = if p.value.starts_with("@file(") && p.value.ends_with(')') {
        Some(file_ref(&p.value))
    } else {
        None
    };
    MultipartField {
        name: p.key,
        value: if file_path.is_some() {
            String::new()
        } else {
            p.value
        },
        file_path,
        content_type: p.content_type,
    }
}

fn file_ref(v: &str) -> String {
    v.strip_prefix("@file(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(v)
        .trim()
        .to_string()
}

fn split_operator(v: &str) -> (String, String) {
    let (head, rest) = v.split_once(char::is_whitespace).unwrap_or((v, ""));
    if OPERATORS.contains(&head) {
        (head.to_string(), rest.trim().to_string())
    } else {
        ("eq".to_string(), v.to_string())
    }
}

// ---------------------------------------------------------------------------
// serialize
// ---------------------------------------------------------------------------

pub fn serialize(req: &Request) -> String {
    let mut s = String::new();

    s.push_str("meta {\n");
    s.push_str(&format!("  name: {}\n", field_str(&req.name)));
    s.push_str(&format!(
        "  type: {}\n",
        field_str(req.extra.get("meta.type").map_or("http", |t| t.as_str()))
    ));
    s.push_str(&format!("  seq: {}\n", req.seq));
    if let Some(tags) = req.extra.get("meta.tags") {
        s.push_str("  tags: [\n");
        for tag in tags.split(',').filter(|t| !t.is_empty()) {
            s.push_str(&format!("    {tag}\n"));
        }
        s.push_str("  ]\n");
    }
    s.push_str("}\n\n");

    if !req.method.is_empty() {
        let verb = req.method.to_lowercase();
        if VERBS.contains(&verb.as_str()) {
            s.push_str(&format!("{verb} {{\n"));
        } else {
            s.push_str(&format!("http {{\n  method: {verb}\n"));
        }
        s.push_str(&format!("  url: {}\n", field_str(&req.url)));
        if let Some(m) = req.extra.get("http.body") {
            s.push_str(&format!("  body: {}\n", field_str(m)));
        }
        if let Some(m) = req.extra.get("http.auth") {
            s.push_str(&format!("  auth: {}\n", field_str(m)));
        }
        s.push_str("}\n\n");
    }

    push_pairs(&mut s, "params:query", &req.query);
    if let Some(raw) = req.extra.get("params:path") {
        push_raw(&mut s, "params:path", raw);
    }
    push_pairs(&mut s, "headers", &req.headers);

    if let Some(mode) = req.extra.get("auth.mode") {
        s.push_str(&format!("auth {{\n  mode: {mode}\n}}\n\n"));
    }
    match &req.auth {
        Auth::None | Auth::Inherit => {}
        Auth::Basic { username, password } => push_raw(
            &mut s,
            "auth:basic",
            &format!(
                "username: {}\npassword: {}",
                value_str(username),
                value_str(password)
            ),
        ),
        Auth::Bearer { token } => push_raw(
            &mut s,
            "auth:bearer",
            &format!("token: {}", value_str(token)),
        ),
        Auth::ApiKey {
            key,
            value,
            placement,
        } => push_raw(
            &mut s,
            "auth:apikey",
            &format!(
                "key: {}\nvalue: {}\nplacement: {}",
                value_str(key),
                value_str(value),
                match placement {
                    ApiKeyPlacement::Header => "header",
                    ApiKeyPlacement::Query => "queryparams",
                }
            ),
        ),
        Auth::OAuth2(o) => {
            let mut lines = vec![
                format!("grant_type: {}", value_str(&o.grant_type)),
                format!("access_token_url: {}", value_str(&o.access_token_url)),
                format!("authorization_url: {}", value_str(&o.authorization_url)),
                format!("client_id: {}", value_str(&o.client_id)),
                format!("client_secret: {}", value_str(&o.client_secret)),
                format!("scope: {}", value_str(&o.scope)),
                format!("callback_url: {}", value_str(&o.callback_url)),
            ];
            for (k, v) in &req.extra {
                if let Some(key) = k.strip_prefix("auth:oauth2.") {
                    lines.push(format!("{}: {}", key_str(key), value_str(v)));
                }
            }
            push_raw(&mut s, "auth:oauth2", &lines.join("\n"));
        }
    }
    for (k, v) in &req.extra {
        if k.starts_with("auth:") && !k.contains('.') {
            push_raw(&mut s, k, v);
        }
    }

    match &req.body {
        Body::None => {}
        Body::Json(t) => push_text(&mut s, "body:json", t),
        Body::Text(t) => push_text(&mut s, "body:text", t),
        Body::Xml(t) => push_text(&mut s, "body:xml", t),
        Body::Sparql(t) => push_text(&mut s, "body:sparql", t),
        Body::Form(items) => push_pairs(&mut s, "body:form-urlencoded", items),
        Body::MultipartForm(fields) => {
            let lines: Vec<String> = fields
                .iter()
                .map(|f| {
                    let value = match &f.file_path {
                        Some(p) => format!("@file({p})"),
                        None => value_str(&f.value),
                    };
                    let ct = match &f.content_type {
                        Some(c) if !c.is_empty() => format!(" @contentType({c})"),
                        _ => String::new(),
                    };
                    format!("{}: {value}{ct}", key_str(&f.name))
                })
                .collect();
            push_raw(&mut s, "body:multipart-form", &lines.join("\n"));
        }
        Body::GraphQl { query, variables } => {
            push_text(&mut s, "body:graphql", query);
            push_text(&mut s, "body:graphql:vars", variables);
        }
        Body::File { path, content_type } => {
            let ct = if content_type.is_empty() {
                String::new()
            } else {
                format!(" @contentType({content_type})")
            };
            push_raw(
                &mut s,
                "body:file",
                &format!("file: {}{ct}", value_str(&format!("@file({path})"))),
            );
        }
    }

    for key in ["vars:pre-request", "vars:post-response"] {
        if let Some(raw) = req.extra.get(key) {
            push_raw(&mut s, key, raw);
        }
    }

    if !req.asserts.is_empty() {
        let lines: Vec<String> = req
            .asserts
            .iter()
            .map(|a| {
                let prefix = if a.enabled { "" } else { "~" };
                let v = format!("{} {}", a.operator, value_str(&a.value));
                format!(
                    "{}: {}",
                    key_str(&format!("{prefix}{}", a.expr)),
                    v.trim_end()
                )
            })
            .collect();
        push_raw(&mut s, "assert", &lines.join("\n"));
    }

    if let Some(t) = &req.pre_request_script {
        push_text(&mut s, "script:pre-request", t);
    }
    if let Some(t) = &req.post_response_script {
        push_text(&mut s, "script:post-response", t);
    }
    if let Some(t) = &req.tests {
        push_text(&mut s, "tests", t);
    }
    if let Some(raw) = req.extra.get("settings") {
        push_raw(&mut s, "settings", raw);
    }
    if let Some(t) = req.extra.get("docs") {
        push_text(&mut s, "docs", t);
    }
    for (k, v) in &req.extra {
        if let Some(name) = k.strip_prefix("block.") {
            push_raw(&mut s, name, v);
        }
    }

    s.trim_end_matches('\n').to_string() + "\n"
}

fn push_raw(s: &mut String, name: &str, body: &str) {
    s.push_str(&format!("{name} {{\n{}\n}}\n\n", indent(body)));
}

/// A text block the user wrote is written back even when empty — dropping it
/// deletes a block from their file.
fn push_text(s: &mut String, name: &str, text: &str) {
    push_raw(s, name, text);
}

fn push_pairs(s: &mut String, name: &str, items: &[(String, String)]) {
    if items.is_empty() {
        return;
    }
    let lines: Vec<String> = items
        .iter()
        .map(|(k, v)| format!("{}: {}", key_str(k), value_str(v)))
        .collect();
    push_raw(s, name, &lines.join("\n"));
}

// ---------------------------------------------------------------------------
// environments
// ---------------------------------------------------------------------------

pub fn parse_environment(src: &str) -> Result<Environment> {
    let mut env = Environment::default();
    for b in split_blocks(src)? {
        match b.name.as_str() {
            "vars" => {
                for p in parse_dict(&b.body) {
                    env.vars.push((p.key, p.value, false));
                }
            }
            "vars:secret" => {
                for name in b.body.split([',', '\n']) {
                    let n = name.trim();
                    if !n.is_empty() {
                        env.vars.push((n.to_string(), String::new(), true));
                    }
                }
            }
            // ponytail: `color:` and `vars:externalsecrets:*` have no home in
            // the model; drop them.
            _ => {}
        }
    }
    Ok(env)
}

// ---------------------------------------------------------------------------
// collection loading
// ---------------------------------------------------------------------------

pub fn load_collection(root: &Path) -> Result<Collection> {
    let name = fs::read_to_string(root.join("bruno.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v["name"].as_str().map(str::to_string))
        .unwrap_or_else(|| {
            root.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        });

    let mut environments = Vec::new();
    let env_dir = root.join("environments");
    if env_dir.is_dir() {
        let mut files: Vec<PathBuf> = fs::read_dir(&env_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "bru"))
            .collect();
        files.sort();
        for f in files {
            let src = fs::read_to_string(&f)?;
            if let Ok(mut env) = parse_environment(&src) {
                env.name = f
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                environments.push(env);
            }
        }
    }

    Ok(Collection {
        name,
        root: root.to_path_buf(),
        items: load_dir(root, true)?,
        environments,
    })
}

fn load_dir(dir: &Path, is_root: bool) -> Result<Vec<Item>> {
    let mut folders: Vec<(String, Vec<Item>)> = Vec::new();
    let mut requests: Vec<(u32, String, PathBuf, Request)> = Vec::new();

    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if path.is_dir() {
            if is_root && file_name == "environments" {
                continue;
            }
            folders.push((file_name, load_dir(&path, false)?));
        } else if path.extension().is_some_and(|e| e == "bru") {
            // folder.bru / collection.bru hold folder and collection level
            // settings, not requests.
            if file_name == "folder.bru" || file_name == "collection.bru" {
                continue;
            }
            // ponytail: a file that fails to parse is skipped rather than
            // failing the whole collection load.
            if let Ok(req) = fs::read_to_string(&path)
                .map_err(anyhow::Error::from)
                .and_then(|s| parse(&s))
            {
                requests.push((req.seq, req.name.clone(), path, req));
            }
        }
    }

    folders.sort_by(|a, b| a.0.cmp(&b.0));
    requests.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));

    let mut items: Vec<Item> = folders
        .into_iter()
        .map(|(name, items)| Item::Folder { name, items })
        .collect();
    items.extend(
        requests
            .into_iter()
            .map(|(_, _, path, request)| Item::Request {
                path,
                request: Box::new(request),
            }),
    );
    Ok(items)
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const PING: &str = include_str!("../tests/fixtures/ping.bru");
    const BASIC_AUTH: &str = include_str!("../tests/fixtures/basic_auth.bru");
    const ECHO_MULTIPART: &str = include_str!("../tests/fixtures/echo_multipart.bru");
    const TYPED_VARS: &str = include_str!("../tests/fixtures/typed_vars.bru");
    const GRAPHQL: &str = include_str!("../tests/fixtures/graphql_mutation.bru");
    const BODY_FILE: &str = include_str!("../tests/fixtures/body_file.bru");
    const FORM: &str = include_str!("../tests/fixtures/form_urlencoded.bru");
    const COLLECTION: &str = include_str!("../tests/fixtures/collection.bru");
    const ENV_LOCAL: &str = include_str!("../tests/fixtures/env_local.bru");
    const PATH_PARAMS: &str = include_str!("../tests/fixtures/path_params.bru");
    const ASSERTS: &str = include_str!("../tests/fixtures/asserts.bru");
    const BODY_XML: &str = include_str!("../tests/fixtures/body_xml.bru");

    const FIXTURES: &[(&str, &str)] = &[
        ("ping", PING),
        ("basic_auth", BASIC_AUTH),
        ("echo_multipart", ECHO_MULTIPART),
        ("typed_vars", TYPED_VARS),
        ("graphql", GRAPHQL),
        ("body_file", BODY_FILE),
        ("form", FORM),
        ("collection", COLLECTION),
        ("path_params", PATH_PARAMS),
        ("asserts", ASSERTS),
        ("body_xml", BODY_XML),
    ];

    #[test]
    fn round_trips_real_fixtures() {
        for (name, src) in FIXTURES {
            let a = parse(src).unwrap_or_else(|e| panic!("{name}: {e}"));
            let text = serialize(&a);
            let b = parse(&text).unwrap_or_else(|e| panic!("{name} reparse: {e}\n{text}"));
            assert_eq!(a, b, "{name} did not round-trip. serialized:\n{text}");
        }
    }

    #[test]
    fn parses_meta_and_verb() {
        let r = parse(PING).unwrap();
        assert_eq!(r.name, "ping");
        assert_eq!(r.method, "GET");
        assert_eq!(r.url, "{{host}}/ping");
        assert_eq!(r.seq, 1);
        assert_eq!(r.extra["meta.type"], "http");
        assert_eq!(r.extra["http.body"], "none");
        assert!(
            r.pre_request_script
                .as_deref()
                .unwrap()
                .contains("stopExecution")
        );
    }

    #[test]
    fn parses_headers_and_query() {
        let r = parse(
            "get {\n  url: http://x/y\n}\n\nheaders {\n  a: 1\n  ~b: 2\n}\n\nparams:query {\n  q: v\n}\n",
        )
        .unwrap();
        assert_eq!(
            r.headers,
            vec![("a".into(), "1".into()), ("~b".into(), "2".into())]
        );
        assert_eq!(r.query, vec![("q".into(), "v".into())]);
    }

    #[test]
    fn parses_auth_basic() {
        let r = parse(BASIC_AUTH).unwrap();
        assert_eq!(
            r.auth,
            Auth::Basic {
                username: "bruno".into(),
                password: "{{basic_auth_password}}".into()
            }
        );
    }

    #[test]
    fn parses_auth_bearer_apikey_oauth2_and_inherit() {
        let bearer =
            parse("get {\n  url: u\n  auth: bearer\n}\n\nauth:bearer {\n  token: t\n}\n").unwrap();
        assert_eq!(bearer.auth, Auth::Bearer { token: "t".into() });

        let apikey = parse(
            "get {\n  url: u\n  auth: apikey\n}\n\nauth:apikey {\n  key: k\n  value: v\n  placement: queryparams\n}\n",
        )
        .unwrap();
        assert_eq!(
            apikey.auth,
            Auth::ApiKey {
                key: "k".into(),
                value: "v".into(),
                placement: ApiKeyPlacement::Query
            }
        );

        let oauth = parse(
            "get {\n  url: u\n  auth: oauth2\n}\n\nauth:oauth2 {\n  grant_type: client_credentials\n  access_token_url: http://t\n  client_id: ci\n  client_secret: cs\n  scope: s\n  pkce: false\n}\n",
        )
        .unwrap();
        match &oauth.auth {
            Auth::OAuth2(o) => {
                assert_eq!(o.grant_type, "client_credentials");
                assert_eq!(o.access_token_url, "http://t");
            }
            other => panic!("expected oauth2, got {other:?}"),
        }
        // Fields the model lacks are kept in extra.
        assert_eq!(oauth.extra["auth:oauth2.pkce"], "false");
        assert_eq!(parse(&serialize(&oauth)).unwrap(), oauth);

        let inherit = parse("get {\n  url: u\n  auth: inherit\n}\n").unwrap();
        assert_eq!(inherit.auth, Auth::Inherit);
    }

    #[test]
    fn keeps_unrepresentable_auth_verbatim() {
        let r = parse(
            "get {\n  url: u\n  auth: digest\n}\n\nauth:digest {\n  username: u\n  password: p\n}\n",
        )
        .unwrap();
        assert_eq!(r.auth, Auth::None);
        assert_eq!(r.extra["auth:digest"], "username: u\npassword: p");
        assert_eq!(parse(&serialize(&r)).unwrap(), r);
    }

    #[test]
    fn parses_text_bodies() {
        assert_eq!(
            parse(BODY_XML).unwrap().body,
            Body::Xml("<hello><world>bruno</world></hello>".into())
        );
        assert_eq!(
            parse("post {\n  url: u\n  body: text\n}\n\nbody:text {\n  hi\n  there\n}\n")
                .unwrap()
                .body,
            Body::Text("hi\nthere".into())
        );
        assert_eq!(
            parse("post {\n  url: u\n  body: sparql\n}\n\nbody:sparql {\n  SELECT *\n}\n")
                .unwrap()
                .body,
            Body::Sparql("SELECT *".into())
        );
        match parse(ASSERTS).unwrap().body {
            Body::Json(j) => assert!(j.starts_with("{\n  \"type\": \"application/json\",")),
            other => panic!("expected json body, got {other:?}"),
        }
    }

    #[test]
    fn parses_graphql_body() {
        match parse(GRAPHQL).unwrap().body {
            Body::GraphQl { query, variables } => {
                assert!(query.starts_with("mutation create($id: String!) {"));
                assert_eq!(variables, "{\n  \"id\":\"1\"\n}");
            }
            other => panic!("expected graphql, got {other:?}"),
        }
    }

    #[test]
    fn parses_form_and_multipart_and_file_bodies() {
        assert_eq!(
            parse(FORM).unwrap().body,
            Body::Form(vec![
                ("tags".into(), "frontend".into()),
                ("tags".into(), "api".into()),
                ("user".into(), "john".into()),
            ])
        );

        let m = parse(ECHO_MULTIPART).unwrap();
        match &m.body {
            Body::MultipartForm(fields) => {
                assert_eq!(fields[0].name, "foo");
                assert_eq!(fields[0].value, "{\"bar\":\"baz\"}");
                assert_eq!(
                    fields[0].content_type.as_deref(),
                    Some("application/json--test")
                );
                // multiline `'''` value with a trailing @contentType
                assert_eq!(fields[1].name, "multiline");
                assert_eq!(fields[1].value, "\"multiline-test\"");
                assert_eq!(
                    fields[1].content_type.as_deref(),
                    Some("application/json--multiline--test")
                );
                let file = fields.last().unwrap();
                assert_eq!(file.file_path.as_deref(), Some("bruno.png"));
            }
            other => panic!("expected multipart, got {other:?}"),
        }

        assert_eq!(
            parse(BODY_FILE).unwrap().body,
            Body::File {
                path: "file.json".into(),
                content_type: "application/json".into()
            }
        );
    }

    #[test]
    fn parses_asserts_with_operators_and_disabled_entries() {
        let r = parse(
            "get {\n  url: u\n}\n\nassert {\n  res.status: eq 200\n  res.body.msg: hello world\n  ~res.body.x: isEmpty\n}\n",
        )
        .unwrap();
        assert_eq!(r.asserts.len(), 3);
        assert_eq!(r.asserts[0].expr, "res.status");
        assert_eq!(r.asserts[0].operator, "eq");
        assert_eq!(r.asserts[0].value, "200");
        assert!(r.asserts[0].enabled);
        // No leading operator token means an implicit `eq`.
        assert_eq!(r.asserts[1].operator, "eq");
        assert_eq!(r.asserts[1].value, "hello world");
        assert_eq!(r.asserts[2].operator, "isEmpty");
        assert_eq!(r.asserts[2].value, "");
        assert!(!r.asserts[2].enabled);
        assert_eq!(parse(&serialize(&r)).unwrap(), r);
    }

    #[test]
    fn disabled_entries_keep_their_marker() {
        let src = "get {\n  url: u\n  body: formUrlEncoded\n}\n\nheaders {\n  ~x-off: 1\n}\n\nbody:form-urlencoded {\n  ~a: 1\n  b: 2\n}\n";
        let r = parse(src).unwrap();
        assert_eq!(r.headers[0].0, "~x-off");
        assert_eq!(
            r.body,
            Body::Form(vec![("~a".into(), "1".into()), ("b".into(), "2".into())])
        );
        let text = serialize(&r);
        assert!(text.contains("  ~x-off: 1"), "{text}");
        assert_eq!(parse(&text).unwrap(), r);
    }

    #[test]
    fn keeps_scripts_tests_docs_vars_and_settings() {
        let r = parse(TYPED_VARS).unwrap();
        assert!(r.extra["vars:pre-request"].contains("@number\nreq_num: 42"));
        assert!(
            r.tests
                .as_deref()
                .unwrap()
                .contains("test(\"collection vars")
        );
        assert!(
            r.pre_request_script
                .as_deref()
                .unwrap()
                .contains("bru.setVar")
        );

        let c = parse(COLLECTION).unwrap();
        assert_eq!(c.extra["auth.mode"], "bearer");
        assert!(c.extra["docs"].contains("bruno-testbench"));

        let p = parse(PATH_PARAMS).unwrap();
        assert_eq!(p.extra["params:path"], "pathParam: ping");

        let s = parse("get {\n  url: u\n}\n\nsettings {\n  encodeUrl: true\n  timeout: 0\n}\n")
            .unwrap();
        assert_eq!(s.extra["settings"], "encodeUrl: true\ntimeout: 0");
    }

    /// Branches around the ones the bug fixes touched, none of which had a unit
    /// test reaching them.
    #[test]
    fn less_travelled_branches() {
        // `tags: [` list values, and the custom-verb `http { method: }` form.
        let r = parse(
            "meta {\n  name: n\n  seq: 2\n  tags: [\n    a,\n    b\n  ]\n}\n\nhttp {\n  method: purge\n  url: u\n}\n",
        )
        .unwrap();
        assert_eq!(r.extra["meta.tags"], "a,b");
        assert_eq!(r.method, "PURGE");
        let text = serialize(&r);
        assert!(text.contains("  tags: [\n    a\n    b\n  ]"), "{text}");
        assert!(text.contains("http {\n  method: purge"), "{text}");
        assert_eq!(parse(&text).unwrap(), r);

        // `auth { mode: }` plus a declared kind with no block: no panic, no auth,
        // and the declaration survives.
        let r = parse("get {\n  url: u\n  auth: awsv4\n}\n\nauth {\n  mode: none\n}\n").unwrap();
        assert_eq!(r.auth, Auth::None);
        assert_eq!(r.extra["auth.mode"], "none");
        assert_eq!(parse(&serialize(&r)).unwrap(), r);

        // `body:file` with the first entry disabled falls back to it; an empty
        // block yields empty strings rather than panicking.
        let r = parse(
            "post {\n  url: u\n  body: file\n}\n\nbody:file {\n  ~a: @file(a.json)\n  b: @file(b.json) @contentType(application/json)\n}\n",
        )
        .unwrap();
        assert_eq!(
            r.body,
            Body::File {
                path: "b.json".into(),
                content_type: "application/json".into()
            }
        );
        assert_eq!(
            parse("post {\n  url: u\n  body: file\n}\n\nbody:file {\n}\n")
                .unwrap()
                .body,
            Body::File {
                path: String::new(),
                content_type: String::new()
            }
        );

        // graphql with only a vars block, and with only a query block.
        assert_eq!(
            parse("post {\n  url: u\n  body: graphql\n}\n\nbody:graphql:vars {\n  {}\n}\n")
                .unwrap()
                .body,
            Body::GraphQl {
                query: String::new(),
                variables: "{}".into()
            }
        );

        // `@contentType(` is only stripped when it really closes the value.
        let r = parse("get {\n  url: u\n}\n\nheaders {\n  a: x @contentType(a)b\n  b: y @contentType(a))\n}\n")
            .unwrap();
        assert_eq!(r.headers[0].1, "x @contentType(a)b");
        assert_eq!(r.headers[1].1, "y @contentType(a))");

        // An assert value that is not a known operator gets the implicit `eq`.
        let r = parse("get {\n  url: u\n}\n\nassert {\n  a: not-an-op x\n}\n").unwrap();
        assert_eq!(r.asserts[0].operator, "eq");
        assert_eq!(r.asserts[0].value, "not-an-op x");

        // Environments: single-line `color:` block and both `vars:secret` forms
        // are handled; unknown blocks are dropped rather than erroring.
        let env =
            parse_environment("vars {\n  a: 1\n}\n\nvars:secret [\n  s1,\n  s2\n]\n\ncolor: red\n")
                .unwrap();
        assert_eq!(env.vars[0], ("a".into(), "1".into(), false));
        assert!(env.vars.iter().any(|(n, _, s)| n == "s1" && *s));
        assert!(env.vars.iter().any(|(n, _, s)| n == "s2" && *s));

        // A collection directory with no bruno.json takes its name from the dir,
        // and a missing `environments/` is simply no environments.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/coll/sub");
        let c = load_collection(&root).unwrap();
        assert_eq!(c.name, "sub");
        assert!(c.environments.is_empty());
        assert_eq!(c.items.len(), 1);
    }

    #[test]
    fn keeps_unknown_blocks() {
        let r = parse("get {\n  url: u\n}\n\napp {\n  enabled: true\n}\n").unwrap();
        assert_eq!(r.extra["block.app"], "enabled: true");
        assert!(serialize(&r).contains("app {\n  enabled: true\n}"));
    }

    #[test]
    fn handles_quoted_keys_and_multiline_values() {
        let r = parse(
            "get {\n  url: u\n}\n\nheaders {\n  \"x y\": 1\n  m: '''\n    a\n    b\n  '''\n}\n",
        )
        .unwrap();
        assert_eq!(r.headers[0], ("x y".into(), "1".into()));
        assert_eq!(r.headers[1], ("m".into(), "a\nb".into()));
        let text = serialize(&r);
        assert!(text.contains("\"x y\": 1"), "{text}");
        assert_eq!(parse(&text).unwrap(), r);
    }

    #[test]
    fn parses_environment_vars_and_secrets() {
        let env = parse_environment(ENV_LOCAL).unwrap();
        assert_eq!(
            env.vars[0],
            ("host".into(), "http://localhost:8080".into(), false)
        );
        assert!(
            env.vars
                .iter()
                .any(|(n, v, s)| n == "typed_env_obj" && v == "{\"scope\":\"env\"}" && !s)
        );
        assert!(
            env.vars
                .iter()
                .any(|(n, _, secret)| n == "github_client_secret" && *secret)
        );
    }

    #[test]
    fn malformed_input_is_an_error() {
        assert!(parse("this is not a bru file").is_err());
        assert!(parse("get {\n  url: u\n").is_err());
        assert!(parse_environment("vars {\n  a: 1\n").is_err());
    }

    /// One regression test per fixed bug. Each was silent data loss.
    #[test]
    fn regressions() {
        let rt = |src: &str| {
            let a = parse(src).unwrap_or_else(|e| panic!("{src}: {e}"));
            let text = serialize(&a);
            assert_eq!(
                parse(&text).unwrap_or_else(|e| panic!("reparse: {e}\n{text}")),
                a,
                "did not round-trip. serialized:\n{text}"
            );
            a
        };

        // 1: a `@description('''x''')` closed on its own line must not swallow
        // the entries after it.
        let r =
            rt("get {\n  url: u\n}\n\nheaders {\n  @description('''one''')\n  a: 1\n  b: 2\n}\n");
        assert_eq!(
            r.headers,
            vec![("a".into(), "1".into()), ("b".into(), "2".into())]
        );

        // 2: an assert value can open a `'''` block after the operator.
        let r = rt("get {\n  url: u\n}\n\nassert {\n  res.body: eq '''\n    x\n    y\n  '''\n}\n");
        assert_eq!(r.asserts[0].operator, "eq");
        assert_eq!(r.asserts[0].value, "x\ny");

        // 3: an assert expression containing `:` is quoted on the way out.
        let r = rt("get {\n  url: u\n}\n\nassert {\n  \"res.headers['x:y']\": eq 1\n}\n");
        assert_eq!(r.asserts[0].expr, "res.headers['x:y']");
        assert_eq!(r.asserts[0].value, "1");

        // 4: an empty text block the user wrote survives a save.
        let r = rt("post {\n  url: u\n  body: json\n}\n\nbody:json {\n}\n\ndocs {\n}\n");
        assert_eq!(r.body, Body::Json(String::new()));
        assert_eq!(r.extra["docs"], "");

        // 5: with no declared body mode nothing is promoted, but the block is
        // preserved rather than dropped.
        let r = rt("meta {\n  name: n\n}\n\nbody:json {\n  {\"a\": 1}\n}\n");
        assert_eq!(r.body, Body::None);
        assert_eq!(r.extra["block.body:json"], "{\"a\": 1}");
        // Same for `body: none` plus a stray block.
        let r = rt("get {\n  url: u\n  body: none\n}\n\nbody:json {\n  {}\n}\n");
        assert_eq!(r.body, Body::None);
        assert_eq!(r.extra["block.body:json"], "{}");

        // Found by the fuzzer while fixing the above: `key_str` escaped `"` but
        // not the `\` that `split_key` reads as an escape character, so a key
        // containing a backslash came back mangled.
        let r = rt("get {\n  url: u\n}\n\nheaders {\n  \"\\\\ a\": 1\n}\n");
        assert_eq!(r.headers, vec![("\\ a".into(), "1".into())]);

        // Also from the fuzzer: an empty `'''` block with trailing text must not
        // gain a leading space, which a single-line write cannot carry.
        let r = rt("meta {\n  name: '''\n  ''' x: y\n}\n");
        assert_eq!(r.name, "x: y");

        // 6: an unterminated quoted key keeps the pair, quote included.
        let r = parse("get {\n  url: u\n}\n\nheaders {\n  \"abc: 1\n  \"def\": 2\n}\n").unwrap();
        assert_eq!(
            r.headers,
            vec![("\"abc".into(), "1".into()), ("def".into(), "2".into())]
        );

        // 7: an empty `type:` falls back to `http`.
        assert_eq!(
            parse("meta {\n  name: x\n  type: \n}\n").unwrap().extra["meta.type"],
            "http"
        );

        // 8: a NUL-only line between blocks is whitespace, not a bad header.
        let r = parse("get {\n  url: http://x\0y\n}\n\nheaders {\n  a\0b: 1\n}\n\0").unwrap();
        assert_eq!(r.url, "http://x\0y");
        assert_eq!(r.headers, vec![("a\0b".into(), "1".into())]);

        // 9: a multiline value that landed in `extra` is re-emitted as a `'''`
        // block instead of being truncated at the first newline.
        let r = rt("meta {\n  type: '''\n    a\n    b\n  '''\n}\n");
        assert_eq!(r.extra["meta.type"], "a\nb");
        for src in [
            "get {\n  url: '''\n    a\n    b\n  '''\n  body: '''\n    j\n    k\n  '''\n}\n",
            "get {\n  url: u\n  auth: basic\n}\n\nauth:basic {\n  username: '''\n    a\n    b\n  '''\n  password: p\n}\n",
            "get {\n  url: u\n  auth: bearer\n}\n\nauth:bearer {\n  token: '''\n    a\n    b\n  '''\n}\n",
            "get {\n  url: u\n  auth: apikey\n}\n\nauth:apikey {\n  key: k\n  value: '''\n    a\n    b\n  '''\n}\n",
            "post {\n  url: u\n  body: file\n}\n\nbody:file {\n  file: '''\n    a\n    b\n  ''' @contentType(text/plain)\n}\n",
        ] {
            rt(src);
        }
    }

    #[test]
    fn loads_a_collection_from_disk() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/coll");
        let c = load_collection(&root).unwrap();
        assert_eq!(c.name, "mini");
        assert_eq!(c.environments.len(), 1);
        assert_eq!(c.environments[0].name, "Local");
        assert_eq!(c.environments[0].vars.len(), 2);

        // folders first, then requests ordered by seq
        assert_eq!(c.items.len(), 3);
        match &c.items[0] {
            Item::Folder { name, items } => {
                assert_eq!(name, "sub");
                assert_eq!(items.len(), 1);
                match &items[0] {
                    Item::Request { request, .. } => assert_eq!(request.name, "nested"),
                    other => panic!("expected request, got {other:?}"),
                }
            }
            other => panic!("expected folder, got {other:?}"),
        }
        let names: Vec<&str> = c.items[1..]
            .iter()
            .map(|i| match i {
                Item::Request { request, .. } => request.name.as_str(),
                Item::Folder { name, .. } => name.as_str(),
            })
            .collect();
        assert_eq!(names, vec!["first", "second"]);
    }
}
