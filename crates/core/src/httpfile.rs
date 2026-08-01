//! IntelliJ / VS Code REST-client `.http` files. One file holds many requests
//! separated by `###`.
//!
//! `{{var}}` placeholders are kept verbatim — `interp` resolves them later.
use crate::model::{Body, MultipartField, Request};
use anyhow::{Result, bail};

const METHODS: &[&str] = &[
    "GET",
    "POST",
    "PUT",
    "PATCH",
    "DELETE",
    "HEAD",
    "OPTIONS",
    "TRACE",
    "CONNECT",
    "GRAPHQL",
    "WEBSOCKET",
];

/// `extra` keys this parser uses. Kept as consts so serialize can't drift.
const K_VERSION: &str = "http_version";
const K_RESPONSE_REF: &str = "response_ref";
const K_POST_SCRIPT_FILE: &str = "post_response_script_file";
const K_PRE_SCRIPT_FILE: &str = "pre_request_script_file";
const K_BODY_FILE_ENC: &str = "body_file_encoding";
/// `>> file` / `>>! file`: JetBrains *response redirect* — write the response
/// body to a file. A distinct feature from `> handler.js`, so a distinct key.
const K_REDIRECT: &str = "response_redirect";
const K_REDIRECT_FORCE: &str = "response_redirect_force";
/// Indices of `query` entries that were written `k=` (explicit `=`, empty
/// value) rather than bare `k`. The model cannot tell the two apart, and the
/// difference is visible on the wire, so it is preserved here.
const K_QUERY_EQ: &str = "query_empty_equals";
/// The `@name = value` declarations that lexically precede this request, stored
/// verbatim (one `@name = value` per line) so `serialize` cannot lose them.
const K_FILE_VARS: &str = "file_vars";

/// In-file `@name = value` declarations, in declaration order.
pub type Decls = Vec<(String, String)>;

/// One in-progress multipart part: headers, body lines, and whether the blank
/// line separating the two has been seen.
type Part<'a> = (Vec<(String, String)>, Vec<&'a str>, bool);

pub fn parse(src: &str) -> Result<Vec<Request>> {
    Ok(parse_with_vars(src)?.0)
}

/// Like `parse`, but also returns the file's `@name = value` declarations in
/// declaration order. Values may themselves contain `{{refs}}`.
pub fn parse_with_vars(src: &str) -> Result<(Vec<Request>, Decls)> {
    // A UTF-8 BOM is invisible to the user and every Windows editor writes one;
    // it is not part of the first line's content.
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let mut out = Vec::new();
    let mut vars: Decls = Vec::new();
    let mut attached = 0usize;
    for (sep_name, lines, base) in split_blocks(src) {
        if let Some(mut r) = parse_block(&lines, sep_name, base, out.len(), &mut vars)? {
            // Declarations seen since the last request belong to this one, so a
            // parse -> serialize cycle can put them back.
            if attached < vars.len() {
                let decls: Vec<String> = vars[attached..]
                    .iter()
                    .map(|(k, v)| format!("@{k} = {v}"))
                    .collect();
                r.extra.insert(K_FILE_VARS.into(), decls.join("\n"));
                attached = vars.len();
            }
            r.seq = out.len() as u32;
            out.push(r);
        }
    }
    Ok((out, vars))
}

/// Split on `###` lines. The text on the separator names the request *below* it.
/// The third element is the 1-based source line number of `lines[0]`, so every
/// error can say where it happened.
fn split_blocks(src: &str) -> Vec<(Option<String>, Vec<&str>, usize)> {
    let mut blocks: Vec<(Option<String>, Vec<&str>, usize)> = vec![(None, Vec::new(), 1)];
    for (n, line) in src.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("###") {
            let name = t.trim_start_matches('#').trim();
            let name = if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            };
            blocks.push((name, Vec::new(), n + 2));
        } else {
            blocks.last_mut().unwrap().1.push(line);
        }
    }
    blocks
}

/// `line 12 of request "login"` — appended to every error so a defect in a
/// 10 000-request file is one jump away instead of a text search.
fn loc(base: usize, i: usize, idx: usize, name: &Option<String>) -> String {
    let n = base + i;
    match name {
        Some(nm) => format!("line {n} of request {nm:?}"),
        None => format!("line {n} of request #{idx}"),
    }
}

fn parse_block(
    lines: &[&str],
    sep_name: Option<String>,
    base: usize,
    idx: usize,
    vars: &mut Decls,
) -> Result<Option<Request>> {
    let mut r = Request::default();
    let mut name = sep_name;
    let mut i = 0usize;

    // --- leading comments, directives and the pre-request handler ---
    while i < lines.len() {
        let t = lines[i].trim();
        if t.is_empty() {
            i += 1;
        } else if is_inline_script(t, '<') {
            let (script, next) = read_inline_script(lines, i, base)?;
            r.pre_request_script = Some(script);
            i = next;
        } else if is_comment(t) {
            apply_comment(t, &mut name, &mut r);
            i += 1;
        } else if t.starts_with("< ") || t == "<" {
            // A pre-request *script file*, which is what serialize writes for
            // K_PRE_SCRIPT_FILE. Without this the line was read as the URL.
            let path = t[1..].trim();
            if path.is_empty() {
                bail!(
                    "`<` needs a file path at {}: write `< ./pre-request.js`",
                    loc(base, i, idx, &name)
                );
            }
            r.extra.insert(K_PRE_SCRIPT_FILE.into(), path.to_string());
            i += 1;
        } else if let Some(decl) = t.strip_prefix('@') {
            // `@name` in this position is always a variable declaration; if it
            // is malformed, say so *here* instead of folding it into the URL and
            // blaming the next line.
            let Some((k, v)) = parse_var_decl(t) else {
                bail!(
                    "malformed variable declaration `@{}` at {}: expected `@name = value`",
                    decl.split_whitespace().next().unwrap_or(""),
                    loc(base, i, idx, &name)
                );
            };
            vars.push((k, v));
            i += 1;
        } else {
            break;
        }
    }
    if i >= lines.len() {
        return Ok(None); // comments-only block
    }

    // --- request line ---
    let req_line = i;
    let first = lines[i].trim();
    if is_header_line(first) {
        bail!(
            "expected a request line, got header-ish {first:?} at {}",
            loc(base, i, idx, &name)
        );
    }
    let mut toks: Vec<&str> = first.split_whitespace().collect();
    if METHODS.contains(&toks[0]) {
        r.method = toks.remove(0).to_string();
    } else if toks.len() >= 2 && is_method_token(toks[0]) {
        // Custom / WebDAV verbs (PURGE, LOCK, MKCOL, PROPFIND, REPORT, …) are
        // passed through verbatim. A single token is always the URL, because a
        // method-less request line is legal.
        r.method = toks.remove(0).to_string();
    } else {
        r.method = "GET".into();
    }
    if toks.len() >= 2
        && let Some(v) = toks.pop_if(|t| t.starts_with("HTTP/"))
    {
        r.extra.insert(K_VERSION.into(), v.to_string());
    }
    if toks.is_empty() {
        bail!(
            "request line {first:?} has no URL at {}",
            loc(base, i, idx, &name)
        );
    }
    let mut url = toks.join(" ");
    i += 1;

    // --- URL continuation lines (indented, or starting with ? / &) ---
    while i < lines.len() {
        let raw = lines[i];
        let t = raw.trim();
        if t.is_empty() || is_comment(t) || is_trailer(t) {
            break;
        }
        let continues = t.starts_with('?')
            || t.starts_with('&')
            || (raw.starts_with([' ', '\t']) && !is_header_line(t));
        if !continues {
            break;
        }
        url.push_str(t);
        i += 1;
    }
    // A URL may not contain raw whitespace: the request line is whitespace
    // separated, so a space folded into the URL is silently eaten on re-parse.
    // Percent-encode instead, which keeps `parse(serialize(x)) == parse(x)`.
    let url = encode_url_whitespace(&url);
    let (url_base, query, empty_eq) = split_query(&url);
    if url_base.is_empty() && query.is_empty() {
        bail!(
            "request line {first:?} has no URL at {}",
            loc(base, req_line, idx, &name)
        );
    }
    r.url = url_base;
    r.query = query;
    if !empty_eq.is_empty() {
        let list: Vec<String> = empty_eq.iter().map(usize::to_string).collect();
        r.extra.insert(K_QUERY_EQ.into(), list.join(","));
    }

    // --- headers ---
    while i < lines.len() {
        let raw = lines[i];
        let t = raw.trim();
        if t.is_empty() {
            i += 1;
            break;
        }
        if is_trailer(t) {
            break;
        }
        if is_comment(t) {
            apply_comment(t, &mut name, &mut r);
            i += 1;
            continue;
        }
        if let Some((k, v)) = t.split_once(':')
            && is_header_line(t)
        {
            r.headers.push((k.trim().to_string(), v.trim().to_string()));
            i += 1;
            continue;
        }
        if raw.starts_with([' ', '\t'])
            && let Some(last) = r.headers.last_mut()
        {
            last.1.push(' ');
            last.1.push_str(t);
            i += 1;
            continue;
        }
        bail!(
            "malformed header line {t:?} at {}",
            loc(base, i, idx, &name)
        );
    }

    // --- body + trailers ---
    let mut body_lines: Vec<&str> = Vec::new();
    let mut body_file: Option<(String, Option<String>)> = None;
    // In a multipart body `< ./file` belongs to a part, not to the whole request.
    let multipart = header(&r.headers, "content-type").is_some_and(|c| {
        c.trim_start()
            .to_ascii_lowercase()
            .starts_with("multipart/")
    });
    while i < lines.len() {
        let t = lines[i].trim();
        if let Some(rest) = t.strip_prefix("<>") {
            r.extra
                .insert(K_RESPONSE_REF.into(), rest.trim().to_string());
            i += 1;
        } else if is_inline_script(t, '>') {
            let (script, next) = read_inline_script(lines, i, base)?;
            r.post_response_script = Some(script);
            i = next;
        } else if is_inline_script(t, '<') {
            let (script, next) = read_inline_script(lines, i, base)?;
            r.pre_request_script = Some(script);
            i = next;
        } else if let Some(rest) = t.strip_prefix(">>") {
            // Response redirect: `>> file` keeps existing files, `>>! file`
            // overwrites. Not a handler script — its own keys.
            let (key, path) = match rest.strip_prefix('!') {
                Some(p) => (K_REDIRECT_FORCE, p.trim()),
                None => (K_REDIRECT, rest.trim()),
            };
            if path.is_empty() {
                bail!(
                    "empty response redirect target at {}: `>>` needs a file path",
                    loc(base, i, idx, &name)
                );
            }
            r.extra.insert(key.into(), path.to_string());
            i += 1;
        } else if let Some(rest) = t.strip_prefix('>') {
            let path = rest.trim();
            if path.is_empty() {
                bail!(
                    "empty response handler reference at {}: `>` needs a path to a .js file",
                    loc(base, i, idx, &name)
                );
            }
            r.extra.insert(K_POST_SCRIPT_FILE.into(), path.to_string());
            i += 1;
        } else if multipart {
            body_lines.push(lines[i]);
            i += 1;
        } else if let Some(rest) = t.strip_prefix("<@") {
            // `<@ encoding ./file` — encoding is optional.
            let mut parts = rest.split_whitespace();
            let (a, b) = (parts.next(), parts.next());
            match (a, b) {
                (Some(enc), Some(path)) => body_file = Some((path.into(), Some(enc.into()))),
                (Some(path), None) => body_file = Some((path.into(), None)),
                _ => bail!(
                    "`<@` needs a file path at {}: write `<@ ./body.json` or \
                     `<@ latin1 ./body.json`",
                    loc(base, i, idx, &name)
                ),
            }
            i += 1;
        } else if t.starts_with("< ") || t == "<" {
            let path = t[1..].trim();
            if path.is_empty() {
                bail!(
                    "`<` needs a file path at {}: write `< ./body.json`",
                    loc(base, i, idx, &name)
                );
            }
            body_file = Some((path.to_string(), None));
            i += 1;
        } else {
            body_lines.push(lines[i]);
            i += 1;
        }
    }
    while body_lines.first().is_some_and(|l| l.trim().is_empty()) {
        body_lines.remove(0);
    }
    while body_lines.last().is_some_and(|l| l.trim().is_empty()) {
        body_lines.pop();
    }
    let text = body_lines.join("\n");

    let ctype = header(&r.headers, "content-type").unwrap_or_default();
    if let Some((path, enc)) = body_file {
        if let Some(e) = enc {
            r.extra.insert(K_BODY_FILE_ENC.into(), e);
        }
        r.body = Body::File {
            path,
            content_type: ctype.clone(),
        };
    } else {
        let where_ = loc(base, req_line, idx, &name);
        r.body = classify_body(&text, &ctype, &r.method, &where_)?;
    }

    r.name = name.unwrap_or_else(|| r.url.clone());
    Ok(Some(r))
}

fn classify_body(text: &str, ctype: &str, method: &str, where_: &str) -> Result<Body> {
    if text.trim().is_empty() {
        return Ok(Body::None);
    }
    let ct = ctype.to_ascii_lowercase();
    if ct.starts_with("multipart/") {
        // not `ct`: the boundary is case-sensitive
        return parse_multipart(text, ctype, where_);
    }
    if ct.contains("graphql") || method == "GRAPHQL" {
        // JetBrains treats the trailing block as GraphQL *variables* only when
        // it is a JSON object, so a blank line inside the query does not split
        // it. Judgement call: we take the earliest blank line whose remainder is
        // a JSON object, which also tolerates blank lines inside the variables.
        let (q, vars) = match graphql_split(text) {
            Some((q, v)) => (q.trim().to_string(), v.trim().to_string()),
            None => (text.trim().to_string(), String::new()),
        };
        return Ok(Body::GraphQl {
            query: q,
            variables: vars,
        });
    }
    if ct.contains("x-www-form-urlencoded") {
        let flat: String = text.split_whitespace().collect();
        let pairs = flat
            .split('&')
            .filter(|s| !s.is_empty())
            .map(|p| match p.split_once('=') {
                Some((k, v)) => (k.to_string(), v.to_string()),
                None => (p.to_string(), String::new()),
            })
            .collect();
        return Ok(Body::Form(pairs));
    }
    if ct.contains("sparql") {
        return Ok(Body::Sparql(text.to_string()));
    }
    if ct.contains("xml") {
        return Ok(Body::Xml(text.to_string()));
    }
    if ct.contains("json") {
        return Ok(Body::Json(text.to_string()));
    }
    if !ct.is_empty() {
        return Ok(Body::Text(text.to_string()));
    }
    // No Content-Type: guess, the same way serialize() assumes we will.
    Ok(match text.trim_start().chars().next() {
        Some('{') | Some('[') => Body::Json(text.to_string()),
        _ => Body::Text(text.to_string()),
    })
}

fn parse_multipart(text: &str, ctype: &str, where_: &str) -> Result<Body> {
    let in_body = text.lines().find(|l| l.starts_with("--")).map(|l| {
        l.trim()
            .trim_start_matches('-')
            .trim_end_matches('-')
            .to_string()
    });
    let boundary = match (boundary_of(ctype), in_body) {
        (Some(declared), Some(used)) if declared != used => bail!(
            "multipart boundary mismatch at {where_}: Content-Type declares \
             `{declared}` but the body is delimited by `--{used}`"
        ),
        (Some(declared), _) => declared,
        (None, Some(used)) => used,
        (None, None) => bail!(
            "multipart body without a boundary at {where_}: add \
             `; boundary=WebAppBoundary` to the Content-Type header and delimit \
             the parts with `--WebAppBoundary`"
        ),
    };
    let marker = format!("--{boundary}");

    let mut fields = Vec::new();
    let mut cur: Option<Part> = None;
    let flush = |cur: Option<Part>, fields: &mut Vec<MultipartField>| {
        let Some((hdrs, body, _)) = cur else { return };
        let disp = header(&hdrs, "content-disposition").unwrap_or_default();
        let name = disp_param(&disp, "name").unwrap_or_default();
        let filename = disp_param(&disp, "filename");
        let ctype = header(&hdrs, "content-type");
        let mut body: Vec<&str> = body;
        while body.first().is_some_and(|l| l.trim().is_empty()) {
            body.remove(0);
        }
        while body.last().is_some_and(|l| l.trim().is_empty()) {
            body.pop();
        }
        let joined = body.join("\n");
        let file_ref = joined
            .strip_prefix("< ")
            .filter(|_| body.len() == 1)
            .map(|p| p.trim().to_string());
        fields.push(match file_ref {
            Some(path) => MultipartField {
                name,
                value: filename.unwrap_or_default(),
                file_path: Some(path),
                content_type: ctype,
            },
            None => MultipartField {
                name,
                value: joined,
                file_path: None,
                content_type: ctype,
            },
        });
    };

    for line in text.lines() {
        let t = line.trim_end();
        if t.trim_start().starts_with(&marker) {
            flush(cur.take(), &mut fields);
            if !t.trim().ends_with("--") || t.trim() == marker {
                cur = Some((Vec::new(), Vec::new(), false));
            }
            continue;
        }
        let Some((hdrs, body, in_body)) = cur.as_mut() else {
            continue;
        };
        if *in_body {
            body.push(line);
        } else if t.trim().is_empty() {
            *in_body = true;
        } else if is_header_line(t.trim()) {
            let (k, v) = t.trim().split_once(':').unwrap();
            hdrs.push((k.trim().to_string(), v.trim().to_string()));
        } else {
            *in_body = true;
            body.push(line);
        }
    }
    flush(cur.take(), &mut fields);
    Ok(Body::MultipartForm(fields))
}

// ---------------------------------------------------------------- small helpers

fn is_comment(t: &str) -> bool {
    (t.starts_with('#') && !t.starts_with("###")) || t.starts_with("//")
}

/// RFC 7230 `tchar`. The header-name charset, and also the method charset.
fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)
}

/// A custom verb (PURGE, LOCK, MKCOL, PROPFIND, …). Requires a letter so that
/// garbage like `!!!!` — a legal RFC 7230 token — stays part of the URL.
fn is_method_token(t: &str) -> bool {
    !t.is_empty()
        && t.len() <= 32
        && t.chars().any(|c| c.is_ascii_alphabetic())
        && t.chars().all(is_token_char)
}

/// `Name: value`, but not a URL like `http://x`. Header names are RFC 7230
/// tokens (so `.`, `+`, `~` … are legal) and may contain `{{placeholders}}`.
fn is_header_line(t: &str) -> bool {
    let Some((k, v)) = t.split_once(':') else {
        return false;
    };
    !k.is_empty() && strip_placeholders(k).chars().all(is_token_char) && !v.starts_with("//")
}

/// `s` with every `{{…}}` span removed, so the rest can be charset-checked.
fn strip_placeholders(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(a) = rest.find("{{") {
        out.push_str(&rest[..a]);
        match rest[a + 2..].find("}}") {
            Some(b) => rest = &rest[a + 2 + b + 2..],
            None => {
                out.push_str(&rest[a..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Spaces and tabs cannot survive a round trip through a whitespace-separated
/// request line, so they are encoded rather than silently dropped.
fn encode_url_whitespace(url: &str) -> String {
    if !url.contains([' ', '\t']) {
        return url.to_string();
    }
    let mut out = String::with_capacity(url.len());
    for c in url.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '\t' => out.push_str("%09"),
            _ => out.push(c),
        }
    }
    out
}

/// Splits a GraphQL body into `(query, variables)` at the first blank line whose
/// remainder is a JSON object. `None` when there are no variables.
fn graphql_split(text: &str) -> Option<(&str, &str)> {
    let mut at = 0usize;
    while let Some(p) = text[at..].find("\n\n") {
        let cut = at + p;
        let tail = text[cut + 2..].trim();
        if tail.starts_with('{') && tail.ends_with('}') {
            return Some((&text[..cut], &text[cut + 2..]));
        }
        at = cut + 2;
    }
    None
}

/// `@name = value`: an in-file variable declaration. `# @name x` directives are
/// comments and never reach here; a header key can't start with `@`.
fn parse_var_decl(t: &str) -> Option<(String, String)> {
    let (k, v) = t.strip_prefix('@')?.split_once('=')?;
    let k = k.trim();
    (!k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
    .then(|| (k.to_string(), v.trim().to_string()))
}

fn is_trailer(t: &str) -> bool {
    t.starts_with("<>")
        || t.starts_with('>')
        || t.starts_with("< ")
        || t.starts_with("<@")
        || t == "<"
}

fn is_inline_script(t: &str, sigil: char) -> bool {
    t.strip_prefix(sigil)
        .is_some_and(|r| r.trim_start().starts_with("{%"))
}

/// Reads `> {% ... %}` starting at `start`; returns the JS and the next index.
fn read_inline_script(lines: &[&str], start: usize, base: usize) -> Result<(String, usize)> {
    let first = lines[start].trim();
    let after = first[first.find("{%").unwrap() + 2..].to_string();
    if let Some(end) = after.find("%}") {
        return Ok((after[..end].trim().to_string(), start + 1));
    }
    let mut out = vec![after];
    for (n, line) in lines.iter().enumerate().skip(start + 1) {
        match line.find("%}") {
            Some(end) => {
                out.push(line[..end].to_string());
                let body = out.join("\n");
                return Ok((
                    body.trim_matches(['\n', '\r']).trim_end().to_string(),
                    n + 1,
                ));
            }
            None => out.push(line.to_string()),
        }
    }
    bail!(
        "unterminated `{{%` handler script opened at line {}: add a closing `%}}`",
        base + start
    )
}

fn apply_comment(t: &str, name: &mut Option<String>, r: &mut Request) {
    let rest = t.trim_start_matches('#').trim_start_matches('/').trim();
    let Some(directive) = rest.strip_prefix('@') else {
        return;
    }; // plain comment: dropped
    // Both `# @name value` and the documented `# @name = value` (and
    // `# @name=value`) forms are accepted; the `=` is punctuation, not content.
    let (key, val) = match directive.find(|c: char| c.is_whitespace() || c == '=') {
        Some(p) => (
            &directive[..p],
            directive[p..].trim_start().trim_start_matches('=').trim(),
        ),
        None => (directive, ""),
    };
    if key == "name" {
        *name = Some(val.to_string());
    } else {
        r.extra.insert(format!("@{key}"), val.to_string());
    }
}

fn header(headers: &[(String, String)], want: &str) -> Option<String> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(want))
        .map(|(_, v)| v.clone())
}

fn boundary_of(ctype: &str) -> Option<String> {
    ctype.split(';').skip(1).find_map(|p| {
        let (k, v) = p.split_once('=')?;
        (k.trim().eq_ignore_ascii_case("boundary")).then(|| v.trim().trim_matches('"').to_string())
    })
}

fn disp_param(disp: &str, want: &str) -> Option<String> {
    disp.split(';').skip(1).find_map(|p| {
        let (k, v) = p.split_once('=')?;
        (k.trim().eq_ignore_ascii_case(want)).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// `(base, params, indices of params written `k=` with an empty value)`.
fn split_query(url: &str) -> (String, Vec<(String, String)>, Vec<usize>) {
    let Some((base, qs)) = url.split_once('?') else {
        return (url.to_string(), Vec::new(), Vec::new());
    };
    let mut query = Vec::new();
    let mut empty_eq = Vec::new();
    for p in qs.split('&').filter(|s| !s.is_empty()) {
        match p.split_once('=') {
            Some((k, v)) => {
                if v.is_empty() {
                    empty_eq.push(query.len());
                }
                query.push((k.to_string(), v.to_string()));
            }
            None => query.push((p.to_string(), String::new())),
        }
    }
    (base.to_string(), query, empty_eq)
}

// ---------------------------------------------------------------- serialize

pub fn serialize(reqs: &[Request]) -> String {
    let mut out = String::new();
    for r in reqs {
        out.push_str("### ");
        out.push_str(&r.name);
        out.push('\n');
        // `@var` declarations first: `parse` reads them in this same position.
        if let Some(decls) = r.extra.get(K_FILE_VARS) {
            out.push_str(decls);
            out.push('\n');
        }
        for (k, v) in &r.extra {
            if k.starts_with('@') {
                out.push_str(&format!("# {k} {v}\n"));
            }
        }
        if let Some(s) = &r.pre_request_script {
            out.push_str(&format!("< {{%\n{s}\n%}}\n"));
        }
        if let Some(p) = r.extra.get(K_PRE_SCRIPT_FILE) {
            out.push_str(&format!("< {p}\n"));
        }

        out.push_str(&r.method);
        out.push(' ');
        out.push_str(&r.url);
        if !r.query.is_empty() {
            out.push('?');
            let empty_eq = r.extra.get(K_QUERY_EQ).map(String::as_str).unwrap_or("");
            let kept = |i: usize| empty_eq.split(',').any(|s| s.parse() == Ok(i));
            let q: Vec<String> = r
                .query
                .iter()
                .enumerate()
                .map(|(i, (k, v))| match (v.is_empty(), kept(i)) {
                    (true, false) => k.clone(),
                    (true, true) => format!("{k}="),
                    _ => format!("{k}={v}"),
                })
                .collect();
            out.push_str(&q.join("&"));
        }
        if let Some(v) = r.extra.get(K_VERSION) {
            out.push(' ');
            out.push_str(v);
        }
        out.push('\n');

        for (k, v) in &r.headers {
            out.push_str(&format!("{k}: {v}\n"));
        }
        if let Some(ct) = implied_content_type(r) {
            out.push_str(&format!("Content-Type: {ct}\n"));
        }

        let body = body_text(r);
        if !body.is_empty() {
            out.push('\n');
            out.push_str(&body);
            out.push('\n');
        }
        if let Some(s) = &r.post_response_script {
            out.push_str(&format!("\n> {{%\n{s}\n%}}\n"));
        }
        if let Some(p) = r.extra.get(K_POST_SCRIPT_FILE) {
            out.push_str(&format!("\n> {p}\n"));
        }
        if let Some(p) = r.extra.get(K_REDIRECT) {
            out.push_str(&format!("\n>> {p}\n"));
        }
        if let Some(p) = r.extra.get(K_REDIRECT_FORCE) {
            out.push_str(&format!("\n>>! {p}\n"));
        }
        if let Some(rf) = r.extra.get(K_RESPONSE_REF) {
            out.push_str(&format!("\n<> {rf}\n"));
        }
        out.push('\n');
    }
    out
}

/// A Content-Type we must write out so `parse` recovers the same `Body` variant.
fn implied_content_type(r: &Request) -> Option<&'static str> {
    if header(&r.headers, "content-type").is_some() {
        return None;
    }
    let looks_json = |s: &str| matches!(s.trim_start().chars().next(), Some('{') | Some('['));
    match &r.body {
        Body::None => None,
        Body::Json(s) => (!looks_json(s)).then_some("application/json"),
        Body::Text(s) => looks_json(s).then_some("text/plain"),
        Body::Xml(_) => Some("application/xml"),
        Body::Sparql(_) => Some("application/sparql-query"),
        Body::Form(_) => Some("application/x-www-form-urlencoded"),
        Body::MultipartForm(_) => Some("multipart/form-data; boundary=WebAppBoundary"),
        Body::GraphQl { .. } => (r.method != "GRAPHQL").then_some("application/graphql"),
        Body::File { .. } => None,
    }
}

fn body_text(r: &Request) -> String {
    match &r.body {
        Body::None => String::new(),
        Body::Json(s) | Body::Text(s) | Body::Xml(s) | Body::Sparql(s) => s.clone(),
        Body::Form(pairs) => pairs
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&"),
        Body::GraphQl { query, variables } => {
            if variables.is_empty() {
                query.clone()
            } else {
                format!("{query}\n\n{variables}")
            }
        }
        Body::File { path, .. } => match r.extra.get(K_BODY_FILE_ENC) {
            Some(enc) => format!("<@ {enc} {path}"),
            None => format!("< {path}"),
        },
        Body::MultipartForm(fields) => {
            let ct = header(&r.headers, "content-type").unwrap_or_default();
            let boundary = boundary_of(&ct).unwrap_or_else(|| "WebAppBoundary".into());
            let mut s = String::new();
            for f in fields {
                s.push_str(&format!("--{boundary}\n"));
                s.push_str(&format!(
                    r#"Content-Disposition: form-data; name="{}""#,
                    f.name
                ));
                if let (Some(_), false) = (&f.file_path, f.value.is_empty()) {
                    s.push_str(&format!(r#"; filename="{}""#, f.value));
                }
                s.push('\n');
                if let Some(ct) = &f.content_type {
                    s.push_str(&format!("Content-Type: {ct}\n"));
                }
                s.push('\n');
                match &f.file_path {
                    Some(p) => s.push_str(&format!("< {p}\n")),
                    None => {
                        s.push_str(&f.value);
                        s.push('\n');
                    }
                }
            }
            s.push_str(&format!("--{boundary}--"));
            s
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(src: &str) -> Request {
        let mut v = parse(src).expect("parse");
        assert_eq!(v.len(), 1, "expected one request in {src:?}");
        v.remove(0)
    }

    fn roundtrip(src: &str) {
        let a = parse(src).expect("first parse");
        let text = serialize(&a);
        let b = parse(&text).unwrap_or_else(|e| panic!("reparse of\n{text}\nfailed: {e}"));
        assert_eq!(a, b, "round-trip mismatch, serialized:\n{text}");
    }

    #[test]
    fn multiple_requests_and_seq() {
        let src = "\
GET https://example.com/one

### second
POST https://example.com/two
Content-Type: application/json

{\"a\": 1}

### third
DELETE https://example.com/three
";
        let rs = parse(src).unwrap();
        assert_eq!(rs.len(), 3);
        assert_eq!((rs[0].seq, rs[1].seq, rs[2].seq), (0, 1, 2));
        assert_eq!(rs[0].name, "https://example.com/one"); // falls back to URL
        assert_eq!(rs[1].name, "second");
        assert_eq!(rs[2].method, "DELETE");
        roundtrip(src);
    }

    #[test]
    fn default_method_is_get() {
        let r = one("https://example.com/x\n");
        assert_eq!(r.method, "GET");
        assert_eq!(r.url, "https://example.com/x");
    }

    #[test]
    fn name_from_at_name_directive() {
        let r = one("### separator name\n# @name realName\nGET https://x/\n");
        assert_eq!(r.name, "realName");
        let r = one("// @name slashName\nGET https://x/\n");
        assert_eq!(r.name, "slashName");
    }

    #[test]
    fn separator_name_can_end_with_hash() {
        let src = "### weird#2#\nGET https://x/\n";
        let r = one(src);
        assert_eq!(r.name, "weird#2#");
        roundtrip(src);
    }

    #[test]
    fn directives_land_in_extra() {
        let r = one("# @no-redirect\n# @timeout 10\nGET https://x/\n");
        assert_eq!(r.extra.get("@no-redirect").map(String::as_str), Some(""));
        assert_eq!(r.extra.get("@timeout").map(String::as_str), Some("10"));
        roundtrip("# @no-redirect\n# @timeout 10\nGET https://x/\n");
    }

    #[test]
    fn comments_are_not_separators() {
        let src = "\
# a plain comment
// another one
GET https://example.com/a
# comment between headers
Accept: application/json

### real separator
GET https://example.com/b
";
        let rs = parse(src).unwrap();
        assert_eq!(rs.len(), 2);
        assert_eq!(
            rs[0].headers,
            vec![("Accept".into(), "application/json".into())]
        );
    }

    #[test]
    fn request_line_version_and_headers() {
        let r = one("POST https://x/y HTTP/1.1\nA: 1\nB: 2\n\nhi\n");
        assert_eq!(r.extra.get(K_VERSION).map(String::as_str), Some("HTTP/1.1"));
        assert_eq!(r.headers.len(), 2);
        assert_eq!(r.body, Body::Text("hi".into()));
        roundtrip("POST https://x/y HTTP/1.1\nA: 1\nB: 2\n\nhi\n");
    }

    #[test]
    fn single_url_token_starting_with_http_slash_is_not_version() {
        let src =
            "POST HTTP/https://api.exant-Type:%20multipart/form-data;%20boundary=WebAppBoundary\n";
        let r = one(src);
        assert_eq!(
            r.url,
            "HTTP/https://api.exant-Type:%20multipart/form-data;%20boundary=WebAppBoundary"
        );
        assert_eq!(r.method, "POST");
        assert_eq!(r.extra.get(K_VERSION), None);
        roundtrip(src);
    }

    #[test]
    fn lone_question_mark_is_not_a_valid_url() {
        let err = parse("?\n").unwrap_err().to_string();
        assert!(err.contains("has no URL"), "{err}");
    }

    #[test]
    fn url_continuation_and_split_query() {
        let src = "\
GET https://example.com/search
    ?q=rust
    &page=2
    &flag
Accept: */*
";
        let r = one(src);
        assert_eq!(r.url, "https://example.com/search");
        assert_eq!(
            r.query,
            vec![
                ("q".into(), "rust".into()),
                ("page".into(), "2".into()),
                ("flag".into(), String::new()),
            ]
        );
        roundtrip(src);
    }

    #[test]
    fn indented_url_continuation() {
        let r = one("GET https://example.com\n    /deep/path?a=b\n");
        assert_eq!(r.url, "https://example.com/deep/path");
        assert_eq!(r.query, vec![("a".into(), "b".into())]);
    }

    #[test]
    fn body_kinds() {
        let json = one("POST /x\nContent-Type: application/json\n\n{\"a\":1}\n");
        assert_eq!(json.body, Body::Json("{\"a\":1}".into()));

        let xml = one("POST /x\nContent-Type: application/xml\n\n<r><a/></r>\n");
        assert_eq!(xml.body, Body::Xml("<r><a/></r>".into()));

        let text = one("POST /x\nContent-Type: text/plain\n\nhello\nworld\n");
        assert_eq!(text.body, Body::Text("hello\nworld".into()));

        let form = one("POST /x\nContent-Type: application/x-www-form-urlencoded\n\na=1&b=two\n");
        assert_eq!(
            form.body,
            Body::Form(vec![("a".into(), "1".into()), ("b".into(), "two".into())])
        );

        let sparql = one("POST /x\nContent-Type: application/sparql-query\n\nSELECT *\n");
        assert_eq!(sparql.body, Body::Sparql("SELECT *".into()));

        let gql = one("GRAPHQL /x\n\nquery { me }\n\n{\"v\":1}\n");
        assert_eq!(
            gql.body,
            Body::GraphQl {
                query: "query { me }".into(),
                variables: "{\"v\":1}".into()
            }
        );

        let none = one("GET /x\nAccept: */*\n\n");
        assert_eq!(none.body, Body::None);
    }

    #[test]
    fn multiline_form_body() {
        let r = one("POST /x\nContent-Type: application/x-www-form-urlencoded\n\na=1&\n  b=2\n");
        assert_eq!(
            r.body,
            Body::Form(vec![("a".into(), "1".into()), ("b".into(), "2".into())])
        );
    }

    #[test]
    fn body_kinds_round_trip() {
        for src in [
            "POST /x\nContent-Type: application/json\n\n{\"a\":1}\n",
            "POST /x\nContent-Type: application/xml\n\n<r><a/></r>\n",
            "POST /x\nContent-Type: text/plain\n\nhello\nworld\n",
            "POST /x\nContent-Type: application/x-www-form-urlencoded\n\na=1&b=two\n",
            "POST /x\nContent-Type: application/sparql-query\n\nSELECT *\n",
            "GRAPHQL /x\n\nquery { me }\n\n{\"v\":1}\n",
            "GET /x\nAccept: */*\n",
        ] {
            roundtrip(src);
        }
    }

    #[test]
    fn body_variant_survives_without_content_type() {
        // Text that looks like JSON still comes back as Text, because serialize
        // writes the Content-Type it needs to disambiguate.
        let r = Request {
            name: "t".into(),
            method: "POST".into(),
            url: "https://x/".into(),
            body: Body::Text("{not json}".into()),
            ..Request::default()
        };
        let back = parse(&serialize(std::slice::from_ref(&r))).unwrap();
        assert_eq!(back[0].body, r.body);
    }

    #[test]
    fn body_from_file() {
        let r = one("POST /x\nContent-Type: application/json\n\n< ./payload.json\n");
        assert_eq!(
            r.body,
            Body::File {
                path: "./payload.json".into(),
                content_type: "application/json".into()
            }
        );
        roundtrip("POST /x\nContent-Type: application/json\n\n< ./payload.json\n");
    }

    #[test]
    fn body_from_file_with_encoding() {
        let r = one("POST /x\n\n<@ latin1 ./payload.txt\n");
        assert_eq!(
            r.body,
            Body::File {
                path: "./payload.txt".into(),
                content_type: String::new()
            }
        );
        assert_eq!(
            r.extra.get(K_BODY_FILE_ENC).map(String::as_str),
            Some("latin1")
        );
        roundtrip("POST /x\n\n<@ latin1 ./payload.txt\n");
    }

    #[test]
    fn multipart_body() {
        let src = "\
POST https://example.com/upload
Content-Type: multipart/form-data; boundary=WebAppBoundary

--WebAppBoundary
Content-Disposition: form-data; name=\"field\"

plain value
--WebAppBoundary
Content-Disposition: form-data; name=\"doc\"; filename=\"a.json\"
Content-Type: application/json

< ./a.json
--WebAppBoundary--
";
        let r = one(src);
        let Body::MultipartForm(fields) = &r.body else {
            panic!("not multipart: {:?}", r.body)
        };
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "field");
        assert_eq!(fields[0].value, "plain value");
        assert_eq!(fields[0].file_path, None);
        assert_eq!(fields[1].name, "doc");
        assert_eq!(fields[1].value, "a.json");
        assert_eq!(fields[1].file_path.as_deref(), Some("./a.json"));
        assert_eq!(fields[1].content_type.as_deref(), Some("application/json"));
        roundtrip(src);
    }

    #[test]
    fn response_handler_scripts() {
        let src = "\
GET https://example.com/token

> {%
    client.global.set(\"tok\", response.body.token);
%}
";
        let r = one(src);
        assert!(
            r.post_response_script
                .as_deref()
                .unwrap()
                .contains("client.global.set")
        );
        assert_eq!(r.body, Body::None);
        roundtrip(src);
    }

    #[test]
    fn one_line_and_file_handlers() {
        let r = one("GET /x\n\n> {% client.log(1); %}\n");
        assert_eq!(r.post_response_script.as_deref(), Some("client.log(1);"));

        let r = one("GET /x\n\n> ./handler.js\n");
        assert_eq!(r.post_response_script, None);
        assert_eq!(
            r.extra.get(K_POST_SCRIPT_FILE).map(String::as_str),
            Some("./handler.js")
        );
        roundtrip("GET /x\n\n> ./handler.js\n");
    }

    #[test]
    fn pre_request_script() {
        let src = "< {%\n  request.variables.set(\"x\", 1);\n%}\nGET https://example.com/\n";
        let r = one(src);
        assert!(
            r.pre_request_script
                .as_deref()
                .unwrap()
                .contains("request.variables.set")
        );
        roundtrip(src);
    }

    #[test]
    fn response_reference_preserved() {
        let src = "GET https://example.com/\n\n<> previous-response.200.json\n";
        let r = one(src);
        assert_eq!(
            r.extra.get(K_RESPONSE_REF).map(String::as_str),
            Some("previous-response.200.json")
        );
        roundtrip(src);
    }

    #[test]
    fn variables_survive_verbatim() {
        let src = "\
### templated
POST {{baseUrl}}/api/{{version}}/users?token={{token}}
Authorization: Bearer {{auth_token}}
Content-Type: application/json

{\"name\": \"{{userName}}\"}
";
        let r = one(src);
        assert_eq!(r.url, "{{baseUrl}}/api/{{version}}/users");
        assert_eq!(r.query, vec![("token".into(), "{{token}}".into())]);
        assert_eq!(r.headers[0].1, "Bearer {{auth_token}}");
        assert_eq!(r.body, Body::Json("{\"name\": \"{{userName}}\"}".into()));
        assert!(serialize(&[r]).contains("{{baseUrl}}"));
        roundtrip(src);
    }

    #[test]
    fn malformed_inputs_return_err() {
        for bad in [
            "Content-Type: application/json\n\nnot a request\n", // header first
            "POST\n",                                            // no URL
            "GET /x\n\n> {%\nunterminated\n",                    // unclosed handler
            "GET /x\nthis is not a header\n\nbody\n",            // junk in headers
            "GET /x\n\n>\n",                                     // empty handler ref
        ] {
            assert!(parse(bad).is_err(), "expected Err for {bad:?}");
        }
    }

    #[test]
    fn empty_and_comment_only_files() {
        assert!(parse("").unwrap().is_empty());
        assert!(
            parse("# just a note\n\n### \n// nothing here\n")
                .unwrap()
                .is_empty()
        );
        assert_eq!(serialize(&[]), "");
    }

    // ------------------------------------------------------------ regressions
    // One per bug found by tests/http_corpus.rs, pinned at unit level.

    #[test]
    fn regression_1_custom_method_is_not_swallowed_by_the_url() {
        for (src, method) in [
            ("PURGE https://cdn.example.com/a.css\n", "PURGE"),
            ("PROPFIND https://dav.example.com/\n", "PROPFIND"),
            ("mkcol https://dav.example.com/d/\n", "mkcol"),
        ] {
            let r = one(src);
            assert_eq!(r.method, method);
            assert!(r.url.starts_with("https://"), "{}", r.url);
            roundtrip(src);
        }
        // A single token is still a method-less GET, not a method with no URL.
        assert_eq!(one("https://example.com/x\n").method, "GET");
        // ... and garbage stays garbage rather than becoming a method.
        assert_eq!(one("!!!! ????\n").method, "GET");
    }

    #[test]
    fn regression_2_pre_request_script_file_round_trips() {
        let src = "### with-pre-script\n< ./pre.js\nGET https://example.com/x\n";
        let r = one(src);
        assert_eq!(r.url, "https://example.com/x");
        assert_eq!(
            r.extra.get(K_PRE_SCRIPT_FILE).map(String::as_str),
            Some("./pre.js")
        );
        roundtrip(src);
    }

    #[test]
    fn regression_3_bom_prefix_is_stripped() {
        let r = one("\u{feff}GET https://example.com/bom\nAccept: */*\n");
        assert_eq!(r.method, "GET");
        assert_eq!(r.url, "https://example.com/bom");
    }

    #[test]
    fn regression_4_multipart_boundary_mismatch_is_an_error() {
        let err = parse(
            "POST /u\nContent-Type: multipart/form-data; boundary=AAA\n\n\
             --BBB\nContent-Disposition: form-data; name=\"a\"\n\none\n--BBB--\n",
        )
        .expect_err("a mismatched boundary must not be swallowed");
        let msg = format!("{err:#}");
        assert!(msg.contains("AAA") && msg.contains("BBB"), "{msg}");
    }

    #[test]
    fn regression_5_and_6_header_names_are_rfc7230_tokens_or_placeholders() {
        let r = one("GET /x\n{{headerName}}: value\nX.Legacy.Trace: 1\nX+A~B: 2\n");
        assert_eq!(
            r.headers,
            vec![
                ("{{headerName}}".into(), "value".into()),
                ("X.Legacy.Trace".into(), "1".into()),
                ("X+A~B".into(), "2".into()),
            ]
        );
        roundtrip("GET /x\n{{headerName}}: value\nX.Legacy.Trace: 1\nX+A~B: 2\n");
    }

    #[test]
    fn regression_7_at_name_accepts_an_equals_sign() {
        assert_eq!(one("# @name = loginUser\nGET /x\n").name, "loginUser");
        assert_eq!(one("# @name=loginUser\nGET /x\n").name, "loginUser");
        assert_eq!(one("# @name loginUser\nGET /x\n").name, "loginUser");
    }

    #[test]
    fn regression_8_response_redirect_has_its_own_key() {
        let r = one("GET /x\n\n>> ./out/response.json\n");
        assert_eq!(
            r.extra.get(K_REDIRECT).map(String::as_str),
            Some("./out/response.json")
        );
        assert_eq!(r.extra.get(K_POST_SCRIPT_FILE), None);
        roundtrip("GET /x\n\n>> ./out/response.json\n");

        let r = one("GET /x\n\n>>! ./out/response.json\n");
        assert_eq!(
            r.extra.get(K_REDIRECT_FORCE).map(String::as_str),
            Some("./out/response.json")
        );
        roundtrip("GET /x\n\n>>! ./out/response.json\n");
    }

    #[test]
    fn regression_9_empty_query_value_keeps_its_equals_sign() {
        let src = "GET https://example.com/x?a=&b=1&flag\n";
        let r = one(src);
        assert_eq!(
            r.query,
            vec![
                ("a".into(), String::new()),
                ("b".into(), "1".into()),
                ("flag".into(), String::new()),
            ]
        );
        let out = serialize(std::slice::from_ref(&r));
        assert!(out.contains("?a=&b=1&flag"), "{out}");
        roundtrip(src);
    }

    #[test]
    fn regression_11_graphql_blank_line_inside_the_query_does_not_split_it() {
        let src = "POST /g\nContent-Type: application/graphql\n\n\
                   query Two {\n  a\n}\n\nquery Also {\n  b\n}\n";
        let r = one(src);
        let Body::GraphQl { query, variables } = &r.body else {
            panic!("{:?}", r.body)
        };
        assert!(variables.is_empty(), "{variables:?}");
        assert!(query.contains("query Also"), "{query}");
        roundtrip(src);

        // A trailing JSON object *is* the variables, even after a blank line in
        // the query.
        let src = "POST /g\nContent-Type: application/graphql\n\n\
                   query Two {\n  a\n}\n\nquery Also {\n  b\n}\n\n{\"v\":1}\n";
        let Body::GraphQl { variables, .. } = &one(src).body else {
            panic!()
        };
        assert_eq!(variables, "{\"v\":1}");
        roundtrip(src);
    }

    #[test]
    fn regression_12_in_file_var_declarations_survive_serialize() {
        let src = "@host = https://api.example.com\n@v = 2\n\n### one\nGET {{host}}/{{v}}\n";
        let (reqs, decls) = parse_with_vars(src).unwrap();
        assert_eq!(decls.len(), 2);
        let (_, again) = parse_with_vars(&serialize(&reqs)).unwrap();
        assert_eq!(again, decls);
        roundtrip(src);
    }

    #[test]
    fn regression_13_and_error_locations() {
        // The bad declaration is named, not the innocent next line.
        let msg = format!(
            "{:#}",
            parse("@baseUrl http://localhost:8080\nGET https://example.com/x\n").unwrap_err()
        );
        assert!(msg.contains("@baseUrl") && msg.contains("line 1"), "{msg}");

        // Every error carries a 1-based line number and the enclosing request.
        for (src, needle) in [
            (
                "### a\nGET /x\n\n### b\nGET /y\njunk\n",
                "line 6 of request \"b\"",
            ),
            ("GET /x\n\n>\n", "line 3 of request"),
            ("GET /x\n\n<@\n", "line 3 of request"),
            ("GET /x\n\n> {%\n", "line 3"),
            ("Content-Type: a/b\n\nnope\n", "line 1 of request #0"),
        ] {
            let msg = format!("{:#}", parse(src).unwrap_err());
            assert!(msg.contains(needle), "{msg:?} should contain {needle:?}");
        }
    }

    #[test]
    fn regression_14_whitespace_in_a_folded_url_is_encoded_not_eaten() {
        // The fuzz reproducer: serialize must be a fixed point.
        let src = "GET h\n a b\n";
        let a = parse(src).unwrap();
        assert_eq!(a[0].url, "ha%20b");
        assert_eq!(parse(&serialize(&a)).unwrap(), a);
        roundtrip(src);
        roundtrip("GET https://x/a b?q=1 2\n");
    }

    #[test]
    fn fixture_files_parse() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/http");
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).expect("fixture dir") {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "http") {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            let reqs = parse(&src).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(!reqs.is_empty(), "{} parsed to nothing", path.display());
            roundtrip(&src);
            seen += 1;
        }
        assert!(seen >= 3, "expected at least 3 fixtures, found {seen}");
    }
}
