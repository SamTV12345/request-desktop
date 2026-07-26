//! Differential test: our `.bru` parser vs Bruno's own JavaScript parser.
//!
//! Unit tests only prove the parser agrees with its author. This one runs the
//! real `@usebruno/lang` (pinned in `scripts/bru_reference/package.json`) over
//! the same corpus and compares field by field. Bruno emits its own JSON
//! schema, we produce `model::Request`, so both sides are mapped into one
//! canonical form by `canon_ours` / `canon_bruno` below.
//!
//! Run it with `RP_DIFFERENTIAL=1 cargo test -p rp-core --test differential_bru`.
//! Without the env var (or without Node / `npm install` in the harness) the
//! test prints a skip notice and passes, so `cargo test` works for a
//! contributor with no Node toolchain. The notice is printed unconditionally —
//! `cargo test -- --nocapture` shows it — so a permanently-skipping test cannot
//! rot unnoticed; CI (`.github/workflows/differential.yml`) always sets the var.
//!
//! # Fields compared
//!
//! `name`, `meta.type`, `seq`, `method`, `url`, declared `auth` mode, declared
//! `body` mode, query params (name/value/**enabled**), path params
//! (name/value/enabled), headers (name/value/**enabled**), effective body kind
//! *and* its content (json / text / xml / sparql text; graphql query and
//! variables; form-urlencoded and multipart pairs including per-field
//! `contentType` and file-vs-text type; `body:file` path and contentType),
//! auth kind and the fields of that kind, asserts (name / raw value /
//! enabled), `script:pre-request`, `script:post-response`, `tests`, `docs`.
//!
//! For files Bruno rejects outright, agreement means "we reject too"; those are
//! counted separately.
//!
//! # Named exclusions
//!
//! Each is a place our model deliberately cannot represent something Bruno
//! emits. They are individually scoped — there is no general "ignore
//! mismatches" switch — and each occurrence is counted and printed, so the
//! report doubles as an audit of where we diverge from Bruno by design.
//!
//! * `X_ANNOTATIONS` — per-pair `@description(...)` / `@number` decorators.
//!   Bruno attaches `description` / `dataType` / `annotations` to the pair;
//!   `Vec<(String, String)>` has nowhere to put them, so we drop them. We
//!   still compare name/value/enabled of the same pair.
//! * `X_MULTIPART_MULTIFILE` — Bruno splits `@file(a|b)` into a list of paths;
//!   `MultipartField::file_path` is one `String`, so we keep the raw `a|b`.
//! * `X_FILE_BODY_EXTRA` — Bruno keeps every `body:file` entry; `Body::File`
//!   keeps only the selected one (first enabled, else first). Extra entries
//!   are not compared.
//! * `X_OAUTH2_FIELDS` — `model::OAuth2` has seven fields; Bruno emits up to
//!   nineteen (pkce, token_placement, ...). We compare the intersection; the
//!   rest live untyped in `extra["auth:oauth2.*"]`.
//! * `X_UNMODELED_AUTH` — awsv4 / digest / ntlm / oauth1 / wsse /
//!   akamai-edgegrid have no `Auth` variant and go to `extra["auth:<kind>"]`
//!   verbatim. We assert the kind was recognised as unmodeled-but-preserved,
//!   not its individual fields.
//! * `X_VARS` — `vars:pre-request` / `vars:post-response` are kept as opaque
//!   block text in `extra`, not as typed pairs, so Bruno's `vars.req` /
//!   `vars.res` (with `local`, `dataType`) is not compared.
//! * `X_SETTINGS` — same: `settings` is opaque text on our side while Bruno
//!   parses and defaults it into typed fields.
//! * Collection/folder/environment files (`collection.bru`, `folder.bru`,
//!   `environments/*.bru`) are skipped entirely: Bruno parses those with
//!   `collectionBruToJson` / `envToJson`, i.e. a *different* reference
//!   implementation than the one under test here.

use rp_core::bru;
use rp_core::model::{ApiKeyPlacement, Auth, Body, Request};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

// --- exclusion counters ----------------------------------------------------

macro_rules! exclusions {
    ($($id:ident => $label:literal),* $(,)?) => {
        $(static $id: AtomicUsize = AtomicUsize::new(0);)*
        const ALL_EXCLUSIONS: &[(&str, &AtomicUsize)] = &[$(($label, &$id)),*];
    };
}

exclusions! {
    X_ANNOTATIONS        => "per-pair @description/@dataType annotations dropped",
    X_MULTIPART_MULTIFILE=> "multipart @file(a|b) multi-path not split",
    X_FILE_BODY_EXTRA    => "body:file entries beyond the selected one dropped",
    X_OAUTH2_FIELDS      => "oauth2 fields outside model::OAuth2's seven",
    X_UNMODELED_AUTH     => "auth kind with no Auth variant (kept in extra)",
    X_VARS               => "vars:pre-request/post-response kept as opaque text",
    X_SETTINGS           => "settings block kept as opaque text",
    X_SEQ_NON_NUMERIC    => "non-numeric meta.seq (Request::seq is u32)",
}

fn hit(c: &AtomicUsize) {
    c.fetch_add(1, Ordering::Relaxed);
}

// --- small json helpers ----------------------------------------------------

fn get<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = v;
    for k in path {
        cur = cur.get(k)?;
    }
    Some(cur)
}

fn str_at(v: &Value, path: &[&str]) -> String {
    get(v, path)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// `[name, value, enabled]` — the shape every dictionary block reduces to.
fn triple(name: &str, value: &str, enabled: bool) -> Value {
    json!([name, value, enabled])
}

/// Splits our `~name` marker off a key.
fn split_disabled(k: &str) -> (&str, bool) {
    match k.strip_prefix('~') {
        Some(n) => (n, false),
        None => (k, true),
    }
}

/// Bruno pair list -> `[name, value, enabled]` triples.
fn bruno_pairs(list: Option<&Value>) -> Value {
    let Some(arr) = list.and_then(Value::as_array) else {
        return json!([]);
    };
    Value::Array(
        arr.iter()
            .map(|p| {
                if p.get("description").is_some()
                    || p.get("dataType").is_some()
                    || p.get("annotations").is_some()
                {
                    hit(&X_ANNOTATIONS);
                }
                triple(
                    p.get("name").and_then(Value::as_str).unwrap_or(""),
                    p.get("value").and_then(Value::as_str).unwrap_or(""),
                    p.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                )
            })
            .collect(),
    )
}

fn our_pairs(items: &[(String, String)]) -> Value {
    Value::Array(
        items
            .iter()
            .map(|(k, v)| {
                let (name, enabled) = split_disabled(k);
                triple(name, v, enabled)
            })
            .collect(),
    )
}

/// Our `extra["params:path"]` / `extra["vars:*"]` are raw block text; turn the
/// `key: value` lines back into triples so they can be compared.
fn raw_block_pairs(raw: Option<&String>) -> Value {
    let Some(raw) = raw else { return json!([]) };
    Value::Array(
        raw.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('@'))
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| {
                let (name, enabled) = split_disabled(k.trim());
                triple(name, v.trim(), enabled)
            })
            .collect(),
    )
}

// --- canonical form: Bruno side -------------------------------------------

/// Bruno's `params` array carries both query and path params, tagged by `type`.
fn bruno_params(j: &Value, kind: &str) -> Value {
    let filtered: Vec<Value> = j
        .get("params")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter(|p| p.get("type").and_then(Value::as_str) == Some(kind))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    bruno_pairs(Some(&Value::Array(filtered)))
}

fn bruno_multipart(list: &Value) -> Value {
    let Some(arr) = list.as_array() else {
        return json!([]);
    };
    Value::Array(
        arr.iter()
            .map(|p| {
                let ty = p.get("type").and_then(Value::as_str).unwrap_or("text");
                let value = match p.get("value") {
                    // File fields arrive as a list of paths.
                    Some(Value::Array(paths)) => {
                        if paths.len() > 1 {
                            hit(&X_MULTIPART_MULTIFILE);
                        }
                        paths
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join("|")
                    }
                    Some(Value::String(s)) => s.clone(),
                    _ => String::new(),
                };
                if p.get("description").is_some() || p.get("annotations").is_some() {
                    hit(&X_ANNOTATIONS);
                }
                json!([
                    p.get("name").and_then(Value::as_str).unwrap_or(""),
                    value,
                    p.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                    ty,
                    p.get("contentType").and_then(Value::as_str).unwrap_or(""),
                ])
            })
            .collect(),
    )
}

/// `body:file` -> the one entry `Body::File` would keep: first `selected`,
/// else first.
fn bruno_file_body(list: &Value) -> Value {
    let Some(arr) = list.as_array() else {
        return Value::Null;
    };
    if arr.len() > 1 {
        hit(&X_FILE_BODY_EXTRA);
    }
    let pick = arr
        .iter()
        .find(|e| e.get("selected").and_then(Value::as_bool) == Some(true))
        .or_else(|| arr.first());
    match pick {
        Some(e) => json!([
            e.get("filePath").and_then(Value::as_str).unwrap_or(""),
            e.get("contentType").and_then(Value::as_str).unwrap_or(""),
        ]),
        None => json!(["", ""]),
    }
}

/// The `Body` variant Bruno's output implies, plus its content. `mode` is the
/// declared `body:` from the verb block.
fn bruno_body(j: &Value, mode: &str) -> (String, Value) {
    let body = j.get("body");
    let content = |k: &str| body.and_then(|b| b.get(k)).cloned();
    match mode {
        "json" | "text" | "xml" | "sparql" => match content(mode) {
            Some(Value::String(s)) => (mode.into(), Value::String(s)),
            _ => ("none".into(), Value::Null),
        },
        "graphql" => match content("graphql") {
            Some(g) => (
                "graphql".into(),
                json!([str_at(&g, &["query"]), str_at(&g, &["variables"])]),
            ),
            None => ("none".into(), Value::Null),
        },
        "formUrlEncoded" => match content("formUrlEncoded") {
            Some(l) => ("form".into(), bruno_pairs(Some(&l))),
            None => ("none".into(), Value::Null),
        },
        "multipartForm" => match content("multipartForm") {
            Some(l) => ("multipartForm".into(), bruno_multipart(&l)),
            None => ("none".into(), Value::Null),
        },
        "file" => match content("file") {
            Some(l) => ("file".into(), bruno_file_body(&l)),
            None => ("none".into(), Value::Null),
        },
        _ => ("none".into(), Value::Null),
    }
}

const MODELED_AUTH: &[&str] = &["basic", "bearer", "apikey", "oauth2"];
const UNMODELED_AUTH: &[&str] = &[
    "awsv4",
    "digest",
    "ntlm",
    "oauth1",
    "wsse",
    "akamai-edgegrid",
];
/// The seven `model::OAuth2` fields, as Bruno spells them.
const OAUTH2_MODELED: &[&str] = &[
    "grantType",
    "accessTokenUrl",
    "authorizationUrl",
    "clientId",
    "clientSecret",
    "scope",
    "callbackUrl",
];

fn bruno_auth(j: &Value, mode: &str) -> (String, Value) {
    let block = |k: &str| get(j, &["auth", k]).cloned();
    match mode {
        "inherit" => ("inherit".into(), Value::Null),
        m if MODELED_AUTH.contains(&m) => match block(m) {
            Some(a) => {
                let mut out = Map::new();
                match m {
                    "basic" => {
                        out.insert("username".into(), json!(str_at(&a, &["username"])));
                        out.insert("password".into(), json!(str_at(&a, &["password"])));
                    }
                    "bearer" => {
                        out.insert("token".into(), json!(str_at(&a, &["token"])));
                    }
                    "apikey" => {
                        out.insert("key".into(), json!(str_at(&a, &["key"])));
                        out.insert("value".into(), json!(str_at(&a, &["value"])));
                        // Our `ApiKeyPlacement` only distinguishes queryparams
                        // from header, so normalise Bruno's raw string the same
                        // way: anything that is not `queryparams` is a header.
                        let p = str_at(&a, &["placement"]);
                        out.insert(
                            "placement".into(),
                            json!(if p == "queryparams" {
                                "queryparams"
                            } else {
                                "header"
                            }),
                        );
                    }
                    _ => {
                        let obj = a.as_object().cloned().unwrap_or_default();
                        if obj.keys().any(|k| !OAUTH2_MODELED.contains(&k.as_str())) {
                            hit(&X_OAUTH2_FIELDS);
                        }
                        // X_OAUTH2_FIELDS: only the intersection is compared.
                        for k in OAUTH2_MODELED {
                            if let Some(v) = obj.get(*k) {
                                out.insert((*k).into(), v.clone());
                            }
                        }
                    }
                }
                (m.into(), Value::Object(out))
            }
            None => ("none".into(), Value::Null),
        },
        m if UNMODELED_AUTH.contains(&m) && block(m).is_some() => {
            hit(&X_UNMODELED_AUTH);
            // X_UNMODELED_AUTH: we only assert the kind was preserved.
            (format!("unmodeled:{m}"), Value::Null)
        }
        _ => ("none".into(), Value::Null),
    }
}

fn canon_bruno(j: &Value) -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    let mode_body = str_at(j, &["http", "body"]);
    let mode_auth = str_at(j, &["http", "auth"]);

    m.insert("name".into(), json!(str_at(j, &["meta", "name"])));
    // Bruno only applies its `type`/`seq` defaults when a meta block exists;
    // with no meta block it emits nothing at all. Apply the same defaults so a
    // missing block is not reported as a difference of opinion.
    m.insert(
        "meta.type".into(),
        match j.get("meta") {
            None => json!("http"),
            Some(meta) => json!(str_at(meta, &["type"])),
        },
    );
    // meta.seq is a string in Bruno's output; absent -> its own default of 1.
    m.insert(
        "seq".into(),
        match get(j, &["meta", "seq"]) {
            None => json!(1u64),
            Some(v) => match v.as_str().unwrap_or_default().trim().parse::<u64>() {
                Ok(n) => json!(n),
                // X_SEQ_NON_NUMERIC: `Request::seq` is a u32, so a non-numeric
                // seq cannot be stored; not compared.
                Err(_) => {
                    hit(&X_SEQ_NON_NUMERIC);
                    Value::Null
                }
            },
        },
    );
    m.insert(
        "method".into(),
        json!(str_at(j, &["http", "method"]).to_uppercase()),
    );
    m.insert("url".into(), json!(str_at(j, &["http", "url"])));
    m.insert("body.mode".into(), json!(mode_body));
    m.insert("auth.mode".into(), json!(mode_auth));
    m.insert("query".into(), bruno_params(j, "query"));
    m.insert("params.path".into(), bruno_params(j, "path"));
    m.insert("headers".into(), bruno_pairs(j.get("headers")));

    let (kind, content) = bruno_body(j, &mode_body);
    m.insert("body.kind".into(), json!(kind));
    m.insert("body.content".into(), content);

    let (kind, fields) = bruno_auth(j, &mode_auth);
    m.insert("auth.kind".into(), json!(kind));
    m.insert("auth.fields".into(), fields);

    m.insert("asserts".into(), bruno_pairs(j.get("assertions")));
    m.insert(
        "script.req".into(),
        get(j, &["script", "req"]).cloned().unwrap_or(Value::Null),
    );
    m.insert(
        "script.res".into(),
        get(j, &["script", "res"]).cloned().unwrap_or(Value::Null),
    );
    m.insert(
        "tests".into(),
        j.get("tests").cloned().unwrap_or(Value::Null),
    );
    m.insert("docs".into(), j.get("docs").cloned().unwrap_or(Value::Null));

    if get(j, &["vars", "req"]).is_some() || get(j, &["vars", "res"]).is_some() {
        hit(&X_VARS);
    }
    if j.get("settings").is_some() {
        hit(&X_SETTINGS);
    }
    m
}

// --- canonical form: our side ---------------------------------------------

fn our_body(r: &Request) -> (String, Value) {
    match &r.body {
        Body::None => ("none".into(), Value::Null),
        Body::Json(s) => ("json".into(), json!(s)),
        Body::Text(s) => ("text".into(), json!(s)),
        Body::Xml(s) => ("xml".into(), json!(s)),
        Body::Sparql(s) => ("sparql".into(), json!(s)),
        Body::GraphQl { query, variables } => ("graphql".into(), json!([query, variables])),
        Body::Form(items) => ("form".into(), our_pairs(items)),
        Body::MultipartForm(fields) => (
            "multipartForm".into(),
            Value::Array(
                fields
                    .iter()
                    .map(|f| {
                        let (name, enabled) = split_disabled(&f.name);
                        let (ty, value) = match &f.file_path {
                            Some(p) => ("file", p.clone()),
                            None => ("text", f.value.clone()),
                        };
                        json!([
                            name,
                            value,
                            enabled,
                            ty,
                            f.content_type.clone().unwrap_or_default()
                        ])
                    })
                    .collect(),
            ),
        ),
        Body::File { path, content_type } => ("file".into(), json!([path, content_type])),
    }
}

fn our_auth(r: &Request) -> (String, Value) {
    match &r.auth {
        Auth::Inherit => ("inherit".into(), Value::Null),
        Auth::Basic { username, password } => (
            "basic".into(),
            json!({ "username": username, "password": password }),
        ),
        Auth::Bearer { token } => ("bearer".into(), json!({ "token": token })),
        Auth::ApiKey {
            key,
            value,
            placement,
        } => (
            "apikey".into(),
            json!({
                "key": key,
                "value": value,
                "placement": match placement {
                    ApiKeyPlacement::Query => "queryparams",
                    ApiKeyPlacement::Header => "header",
                },
            }),
        ),
        Auth::OAuth2(o) => {
            let all = json!({
                "grantType": o.grant_type,
                "accessTokenUrl": o.access_token_url,
                "authorizationUrl": o.authorization_url,
                "clientId": o.client_id,
                "clientSecret": o.client_secret,
                "scope": o.scope,
                "callbackUrl": o.callback_url,
            });
            ("oauth2".into(), all)
        }
        // Auth::None covers both "no auth" and "an auth kind we cannot model" —
        // the latter is recognisable by the verbatim copy left in `extra`.
        Auth::None => {
            let mode = r.extra.get("http.auth").cloned().unwrap_or_default();
            if r.extra.contains_key(&format!("auth:{mode}")) {
                (format!("unmodeled:{mode}"), Value::Null)
            } else {
                ("none".into(), Value::Null)
            }
        }
    }
}

fn canon_ours(r: &Request) -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    m.insert("name".into(), json!(r.name));
    m.insert(
        "meta.type".into(),
        json!(r.extra.get("meta.type").cloned().unwrap_or_default()),
    );
    m.insert("seq".into(), json!(r.seq as u64));
    m.insert("method".into(), json!(r.method.to_uppercase()));
    m.insert("url".into(), json!(r.url));
    m.insert(
        "body.mode".into(),
        json!(r.extra.get("http.body").cloned().unwrap_or_default()),
    );
    m.insert(
        "auth.mode".into(),
        json!(r.extra.get("http.auth").cloned().unwrap_or_default()),
    );
    m.insert("query".into(), our_pairs(&r.query));
    m.insert(
        "params.path".into(),
        raw_block_pairs(r.extra.get("params:path")),
    );
    m.insert("headers".into(), our_pairs(&r.headers));

    let (kind, content) = our_body(r);
    m.insert("body.kind".into(), json!(kind));
    m.insert("body.content".into(), content);

    let (kind, fields) = our_auth(r);
    m.insert("auth.kind".into(), json!(kind));
    m.insert("auth.fields".into(), fields);

    // Bruno keeps the assert value raw ("eq 200"); we split it into
    // operator + value. Re-join so the comparison checks our split is a
    // faithful decomposition. A bare value is only allowed when the operator
    // we inferred is the implicit `eq`.
    m.insert(
        "asserts".into(),
        Value::Array(
            r.asserts
                .iter()
                .map(|a| {
                    json!([
                        a.expr,
                        format!("{} {}", a.operator, a.value).trim().to_string(),
                        a.enabled
                    ])
                })
                .collect(),
        ),
    );
    m.insert("script.req".into(), opt(&r.pre_request_script));
    m.insert("script.res".into(), opt(&r.post_response_script));
    m.insert("tests".into(), opt(&r.tests));
    m.insert("docs".into(), opt(&r.extra.get("docs").cloned()));
    m
}

fn opt(v: &Option<String>) -> Value {
    v.as_ref().map(|s| json!(s)).unwrap_or(Value::Null)
}

/// X_OAUTH2_FIELDS: Bruno's `oauth2` object is grant-type specific — a
/// `client_credentials` block has no `authorizationUrl`/`callbackUrl` key even
/// when the file supplies one, while `model::OAuth2` always has all seven
/// fields. Compare the keys both sides actually carry.
fn auth_fields_agree(ours: &Value, theirs: &Value) -> bool {
    match (ours.as_object(), theirs.as_object()) {
        (Some(a), Some(b)) => b.iter().all(|(k, v)| a.get(k).is_none_or(|o| o == v)),
        _ => ours == theirs,
    }
}

/// The implicit-`eq` allowance described above: Bruno's raw `"hello world"`
/// against our re-joined `"eq hello world"`.
fn asserts_agree(ours: &Value, theirs: &Value) -> bool {
    let (Some(a), Some(b)) = (ours.as_array(), theirs.as_array()) else {
        return ours == theirs;
    };
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            if x == y {
                return true;
            }
            let (Some(x), Some(y)) = (x.as_array(), y.as_array()) else {
                return false;
            };
            let joined = x[1].as_str().unwrap_or("");
            let raw = y[1].as_str().unwrap_or("");
            x[0] == y[0] && x[2] == y[2] && joined == format!("eq {raw}").trim()
        })
}

// --- harness plumbing ------------------------------------------------------

/// Not canonicalized on purpose: on Windows that yields a `\\?\` UNC path,
/// which Node cannot `require` from.
fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}

fn corpus_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("RP_BRU_CORPUS") {
        return Some(PathBuf::from(p));
    }
    let vendored = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/bru");
    vendored.is_dir().then_some(vendored)
}

fn collect_bru(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            // Environment files are parsed by `envToJson`, a different
            // reference implementation than the one under test.
            if p.file_name().is_some_and(|n| n == "environments") {
                continue;
            }
            collect_bru(&p, out);
        } else if p.extension().is_some_and(|e| e == "bru") {
            // Likewise `collectionBruToJson` for collection/folder config.
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name != "collection.bru" && name != "folder.bru" {
                out.push(p);
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct RefResult {
    path: String,
    #[serde(default)]
    json: Option<Value>,
    #[serde(default)]
    error: Option<String>,
}

/// `Err` means "cannot run" (skip), not "test failed".
fn run_harness(files: &[PathBuf]) -> Result<Vec<RefResult>, String> {
    let dir = repo_root().join("scripts/bru_reference");
    let script = dir.join("index.js");
    if !dir.join("node_modules/@usebruno/lang").exists() {
        return Err(format!(
            "reference package missing: run `npm install` in {}",
            dir.display()
        ));
    }
    let mut child = Command::new("node")
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run `node`: {e}"))?;
    let paths: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&paths).unwrap().as_bytes())
        .map_err(|e| format!("writing to harness: {e}"))?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "harness failed: {}",
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .take(4)
                .collect::<Vec<_>>()
                .join(" | ")
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("bad harness output: {e}"))
}

// --- known divergences -----------------------------------------------------

/// Every place a specific corpus file parses differently, keyed by
/// `(path relative to the corpus root, canonical field, verdict + why)`.
/// `parse` as the field means the two disagree on whether the file is valid
/// at all. These are *not* exclusions — an exclusion is a shape our model
/// cannot hold, whereas each of these is a concrete behavioural difference,
/// and all of them live in the deliberately hostile `malformed/` fixtures.
/// `differential_against_bruno` tolerates exactly these entries;
/// `differential_against_bruno_known_bugs` (`#[ignore]`) does not, so fixing
/// one shows up as that test starting to pass.
const KNOWN_DIVERGENCES: &[(&str, &str, &str)] = &[
    // -- ours is wrong --
    // (none: the six former `ours-bug` entries were fixed in bru.rs)
    // -- Bruno is wrong --
    (
        "malformed/empty_body_and_docs.bru",
        "body.content",
        "bruno-quirk: for an empty text block Bruno's `textblock tagend` grammar \
      swallows the closing brace and the block that follows, yielding a body of \
      \"}\\n\\ndocs {\"; we return an empty body",
    ),
    (
        "malformed/empty_body_and_docs.bru",
        "docs",
        "bruno-quirk: consequence of the above — the docs block was eaten",
    ),
    // -- we accept input Bruno's grammar rejects --
    // Reading a file more permissively than Bruno cannot lose user data, and
    // `serialize` writes canonical output, so this is left as is rather than
    // hardening the parser into a validator.
    (
        "malformed/annotation_unclosed.bru",
        "parse",
        "leniency: unterminated @annotation('''",
    ),
    (
        "malformed/deep_brackets.bru",
        "parse",
        "leniency: nested `[` lists",
    ),
    (
        "malformed/many_blocks.bru",
        "parse",
        "leniency: many `blkN` blocks Bruno's grammar has no rule for",
    ),
    (
        "malformed/mixed_line_endings.bru",
        "parse",
        "leniency: `}` on the same line as the last pair",
    ),
    (
        "malformed/multiline_delimiter_soup.bru",
        "parse",
        "leniency: unbalanced ''' delimiters",
    ),
    (
        "malformed/multiline_never_closed.bru",
        "parse",
        "leniency: ''' never closed",
    ),
    (
        "malformed/nested_block_headers.bru",
        "parse",
        "leniency: a block header inside a block",
    ),
    (
        "malformed/tilde_on_nothing.bru",
        "parse",
        "leniency: a bare `~` with no key",
    ),
    (
        "malformed/unknown_block_names.bru",
        "parse",
        "leniency: unknown block names",
    ),
];

fn known(rel: &str, field: &str) -> Option<&'static str> {
    let rel = rel.replace('\\', "/");
    KNOWN_DIVERGENCES
        .iter()
        .find(|(p, f, _)| *p == rel && *f == field)
        .map(|(_, _, why)| *why)
}

// --- the test --------------------------------------------------------------

#[test]
fn differential_against_bruno() {
    run(true);
}

/// The same comparison with the known-divergence list disabled: it fails, and
/// its output is the inventory of everything still to fix.
#[test]
#[ignore = "bug: known divergences from Bruno, listed in KNOWN_DIVERGENCES"]
fn differential_against_bruno_known_bugs() {
    run(false);
}

fn run(allow_known: bool) {
    // The counters are process-wide statics and both tests share a binary.
    ALL_EXCLUSIONS
        .iter()
        .for_each(|(_, c)| c.store(0, Ordering::Relaxed));
    // Skip loudly rather than fail when the toolchain isn't there.
    let skip = |why: String| println!("SKIP differential_against_bruno: {why}");
    if std::env::var("RP_DIFFERENTIAL").as_deref() != Ok("1") {
        return skip("RP_DIFFERENTIAL=1 not set".into());
    }
    let Some(corpus) = corpus_dir() else {
        return skip("no corpus: set RP_BRU_CORPUS or vendor crates/core/tests/corpus/bru".into());
    };
    let mut files = Vec::new();
    collect_bru(&corpus, &mut files);
    if files.is_empty() {
        return skip(format!("no .bru files under {}", corpus.display()));
    }
    let reference = match run_harness(&files) {
        Ok(r) => r,
        Err(why) => return skip(why),
    };

    let mut compared = 0usize;
    let mut matched = 0usize;
    let mut both_rejected = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut tolerated: Vec<String> = Vec::new();

    for r in &reference {
        // Lossy on purpose: `bru::parse` takes `&str`, and Node's
        // `readFileSync(_, 'utf8')` also substitutes replacement characters,
        // so both sides see the same input for non-UTF-8 files.
        let src = String::from_utf8_lossy(&std::fs::read(&r.path).expect("read .bru")).into_owned();
        let ours = bru::parse(&src);
        let rel = Path::new(&r.path)
            .strip_prefix(&corpus)
            .unwrap_or(Path::new(&r.path))
            .display()
            .to_string();

        // `record` routes a difference to either the known-divergence log or
        // the failure list.
        let tolerated_before = tolerated.len();
        let mut diffs: Vec<String> = Vec::new();
        let mut record = |field: &str, detail: String| match known(&rel, field) {
            Some(why) if allow_known => tolerated.push(format!("  {rel} [{field}] {why}")),
            _ => diffs.push(format!("    {field}: {detail}")),
        };

        match (&r.json, &ours) {
            (None, Err(_)) => both_rejected += 1,
            (None, Ok(_)) => record(
                "parse",
                format!(
                    "Bruno rejects this file ({}), we accept it",
                    r.error.as_deref().unwrap_or("?")
                ),
            ),
            (Some(_), Err(e)) => record("parse", format!("Bruno accepts this file, we error: {e}")),
            (Some(theirs), Ok(ours)) => {
                compared += 1;
                let a = canon_ours(ours);
                let b = canon_bruno(theirs);
                for (k, ours_v) in &a {
                    let theirs_v = &b[k];
                    let agree = match k.as_str() {
                        "asserts" => asserts_agree(ours_v, theirs_v),
                        "auth.fields" => auth_fields_agree(ours_v, theirs_v),
                        // X_SEQ_NON_NUMERIC leaves a null behind.
                        "seq" => theirs_v.is_null() || ours_v == theirs_v,
                        _ => ours_v == theirs_v,
                    };
                    if !agree {
                        record(k, format!("ours={ours_v} bruno={theirs_v}"));
                    }
                }
            }
        }
        if !diffs.is_empty() {
            failures.push(format!("{rel}\n{}", diffs.join("\n")));
        } else if tolerated.len() == tolerated_before && r.json.is_some() && ours.is_ok() {
            matched += 1;
        }
    }

    println!(
        "differential: {compared} files compared, {matched} matched, \
         {both_rejected} rejected by both, {} known divergences tolerated, \
         {} unexpected mismatches",
        tolerated.len(),
        failures.len()
    );
    println!("named exclusions applied (occurrences):");
    for (label, c) in ALL_EXCLUSIONS {
        println!("  {:>5}  {label}", c.load(Ordering::Relaxed));
    }
    if !tolerated.is_empty() {
        println!("known divergences (see KNOWN_DIVERGENCES):");
        tolerated.iter().for_each(|t| println!("{t}"));
    }
    assert!(
        failures.is_empty(),
        "{} file(s) diverge from Bruno:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
