//! Request execution: interpolate -> build -> send -> assert.
use crate::interp::{Vars, interpolate};
use crate::model::{ApiKeyPlacement, Assert, Auth, Body, MultipartField, Request, Response};
use anyhow::{Context, Result, bail};
use base64::Engine;
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Instant;

/// How long to wait for the connection itself.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
/// The `# @no-redirect` directive, as `httpfile` stores it. A 3xx then comes back
/// as the response instead of being followed, which is what you want when the
/// redirect *is* the thing under test.
const NO_REDIRECT: &str = "@no-redirect";
/// The `>> file` and `>>! file` response redirects, as `httpfile` stores them.
/// Acted on by [`write_redirect`], which needs a base directory `send` has not got.
const REDIRECT: &str = "response_redirect";
const REDIRECT_FORCE: &str = "response_redirect_force";

/// Ceiling for a whole request when the file does not say otherwise.
/// Overridable per request with the JetBrains `# @timeout <seconds>` directive.
const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// The cookie jar, shared by every client below.
///
/// Held apart from the clients because there are two of them — see [`client`] —
/// and a request that opts out of redirects must still see the session cookie an
/// earlier request was given. Handing both clients the same jar is what keeps
/// that true; `cookie_store(true)` would give each its own.
fn jar() -> &'static std::sync::Arc<reqwest::cookie::Jar> {
    static J: OnceLock<std::sync::Arc<reqwest::cookie::Jar>> = OnceLock::new();
    J.get_or_init(Default::default)
}

fn build_client(follow_redirects: bool) -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_provider(jar().clone())
        .gzip(true)
        .redirect(if follow_redirects {
            reqwest::redirect::Policy::limited(10)
        } else {
            reqwest::redirect::Policy::none()
        })
        // reqwest has no default timeout, so without these a server that
        // accepts the connection and then says nothing wedges the caller
        // forever — in the UI that is a request that can never be cancelled.
        // Generous, because a slow API is not an error; the point is that
        // there is an end.
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(DEFAULT_TIMEOUT)
        .build()
        .expect("reqwest client")
}

/// The client for this request, kept for the whole process: the connection pool
/// is shared state and building one per request would leak it.
///
/// Two of them, because reqwest fixes the redirect policy on the *client* and the
/// JetBrains `# @no-redirect` directive is per request. They share the jar, so the
/// only thing that differs is whether a 3xx is followed.
pub fn client_for(req: &Request) -> &'static reqwest::Client {
    if req.extra.contains_key(NO_REDIRECT) {
        static NOT_FOLLOWING: OnceLock<reqwest::Client> = OnceLock::new();
        NOT_FOLLOWING.get_or_init(|| build_client(false))
    } else {
        following_client()
    }
}

/// The ordinary client. Used directly for machinery that is not the user's
/// request — an OAuth2 token fetch, where a 3xx is the provider's business and
/// never something the caller asked to inspect.
fn following_client() -> &'static reqwest::Client {
    static FOLLOWING: OnceLock<reqwest::Client> = OnceLock::new();
    FOLLOWING.get_or_init(|| build_client(true))
}

pub async fn send(req: &Request, vars: &Vars) -> Result<Response> {
    let mut r = interp_request(req, vars);
    check_resolved(req, vars)?;
    r.auth = resolve_auth(&r).await?;
    let built = build_request(&r)?;

    let start = Instant::now();
    let res = client_for(req).execute(built).await?;
    let status = res.status();
    let headers: Vec<(String, String)> = res
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                v.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let body = res.bytes().await?.to_vec();
    let duration_ms = start.elapsed().as_millis();

    Ok(Response {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or_default().to_string(),
        headers,
        size_bytes: body.len(),
        body,
        duration_ms,
    })
}

/// Carry out a `>> file` / `>>! file` response redirect, if the request has one.
///
/// Returns the path written, or `None` when the request asked for nothing. `>>`
/// keeps an existing file and reports that it did; `>>!` overwrites. `base` is the
/// directory of the `.http` file, because that is what a relative path in it means
/// — the process's working directory is not it.
///
/// Separate from [`send`] rather than inside it: `send` takes no base directory
/// and should not start writing to the disk of its own accord. The caller decides,
/// and the caller is the one that knows where the file came from.
pub fn write_redirect(
    req: &Request,
    body: &[u8],
    base: &std::path::Path,
) -> Result<Option<std::path::PathBuf>> {
    let (target, force) = match (req.extra.get(REDIRECT), req.extra.get(REDIRECT_FORCE)) {
        (_, Some(p)) => (p, true),
        (Some(p), None) => (p, false),
        (None, None) => return Ok(None),
    };
    let path = base.join(target);
    if !force && path.exists() {
        bail!(
            "{} exists and `>>` does not overwrite; write `>>! {target}` if that is what you want",
            path.display()
        );
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    std::fs::write(&path, body).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(Some(path))
}

/// Fail before hitting the wire when a `{{variable}}` went unresolved.
///
/// Otherwise the literal braces reach the HTTP client and surface as an opaque
/// URL parse failure that says nothing about the actual cause.
fn check_resolved(req: &Request, vars: &Vars) -> Result<()> {
    let mut names = Vec::new();
    let mut scan = |s: &str| {
        for n in crate::interp::missing(s, vars) {
            if !names.contains(&n) {
                names.push(n);
            }
        }
    };
    scan(&req.url);
    for (k, v) in req.headers.iter().chain(req.query.iter()) {
        scan(k);
        scan(v);
    }
    match &req.body {
        Body::Json(s) | Body::Text(s) | Body::Xml(s) | Body::Sparql(s) => scan(s),
        Body::GraphQl { query, variables } => {
            scan(query);
            scan(variables);
        }
        Body::Form(f) => {
            for (k, v) in f {
                scan(k);
                scan(v);
            }
        }
        Body::MultipartForm(f) => {
            for m in f {
                scan(&m.name);
                scan(&m.value);
            }
        }
        Body::File { path, .. } => scan(path),
        Body::None => {}
    }
    if names.is_empty() {
        return Ok(());
    }
    bail!(
        "unresolved variable{}: {}. Define {} in an environment, as `@name = value` in the \
         file, or have an earlier request capture {} via `client.global.set`.",
        if names.len() == 1 { "" } else { "s" },
        names.join(", "),
        if names.len() == 1 { "it" } else { "them" },
        if names.len() == 1 { "it" } else { "them" },
    )
}

// ---------------------------------------------------------------- interpolation

fn interp_request(req: &Request, vars: &Vars) -> Request {
    let i = |s: &String| interpolate(s, vars);
    let pairs = |v: &Vec<(String, String)>| v.iter().map(|(k, val)| (i(k), i(val))).collect();
    Request {
        url: i(&req.url),
        headers: pairs(&req.headers),
        query: pairs(&req.query),
        body: match &req.body {
            Body::None => Body::None,
            Body::Json(s) => Body::Json(i(s)),
            Body::Text(s) => Body::Text(i(s)),
            Body::Xml(s) => Body::Xml(i(s)),
            Body::Sparql(s) => Body::Sparql(i(s)),
            Body::Form(f) => Body::Form(pairs(f)),
            Body::MultipartForm(f) => Body::MultipartForm(
                f.iter()
                    .map(|m| MultipartField {
                        name: i(&m.name),
                        value: i(&m.value),
                        file_path: m.file_path.as_ref().map(i),
                        content_type: m.content_type.clone(),
                    })
                    .collect(),
            ),
            Body::GraphQl { query, variables } => Body::GraphQl {
                query: i(query),
                variables: i(variables),
            },
            Body::File { path, content_type } => Body::File {
                path: i(path),
                content_type: content_type.clone(),
            },
        },
        auth: match &req.auth {
            Auth::Basic { username, password } => Auth::Basic {
                username: i(username),
                password: i(password),
            },
            Auth::Bearer { token } => Auth::Bearer { token: i(token) },
            Auth::ApiKey {
                key,
                value,
                placement,
            } => Auth::ApiKey {
                key: i(key),
                value: i(value),
                placement: *placement,
            },
            Auth::OAuth2(o) => {
                let mut o = (**o).clone();
                o.access_token_url = i(&o.access_token_url);
                o.authorization_url = i(&o.authorization_url);
                o.client_id = i(&o.client_id);
                o.client_secret = i(&o.client_secret);
                o.scope = i(&o.scope);
                o.callback_url = i(&o.callback_url);
                Auth::OAuth2(Box::new(o))
            }
            other => other.clone(),
        },
        extra: req.extra.iter().map(|(k, v)| (k.clone(), i(v))).collect(),
        ..req.clone()
    }
}

// ------------------------------------------------------------------------ auth

/// Turns OAuth2 into a plain Bearer by fetching a token; every other variant
/// passes through untouched so `build_request` stays synchronous and testable.
async fn resolve_auth(req: &Request) -> Result<Auth> {
    let Auth::OAuth2(o) = &req.auth else {
        return Ok(req.auth.clone());
    };
    let mut form: Vec<(&str, &str)> = vec![("grant_type", o.grant_type.as_str())];
    match o.grant_type.as_str() {
        "client_credentials" => {}
        "password" => {
            // ponytail: OAuth2 has no username/password fields, so the resource-owner
            // credentials ride along in `extra` (as the .bru parser stores them).
            let user = req
                .extra
                .get("oauth2_username")
                .or_else(|| req.extra.get("username"))
                .map(String::as_str)
                .context("oauth2 password grant needs `username` in the request")?;
            let pass = req
                .extra
                .get("oauth2_password")
                .or_else(|| req.extra.get("password"))
                .map(String::as_str)
                .unwrap_or("");
            form.push(("username", user));
            form.push(("password", pass));
        }
        "authorization_code" => bail!(
            "oauth2 authorization_code needs a browser round-trip; the UI has to drive \
             the flow and hand exec::send a Bearer token"
        ),
        other => bail!("unsupported oauth2 grant type: {other}"),
    }
    if !o.client_id.is_empty() {
        form.push(("client_id", &o.client_id));
    }
    if !o.client_secret.is_empty() {
        form.push(("client_secret", &o.client_secret));
    }
    if !o.scope.is_empty() {
        form.push(("scope", &o.scope));
    }

    let res = following_client()
        .post(&o.access_token_url)
        .form(&form)
        .send()
        .await
        .context("oauth2 token request failed")?;
    if !res.status().is_success() {
        bail!("oauth2 token endpoint returned {}", res.status());
    }
    let json: Value = res
        .json()
        .await
        .context("oauth2 token response is not JSON")?;
    let token = json
        .get("access_token")
        .and_then(Value::as_str)
        .context("oauth2 token response has no access_token")?;
    Ok(Auth::Bearer {
        token: token.to_string(),
    })
}

// ----------------------------------------------------------------- build

fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name))
}

/// A JetBrains `@timeout` value: a number, optionally followed by a unit with or
/// without a space (`600`, `30 s`, `2m`, `500ms`). A bare number is seconds.
///
/// Anything else is an error, not a silently dropped directive — a timeout the
/// user wrote and the client ignored is worse than no timeout at all.
fn parse_timeout(raw: &str) -> Result<std::time::Duration> {
    let t = raw.trim();
    let split = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '+'))
        .unwrap_or(t.len());
    let (number, unit) = t.split_at(split);
    let value: f64 = number
        .parse()
        .with_context(|| format!("@timeout {raw:?} does not start with a number"))?;
    let seconds = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "s" | "sec" | "secs" | "second" | "seconds" => value,
        "ms" | "milli" | "millis" | "milliseconds" => value / 1000.0,
        "m" | "min" | "mins" | "minute" | "minutes" => value * 60.0,
        "h" | "hr" | "hrs" | "hour" | "hours" => value * 3600.0,
        other => bail!("@timeout {raw:?} has an unknown unit {other:?}"),
    };
    if !seconds.is_finite() || seconds <= 0.0 {
        bail!("@timeout must be a positive duration, got {raw:?}");
    }
    Ok(std::time::Duration::from_secs_f64(seconds))
}

fn build_request(r: &Request) -> Result<reqwest::Request> {
    let method = reqwest::Method::from_bytes(r.method.trim().to_uppercase().as_bytes())
        .with_context(|| format!("bad http method {:?}", r.method))?;
    let mut rb = client_for(r).request(method, &r.url);

    // `# @timeout` from a .http file overrides the client default.
    //
    // ponytail: `@connection-timeout` is parsed by the .http reader too, but
    // reqwest only takes a connect timeout on the client, not per request, so it
    // cannot be honoured here. Left unapplied rather than silently mapped onto
    // the overall timeout, which would mean something different.
    if let Some(raw) = r.extra.get("@timeout") {
        rb = rb.timeout(parse_timeout(raw)?);
    }

    let mut query = r.query.clone();
    for (k, v) in &r.headers {
        rb = rb.header(k.as_str(), v.as_str());
    }

    match &r.auth {
        Auth::None | Auth::Inherit => {}
        Auth::Basic { username, password } => {
            let raw = format!("{username}:{password}");
            let b64 = base64::engine::general_purpose::STANDARD.encode(raw);
            rb = rb.header("authorization", format!("Basic {b64}"));
        }
        Auth::Bearer { token } => {
            rb = rb.header("authorization", format!("Bearer {token}"));
        }
        Auth::ApiKey {
            key,
            value,
            placement,
        } => match placement {
            ApiKeyPlacement::Header => rb = rb.header(key.as_str(), value.as_str()),
            ApiKeyPlacement::Query => query.push((key.clone(), value.clone())),
        },
        // resolve_auth turned this into Bearer already.
        Auth::OAuth2(_) => bail!("oauth2 auth reached build_request unresolved"),
    }

    if !query.is_empty() {
        rb = rb.query(&query);
    }

    let ct_set = has_header(&r.headers, "content-type");
    let with_ct = |rb: reqwest::RequestBuilder, ct: &str| {
        if ct_set {
            rb
        } else {
            rb.header("content-type", ct)
        }
    };

    rb = match &r.body {
        Body::None => rb,
        Body::Json(s) => with_ct(rb.body(s.clone()), "application/json"),
        Body::Text(s) => with_ct(rb.body(s.clone()), "text/plain"),
        Body::Xml(s) => with_ct(rb.body(s.clone()), "application/xml"),
        Body::Sparql(s) => with_ct(rb.body(s.clone()), "application/sparql-query"),
        Body::Form(f) => rb.form(f),
        Body::MultipartForm(fields) => {
            let (boundary, bytes) = multipart_body(fields)?;
            rb.header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(bytes)
        }
        Body::GraphQl { query, variables } => {
            let vars: Value = if variables.trim().is_empty() {
                Value::Null
            } else {
                serde_json::from_str(variables).unwrap_or(Value::Null)
            };
            let payload = serde_json::json!({ "query": query, "variables": vars });
            with_ct(rb.body(serde_json::to_vec(&payload)?), "application/json")
        }
        Body::File { path, content_type } => {
            let bytes = std::fs::read(path).with_context(|| format!("reading body file {path}"))?;
            let ct = if content_type.is_empty() {
                "application/octet-stream"
            } else {
                content_type
            };
            with_ct(rb.body(bytes), ct)
        }
    };

    Ok(rb.build()?)
}

/// Hand-rolled multipart/form-data — reqwest's `multipart` feature is not
/// enabled and the wire format is a dozen lines.
fn multipart_body(fields: &[MultipartField]) -> Result<(String, Vec<u8>)> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let boundary = format!("----rp-boundary-{nanos:x}");
    let mut out: Vec<u8> = Vec::new();
    for f in fields {
        out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        out.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{}\"", f.name).as_bytes(),
        );
        let data = match &f.file_path {
            Some(p) => {
                let name = std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| p.clone());
                out.extend_from_slice(format!("; filename=\"{name}\"").as_bytes());
                std::fs::read(p).with_context(|| format!("reading multipart file {p}"))?
            }
            None => f.value.clone().into_bytes(),
        };
        out.extend_from_slice(b"\r\n");
        let ct = f.content_type.clone().or_else(|| {
            f.file_path
                .as_ref()
                .map(|_| "application/octet-stream".to_string())
        });
        if let Some(ct) = ct {
            out.extend_from_slice(format!("Content-Type: {ct}\r\n").as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&data);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok((boundary, out))
}

// --------------------------------------------------------------------- asserts

#[derive(Debug, Clone, PartialEq)]
pub struct AssertResult {
    pub expr: String,
    pub passed: bool,
    pub detail: String,
}

pub fn run_asserts(res: &Response, asserts: &[Assert]) -> Vec<AssertResult> {
    asserts
        .iter()
        .filter(|a| a.enabled)
        .map(|a| {
            let expr = format!("{} {} {}", a.expr.trim(), a.operator.trim(), a.value.trim())
                .trim_end()
                .to_string();
            let lhs = lhs_value(res, &a.expr);
            match eval(&lhs, a.operator.trim(), a.value.trim()) {
                Ok(()) => AssertResult {
                    expr,
                    passed: true,
                    detail: String::new(),
                },
                Err(detail) => AssertResult {
                    expr,
                    passed: false,
                    detail,
                },
            }
        })
        .collect()
}

/// `None` means "undefined" (the path does not exist), mirroring JS.
fn lhs_value(res: &Response, expr: &str) -> Option<Value> {
    let keys = parse_path(expr.trim());
    let mut it = keys.iter();
    if it.next().map(String::as_str) != Some("res") {
        return None;
    }
    let mut cur = match it.next().map(String::as_str)? {
        "status" | "statusCode" => Value::from(res.status),
        "statusText" => Value::from(res.status_text.clone()),
        "responseTime" => Value::from(res.duration_ms as u64),
        "size" => Value::from(res.size_bytes as u64),
        "headers" => {
            let name = it.next()?;
            let v = res
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.clone())?;
            Value::from(v)
        }
        "body" => {
            let text = String::from_utf8_lossy(&res.body).to_string();
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        }
        _ => return None,
    };
    for k in it {
        cur = match &cur {
            Value::Object(m) => m.get(k)?.clone(),
            Value::Array(a) => a.get(k.parse::<usize>().ok()?)?.clone(),
            _ => return None,
        };
    }
    Some(cur)
}

/// `a.b[0]["x.y"]` -> ["a","b","0","x.y"]
fn parse_path(path: &str) -> Vec<String> {
    let cs: Vec<char> = path.chars().collect();
    let (mut i, mut keys) = (0usize, Vec::new());
    while i < cs.len() {
        match cs[i] {
            '.' => i += 1,
            '[' => {
                i += 1;
                let quote = cs.get(i).copied().filter(|c| *c == '\'' || *c == '"');
                if let Some(q) = quote {
                    i += 1;
                    let mut k = String::new();
                    while i < cs.len() && cs[i] != q {
                        if cs[i] == '\\' && cs.get(i + 1) == Some(&q) {
                            k.push(q);
                            i += 2;
                        } else {
                            k.push(cs[i]);
                            i += 1;
                        }
                    }
                    i += 2; // closing quote + ']'
                    keys.push(k);
                } else {
                    let mut k = String::new();
                    while i < cs.len() && cs[i] != ']' {
                        k.push(cs[i]);
                        i += 1;
                    }
                    i += 1;
                    keys.push(k);
                }
            }
            _ => {
                let mut k = String::new();
                while i < cs.len() && cs[i] != '.' && cs[i] != '[' {
                    k.push(cs[i]);
                    i += 1;
                }
                keys.push(k);
            }
        }
    }
    keys
}

/// Coerce the assert's string rhs towards the lhs type, so `res.status eq 200`
/// works without the user quoting anything.
fn coerce(lhs: Option<&Value>, rhs: &str) -> Value {
    match lhs {
        Some(Value::String(_)) => Value::String(rhs.to_string()),
        _ => serde_json::from_str(rhs).unwrap_or_else(|_| Value::String(rhs.to_string())),
    }
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn len_of(v: &Value) -> Option<usize> {
    match v {
        Value::String(s) => Some(s.chars().count()),
        Value::Array(a) => Some(a.len()),
        Value::Object(m) => Some(m.len()),
        _ => None,
    }
}

fn show(v: Option<&Value>) -> String {
    match v {
        None => "undefined".into(),
        Some(v) => v.to_string(),
    }
}

fn list_items(lhs: Option<&Value>, rhs: &str) -> Vec<Value> {
    let inner = rhs
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(rhs);
    inner.split(',').map(|s| coerce(lhs, s.trim())).collect()
}

fn check(ok: bool, msg: impl Into<String>) -> Result<(), String> {
    if ok { Ok(()) } else { Err(msg.into()) }
}

fn eval(lhs: &Option<Value>, op: &str, rhs: &str) -> Result<(), String> {
    let l = lhs.as_ref();
    let op = if op.is_empty() { "eq" } else { op };
    let need_num = |v: Option<&Value>| {
        v.and_then(num)
            .ok_or_else(|| format!("{} is not a number", show(v)))
    };
    let as_str = |v: Option<&Value>| match v {
        Some(Value::String(s)) => Ok(s.clone()),
        other => Err(format!("{} is not a string", show(other))),
    };

    match op {
        "eq" => check(
            l == Some(&coerce(l, rhs)),
            format!("expected {} to equal {rhs}", show(l)),
        ),
        "neq" => check(
            l != Some(&coerce(l, rhs)),
            format!("expected {} not to equal {rhs}", show(l)),
        ),
        "gt" | "gte" | "lt" | "lte" => {
            let a = need_num(l)?;
            let b = num(&coerce(l, rhs)).ok_or_else(|| format!("{rhs} is not a number"))?;
            let ok = match op {
                "gt" => a > b,
                "gte" => a >= b,
                "lt" => a < b,
                _ => a <= b,
            };
            check(ok, format!("expected {a} {op} {b}"))
        }
        "in" => check(
            l.is_some_and(|v| list_items(l, rhs).contains(v)),
            format!("expected {} to be one of {rhs}", show(l)),
        ),
        "notIn" => check(
            !l.is_some_and(|v| list_items(l, rhs).contains(v)),
            format!("expected {} not to be one of {rhs}", show(l)),
        ),
        "contains" | "notContains" => {
            let hit = match l {
                Some(Value::String(s)) => s.contains(rhs),
                Some(Value::Array(a)) => a.contains(&coerce(None, rhs)),
                Some(Value::Object(m)) => m.contains_key(rhs),
                _ => return Err(format!("{} cannot contain anything", show(l))),
            };
            check(
                hit == (op == "contains"),
                format!("expected {} to {op} {rhs}", show(l)),
            )
        }
        "length" => {
            let want = rhs
                .trim()
                .parse::<usize>()
                .map_err(|_| format!("{rhs} is not a length"))?;
            let got = len_of(l.ok_or_else(|| "undefined has no length".to_string())?)
                .ok_or_else(|| format!("{} has no length", show(l)))?;
            check(got == want, format!("expected length {want}, got {got}"))
        }
        "matches" | "notMatches" => {
            let pat = rhs
                .strip_prefix('/')
                .and_then(|s| s.strip_suffix('/'))
                .unwrap_or(rhs);
            let text = match l {
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => return Err("undefined does not match".into()),
            };
            let hit = re_search(pat, &text);
            check(
                hit == (op == "matches"),
                format!("expected {text:?} to {op} /{pat}/"),
            )
        }
        "startsWith" => {
            let s = as_str(l)?;
            check(
                s.starts_with(rhs),
                format!("expected {s:?} to start with {rhs:?}"),
            )
        }
        "endsWith" => {
            let s = as_str(l)?;
            check(
                s.ends_with(rhs),
                format!("expected {s:?} to end with {rhs:?}"),
            )
        }
        "isEmpty" | "isNotEmpty" => {
            let empty = len_of(l.ok_or_else(|| "undefined is not empty-checkable".to_string())?)
                .ok_or_else(|| format!("{} is not empty-checkable", show(l)))?
                == 0;
            check(
                empty == (op == "isEmpty"),
                format!("expected {} to be {op}", show(l)),
            )
        }
        "isNull" => check(
            l == Some(&Value::Null),
            format!("expected {} to be null", show(l)),
        ),
        "isUndefined" => check(l.is_none(), format!("expected {} to be undefined", show(l))),
        "isDefined" => check(l.is_some(), "expected value to be defined"),
        // Bruno maps isTruthy/isFalsy onto chai's `.true`/`.false`, i.e. strict
        // booleans, not JS truthiness. Keep parity.
        "isTruthy" => check(
            l == Some(&Value::Bool(true)),
            format!("expected {} to be true", show(l)),
        ),
        "isFalsy" => check(
            l == Some(&Value::Bool(false)),
            format!("expected {} to be false", show(l)),
        ),
        "isJson" => check(
            matches!(l, Some(Value::Object(_)) | Some(Value::Array(_))),
            format!("expected {} to be JSON", show(l)),
        ),
        "isArray" => check(
            matches!(l, Some(Value::Array(_))),
            format!("expected {} to be an array", show(l)),
        ),
        "isNumber" => check(
            matches!(l, Some(Value::Number(_))),
            format!("expected {} to be a number", show(l)),
        ),
        "isString" => check(
            matches!(l, Some(Value::String(_))),
            format!("expected {} to be a string", show(l)),
        ),
        "isBoolean" => check(
            matches!(l, Some(Value::Bool(_))),
            format!("expected {} to be a boolean", show(l)),
        ),
        other => Err(format!("unknown assert operator {other}")),
    }
}

// ------------------------------------------------------------- tiny regex
// ponytail: backtracking matcher for `^ $ . * + ? [...] \d\w\s` and literals.
// No groups, alternation or {n,m} — pull in the `regex` crate the day an
// assert actually needs them.

#[derive(Debug)]
enum Item {
    Any,
    Lit(char),
    Class(Vec<(char, char)>, bool),
}

fn re_search(pat: &str, text: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = text.chars().collect();
    if p.first() == Some(&'^') {
        return re_here(&p[1..], &t);
    }
    (0..=t.len()).any(|i| re_here(&p, &t[i..]))
}

fn re_here(p: &[char], t: &[char]) -> bool {
    if p.is_empty() {
        return true;
    }
    if p.len() == 1 && p[0] == '$' {
        return t.is_empty();
    }
    let (item, rest) = parse_item(p);
    match rest.first() {
        Some('*') => re_repeat(&item, &rest[1..], t, 0),
        Some('+') => re_repeat(&item, &rest[1..], t, 1),
        Some('?') => {
            (!t.is_empty() && item_match(&item, t[0]) && re_here(&rest[1..], &t[1..]))
                || re_here(&rest[1..], t)
        }
        _ => !t.is_empty() && item_match(&item, t[0]) && re_here(rest, &t[1..]),
    }
}

fn re_repeat(item: &Item, rest: &[char], t: &[char], min: usize) -> bool {
    let mut i = 0usize;
    loop {
        if i >= min && re_here(rest, &t[i..]) {
            return true;
        }
        if i < t.len() && item_match(item, t[i]) {
            i += 1;
        } else {
            return false;
        }
    }
}

fn escape_item(c: char) -> Item {
    match c {
        'd' => Item::Class(vec![('0', '9')], false),
        'D' => Item::Class(vec![('0', '9')], true),
        'w' => Item::Class(vec![('0', '9'), ('a', 'z'), ('A', 'Z'), ('_', '_')], false),
        'W' => Item::Class(vec![('0', '9'), ('a', 'z'), ('A', 'Z'), ('_', '_')], true),
        's' => Item::Class(
            vec![(' ', ' '), ('\t', '\t'), ('\n', '\n'), ('\r', '\r')],
            false,
        ),
        'S' => Item::Class(
            vec![(' ', ' '), ('\t', '\t'), ('\n', '\n'), ('\r', '\r')],
            true,
        ),
        other => Item::Lit(other),
    }
}

fn parse_item(p: &[char]) -> (Item, &[char]) {
    match p[0] {
        '.' => (Item::Any, &p[1..]),
        '\\' if p.len() > 1 => (escape_item(p[1]), &p[2..]),
        '[' => {
            let mut i = 1;
            let neg = p.get(i) == Some(&'^');
            if neg {
                i += 1;
            }
            let mut ranges = Vec::new();
            while i < p.len() && p[i] != ']' {
                let c = if p[i] == '\\' && i + 1 < p.len() {
                    i += 1;
                    p[i]
                } else {
                    p[i]
                };
                if p.get(i + 1) == Some(&'-') && p.get(i + 2).is_some_and(|e| *e != ']') {
                    ranges.push((c, p[i + 2]));
                    i += 3;
                } else {
                    ranges.push((c, c));
                    i += 1;
                }
            }
            (Item::Class(ranges, neg), &p[(i + 1).min(p.len())..])
        }
        c => (Item::Lit(c), &p[1..]),
    }
}

fn item_match(item: &Item, c: char) -> bool {
    match item {
        Item::Any => true,
        Item::Lit(l) => *l == c,
        Item::Class(ranges, neg) => ranges.iter().any(|(a, b)| c >= *a && c <= *b) != *neg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn req(method: &str, url: &str) -> Request {
        Request {
            method: method.into(),
            url: url.into(),
            ..Default::default()
        }
    }

    /// A server that accepts a connection and then never answers used to hang
    /// the caller forever, because reqwest has no default timeout and the client
    /// set none. A request must always end.
    #[test]
    fn a_silent_server_times_out_instead_of_hanging() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        // Accept and then hold the connection open, answering nothing.
        std::thread::spawn(move || {
            let _held = listener.accept();
            std::thread::sleep(std::time::Duration::from_secs(30));
        });

        let mut r = req("GET", &format!("http://127.0.0.1:{port}/"));
        // The per-request override is what keeps this test fast; the point is
        // that a bound exists and is honoured.
        r.extra.insert("@timeout".into(), "0.5".into());

        let start = std::time::Instant::now();
        let err = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(send(&r, &Vars::new()))
            .unwrap_err();
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "it hung"
        );
        let msg = format!("{err:#}").to_lowercase();
        assert!(
            msg.contains("timed out") || msg.contains("timeout"),
            "{err:#}"
        );
    }

    #[test]
    fn timeout_directives_accept_the_jetbrains_unit_forms() {
        let secs = |s: &str| parse_timeout(s).unwrap().as_secs_f64();
        assert_eq!(secs("600"), 600.0); // bare number is seconds
        assert_eq!(secs("30 s"), 30.0);
        assert_eq!(secs("30s"), 30.0);
        assert_eq!(secs("2 m"), 120.0);
        assert_eq!(secs("500ms"), 0.5);
        assert_eq!(secs("1 hour"), 3600.0);
        assert_eq!(secs(" 2.5 "), 2.5);
        for bad in ["", "abc", "0", "-5", "NaN", "10 fortnights", "s"] {
            assert!(
                parse_timeout(bad).is_err(),
                "@timeout {bad:?} should be refused"
            );
        }
    }

    #[test]
    fn a_bad_timeout_directive_is_rejected_not_ignored() {
        let vars = Vars::new();
        for bad in ["", "abc", "0", "-5", "NaN"] {
            let mut r = req("GET", "http://127.0.0.1:1/");
            r.extra.insert("@timeout".into(), bad.into());
            assert!(
                build_request(&interp_request(&r, &vars)).is_err(),
                "@timeout {bad:?} should be refused"
            );
        }
        let mut r = req("GET", "http://127.0.0.1:1/");
        r.extra.insert("@timeout".into(), "2.5".into());
        assert!(build_request(&interp_request(&r, &vars)).is_ok());
    }

    #[test]
    fn unresolved_variables_are_named_not_swallowed() {
        let vars = Vars::from([("host".into(), "http://x".into())]);

        let mut r = req("GET", "{{baseUrl}}/api");
        r.headers
            .push(("Authorization".into(), "Bearer {{authToken}}".into()));
        let err = check_resolved(&r, &vars).unwrap_err().to_string();
        assert!(err.contains("baseUrl"), "{err}");
        assert!(err.contains("authToken"), "{err}");
        assert!(err.contains("client.global.set"), "{err}");

        // Every place a variable can hide is scanned, not just the URL.
        let mut r = req("POST", "http://x");
        r.body = Body::Json("{\"n\":\"{{userName}}\"}".into());
        assert!(
            check_resolved(&r, &vars)
                .unwrap_err()
                .to_string()
                .contains("userName")
        );

        // Fully resolved requests pass.
        assert!(check_resolved(&req("GET", "{{host}}/api"), &vars).is_ok());
    }

    #[test]
    fn send_reports_the_missing_variable_instead_of_a_builder_error() {
        // This is the exact failure the user hit on api.http.
        let r = req("GET", "{{baseUrl}}/api/v1/users");
        let err = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(send(&r, &Vars::new()))
            .unwrap_err()
            .to_string();
        assert!(err.contains("baseUrl"), "{err}");
        assert!(!err.contains("builder error"), "{err}");
    }

    fn body_str(r: &reqwest::Request) -> String {
        String::from_utf8_lossy(r.body().and_then(|b| b.as_bytes()).unwrap_or(b"")).to_string()
    }

    fn ct(r: &reqwest::Request) -> String {
        r.headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default()
    }

    // ------------------------------------------------------------ interpolation

    #[test]
    fn interpolates_everything_without_mutating_the_input() {
        let vars = Vars::from([
            ("host".into(), "http://h".into()),
            ("tok".into(), "T".into()),
            ("id".into(), "42".into()),
        ]);
        let original = Request {
            method: "POST".into(),
            url: "{{host}}/u/{{id}}".into(),
            headers: vec![("X-{{id}}".into(), "{{tok}}".into())],
            query: vec![("q".into(), "{{id}}".into())],
            body: Body::Json("{\"a\":\"{{id}}\"}".into()),
            auth: Auth::Bearer {
                token: "{{tok}}".into(),
            },
            ..Default::default()
        };
        let snapshot = original.clone();
        let i = interp_request(&original, &vars);

        assert_eq!(original, snapshot, "caller's Request was mutated");
        assert_eq!(i.url, "http://h/u/42");
        assert_eq!(i.headers, vec![("X-42".to_string(), "T".to_string())]);
        assert_eq!(i.query, vec![("q".to_string(), "42".to_string())]);
        assert_eq!(i.body, Body::Json("{\"a\":\"42\"}".into()));
        assert_eq!(i.auth, Auth::Bearer { token: "T".into() });
    }

    #[test]
    fn interpolated_values_land_in_the_built_request() {
        let vars = Vars::from([
            ("host".into(), "http://h.test".into()),
            ("v".into(), "9".into()),
        ]);
        let mut r = req("GET", "{{host}}/x");
        r.query = vec![("p".into(), "{{v}}".into())];
        let built = build_request(&interp_request(&r, &vars)).unwrap();
        assert_eq!(built.url().as_str(), "http://h.test/x?p=9");
    }

    // -------------------------------------------------------------------- auth

    #[test]
    fn basic_auth_is_base64_user_colon_pass() {
        let mut r = req("GET", "http://h.test/");
        r.auth = Auth::Basic {
            username: "u".into(),
            password: "p:w".into(),
        };
        let built = build_request(&r).unwrap();
        // base64("u:p:w")
        assert_eq!(built.headers()["authorization"], "Basic dTpwOnc=");
    }

    #[test]
    fn bearer_and_apikey_header_and_query() {
        let mut r = req("GET", "http://h.test/");
        r.auth = Auth::Bearer {
            token: "abc".into(),
        };
        assert_eq!(
            build_request(&r).unwrap().headers()["authorization"],
            "Bearer abc"
        );

        r.auth = Auth::ApiKey {
            key: "X-Key".into(),
            value: "k1".into(),
            placement: ApiKeyPlacement::Header,
        };
        let built = build_request(&r).unwrap();
        assert_eq!(built.headers()["x-key"], "k1");
        assert_eq!(built.url().query(), None);

        r.auth = Auth::ApiKey {
            key: "api_key".into(),
            value: "k1".into(),
            placement: ApiKeyPlacement::Query,
        };
        let built = build_request(&r).unwrap();
        assert_eq!(built.url().query(), Some("api_key=k1"));
        assert!(built.headers().get("api_key").is_none());
    }

    #[test]
    fn none_and_inherit_add_nothing() {
        for auth in [Auth::None, Auth::Inherit] {
            let mut r = req("GET", "http://h.test/");
            r.auth = auth;
            let built = build_request(&r).unwrap();
            assert!(built.headers().get("authorization").is_none());
            assert_eq!(built.url().query(), None);
        }
    }

    // -------------------------------------------------------------------- body

    #[test]
    fn body_variants_map_to_content_types() {
        let cases = [
            (Body::Json("{}".into()), "application/json", "{}"),
            (Body::Text("hi".into()), "text/plain", "hi"),
            (Body::Xml("<a/>".into()), "application/xml", "<a/>"),
            (
                Body::Sparql("SELECT".into()),
                "application/sparql-query",
                "SELECT",
            ),
        ];
        for (body, want_ct, want_body) in cases {
            let mut r = req("POST", "http://h.test/");
            r.body = body;
            let built = build_request(&r).unwrap();
            assert_eq!(ct(&built), want_ct);
            assert_eq!(body_str(&built), want_body);
        }
    }

    #[test]
    fn user_content_type_wins() {
        let mut r = req("POST", "http://h.test/");
        r.headers = vec![("Content-Type".into(), "application/vnd.me+json".into())];
        r.body = Body::Json("{}".into());
        assert_eq!(ct(&build_request(&r).unwrap()), "application/vnd.me+json");
    }

    #[test]
    fn form_body_is_urlencoded() {
        let mut r = req("POST", "http://h.test/");
        r.body = Body::Form(vec![("a".into(), "1 2".into()), ("b".into(), "&".into())]);
        let built = build_request(&r).unwrap();
        assert_eq!(ct(&built), "application/x-www-form-urlencoded");
        assert_eq!(body_str(&built), "a=1+2&b=%26");
    }

    #[test]
    fn graphql_body_is_query_plus_variables() {
        let mut r = req("POST", "http://h.test/");
        r.body = Body::GraphQl {
            query: "{me}".into(),
            variables: "{\"x\":1}".into(),
        };
        let built = build_request(&r).unwrap();
        assert_eq!(ct(&built), "application/json");
        let v: Value = serde_json::from_str(&body_str(&built)).unwrap();
        assert_eq!(v["query"], "{me}");
        assert_eq!(v["variables"]["x"], 1);

        r.body = Body::GraphQl {
            query: "{me}".into(),
            variables: String::new(),
        };
        let v: Value = serde_json::from_str(&body_str(&build_request(&r).unwrap())).unwrap();
        assert!(v["variables"].is_null());
    }

    #[test]
    fn file_body_is_read_from_disk() {
        let p = std::env::temp_dir().join("rp_core_exec_file_body.txt");
        std::fs::write(&p, b"payload").unwrap();
        let mut r = req("PUT", "http://h.test/");
        r.body = Body::File {
            path: p.to_string_lossy().to_string(),
            content_type: "text/csv".into(),
        };
        let built = build_request(&r).unwrap();
        assert_eq!(ct(&built), "text/csv");
        assert_eq!(body_str(&built), "payload");
    }

    #[test]
    fn multipart_carries_values_and_file_parts() {
        let p = std::env::temp_dir().join("rp_core_exec_multipart.txt");
        std::fs::write(&p, b"FILEDATA").unwrap();
        let mut r = req("POST", "http://h.test/");
        r.body = Body::MultipartForm(vec![
            MultipartField {
                name: "field".into(),
                value: "val".into(),
                file_path: None,
                content_type: None,
            },
            MultipartField {
                name: "upload".into(),
                value: String::new(),
                file_path: Some(p.to_string_lossy().to_string()),
                content_type: None,
            },
        ]);
        let built = build_request(&r).unwrap();
        let content_type = ct(&built);
        let boundary = content_type.split("boundary=").nth(1).unwrap().to_string();
        assert!(content_type.starts_with("multipart/form-data; boundary=----rp-boundary-"));
        let b = body_str(&built);
        assert!(b.contains(&format!("--{boundary}\r\n")));
        assert!(b.contains("Content-Disposition: form-data; name=\"field\"\r\n\r\nval"));
        assert!(b.contains("name=\"upload\"; filename=\"rp_core_exec_multipart.txt\""));
        assert!(b.contains("Content-Type: application/octet-stream"));
        assert!(b.contains("FILEDATA"));
        assert!(b.ends_with(&format!("--{boundary}--\r\n")));
    }

    #[test]
    fn bad_method_is_an_error() {
        assert!(build_request(&req("BAD METHOD", "http://h.test/")).is_err());
    }

    // ------------------------------------------------------------------ oauth2

    #[tokio::test]
    async fn oauth2_authorization_code_bails_with_a_hint() {
        let mut r = req("GET", "http://h.test/");
        r.auth = Auth::OAuth2(Box::new(crate::model::OAuth2 {
            grant_type: "authorization_code".into(),
            ..Default::default()
        }));
        let err = resolve_auth(&r).await.unwrap_err().to_string();
        assert!(err.contains("browser"), "{err}");
    }

    #[tokio::test]
    async fn oauth2_password_grant_needs_credentials() {
        let mut r = req("GET", "http://h.test/");
        r.auth = Auth::OAuth2(Box::new(crate::model::OAuth2 {
            grant_type: "password".into(),
            ..Default::default()
        }));
        assert!(
            resolve_auth(&r)
                .await
                .unwrap_err()
                .to_string()
                .contains("username")
        );
    }

    #[tokio::test]
    async fn oauth2_client_credentials_fetches_a_bearer_token() {
        let (addr, _h) = canned_server(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 26\r\n\r\n{\"access_token\":\"tok123\"}\n",
            1,
        );
        let mut r = req("GET", "http://h.test/");
        r.auth = Auth::OAuth2(Box::new(crate::model::OAuth2 {
            grant_type: "client_credentials".into(),
            access_token_url: format!("http://{addr}/token"),
            client_id: "cid".into(),
            client_secret: "sec".into(),
            ..Default::default()
        }));
        assert_eq!(
            resolve_auth(&r).await.unwrap(),
            Auth::Bearer {
                token: "tok123".into()
            }
        );
    }

    // -------------------------------------------------------------------- send

    /// Throwaway blocking listener that answers `n` connections with `resp`.
    fn canned_server(resp: &'static str, n: usize) -> (String, std::thread::JoinHandle<()>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap().to_string();
        let h = std::thread::spawn(move || {
            for _ in 0..n {
                let Ok((mut s, _)) = l.accept() else { return };
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let _ = s.write_all(resp.as_bytes());
                let _ = s.flush();
            }
        });
        (addr, h)
    }

    #[tokio::test]
    async fn send_populates_the_response() {
        let (addr, _h) = canned_server(
            "HTTP/1.1 201 Created\r\nX-Test: yes\r\nContent-Length: 13\r\n\r\nhello, world!",
            1,
        );
        let vars = Vars::from([("host".into(), format!("http://{addr}"))]);
        let mut r = req("POST", "{{host}}/echo");
        r.body = Body::Json("{\"a\":1}".into());

        let res = send(&r, &vars).await.unwrap();
        assert_eq!(res.status, 201);
        assert_eq!(res.status_text, "Created");
        assert_eq!(res.body, b"hello, world!");
        assert_eq!(res.size_bytes, 13);
        assert!(res.duration_ms < 60_000);
        assert!(
            res.headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("x-test") && v == "yes"),
            "{:?}",
            res.headers
        );
    }

    // ----------------------------------------------------------------- asserts

    fn res_fixture() -> Response {
        let body = br#"{"name":"bruno","n":7,"flag":true,"nil":null,"empty":"","list":[1,2,3],"obj":{"a":{"b":"deep"}}}"#;
        Response {
            status: 200,
            status_text: "OK".into(),
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.to_vec(),
            duration_ms: 120,
            size_bytes: body.len(),
        }
    }

    fn a(expr: &str, operator: &str, value: &str) -> Assert {
        Assert {
            expr: expr.into(),
            operator: operator.into(),
            value: value.into(),
            enabled: true,
        }
    }

    fn one(expr: &str, op: &str, val: &str) -> AssertResult {
        run_asserts(&res_fixture(), &[a(expr, op, val)]).remove(0)
    }

    #[test]
    fn asserts_pass_for_every_operator() {
        let pass = [
            ("res.status", "eq", "200"),
            ("res.status", "neq", "500"),
            ("res.status", "gt", "199"),
            ("res.status", "gte", "200"),
            ("res.status", "lt", "300"),
            ("res.status", "lte", "200"),
            ("res.status", "in", "[200, 201]"),
            ("res.status", "notIn", "400,500"),
            ("res.responseTime", "lt", "1000"),
            ("res.headers.content-type", "contains", "json"),
            ("res.headers.Content-Type", "eq", "application/json"),
            ("res.headers.content-type", "notContains", "xml"),
            ("res.headers.content-type", "startsWith", "application/"),
            ("res.headers.content-type", "endsWith", "json"),
            ("res.headers.content-type", "matches", "^application/js?on$"),
            ("res.headers.content-type", "notMatches", "^xml"),
            ("res.body.name", "eq", "bruno"),
            ("res.body.name", "length", "5"),
            ("res.body.n", "isNumber", ""),
            ("res.body.name", "isString", ""),
            ("res.body.flag", "isBoolean", ""),
            ("res.body.flag", "isTruthy", ""),
            ("res.body.nil", "isNull", ""),
            ("res.body.nil", "isDefined", ""),
            ("res.body.missing", "isUndefined", ""),
            ("res.body.empty", "isEmpty", ""),
            ("res.body.name", "isNotEmpty", ""),
            ("res.body", "isJson", ""),
            ("res.body.list", "isArray", ""),
            ("res.body.list", "length", "3"),
            ("res.body.list", "contains", "2"),
            ("res.body.list[0]", "eq", "1"),
            ("res.body.obj.a.b", "eq", "deep"),
            ("res.body[\"obj\"].a.b", "eq", "deep"),
            ("res.body.n", "matches", "^\\d+$"),
        ];
        for (e, o, v) in pass {
            let r = one(e, o, v);
            assert!(r.passed, "{e} {o} {v} -> {}", r.detail);
        }
    }

    #[test]
    fn asserts_fail_with_detail() {
        let fail = [
            ("res.status", "eq", "404"),
            ("res.status", "neq", "200"),
            ("res.status", "gt", "200"),
            ("res.status", "gte", "201"),
            ("res.status", "lt", "200"),
            ("res.status", "lte", "199"),
            ("res.status", "in", "[301,302]"),
            ("res.status", "notIn", "200,201"),
            ("res.body.name", "contains", "xyz"),
            ("res.body.name", "notContains", "bru"),
            ("res.body.name", "length", "9"),
            ("res.body.name", "matches", "^x"),
            ("res.body.name", "notMatches", "^bru"),
            ("res.body.name", "startsWith", "x"),
            ("res.body.name", "endsWith", "x"),
            ("res.body.name", "isEmpty", ""),
            ("res.body.empty", "isNotEmpty", ""),
            ("res.body.name", "isNull", ""),
            ("res.body.name", "isUndefined", ""),
            ("res.body.missing", "isDefined", ""),
            ("res.body.n", "isTruthy", ""),
            ("res.body.flag", "isFalsy", ""),
            ("res.body.name", "isJson", ""),
            ("res.body.name", "isNumber", ""),
            ("res.body.n", "isString", ""),
            ("res.body.n", "isBoolean", ""),
            ("res.body.n", "isArray", ""),
            ("res.status", "nonsense", "1"),
        ];
        for (e, o, v) in fail {
            let r = one(e, o, v);
            assert!(!r.passed, "{e} {o} {v} unexpectedly passed");
            assert!(!r.detail.is_empty(), "{e} {o} {v} has no detail");
        }
        assert!(!one("res.body.n", "isFalsy", "").passed);
    }

    #[test]
    fn missing_paths_and_unknown_roots_are_undefined_not_panics() {
        for expr in [
            "res.body.nope.deeper",
            "res.body.list[9]",
            "res.body.name.x",
            "res.headers.x-absent",
            "res.whatever",
            "req.url",
            "",
        ] {
            let r = one(expr, "eq", "1");
            assert!(!r.passed, "{expr}");
            assert!(r.detail.contains("undefined"), "{expr} -> {}", r.detail);
        }
    }

    #[test]
    fn assert_result_carries_expression_and_skips_disabled() {
        let res = res_fixture();
        let out = run_asserts(
            &res,
            &[
                a("res.status", "eq", "200"),
                Assert {
                    enabled: false,
                    ..a("res.status", "eq", "999")
                },
            ],
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].expr, "res.status eq 200");
        assert!(out[0].passed);
        assert!(run_asserts(&res, &[]).is_empty());
    }

    #[test]
    fn mini_regex_handles_the_supported_subset() {
        assert!(re_search("^a.c$", "abc"));
        assert!(!re_search("^a.c$", "abcd"));
        assert!(re_search("ab*c", "ac"));
        assert!(!re_search("ab+c", "ac"));
        assert!(re_search("ab?c", "abc"));
        assert!(re_search("[0-9]+", "x42"));
        assert!(!re_search("^[0-9]+$", "x42"));
        assert!(re_search("[^0-9]", "a"));
        assert!(re_search("\\w+\\s\\d", "ab 1"));
        assert!(re_search("a\\.c", "a.c"));
        assert!(!re_search("a\\.c", "abc"));
    }
}
