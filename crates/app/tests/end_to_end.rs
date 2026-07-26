//! End-to-end suite for the app: file on disk -> parsed request -> resolved
//! variables -> bytes on a socket, plus the session store round-trip.
//!
//! Every layer here is unit-tested on its own; nothing tested the seam. The two
//! bugs this shape of test is meant to catch are "the file said one thing and
//! the wire got another" and "the captured token never reaches the next
//! request", which is what the user was actually blocked on.
//!
//! ## What cannot be reached from here
//! `crates/app` is a binary (`src/main.rs`), so an integration test cannot
//! `use` its modules: `session::{load, kind_of, flatten}` — the loader this
//! file has to reimplement below — `tabs`, `request_pane`, `editor` and
//! `text_input` are all invisible from outside. `session.rs` says in its own
//! header that it was split out of `main.rs` "so it is testable", but as long
//! as the package has no `[lib]` target that only means unit tests inside the
//! binary. Extracting `session.rs` (and `tabs.rs`, which is deliberately
//! generic over the content type) into a small `rp-app-core` library, or adding
//! a `[lib]` target to this package, would let this file drive the app's real
//! loader instead of a copy of it.
//!
//! No new dependencies: the HTTP server is `std::net`, the same ~120-line shape
//! as `crates/core/tests/exec_corpus.rs`, bound to `127.0.0.1:0`.

use rp_core::interp::{self, Vars};
use rp_core::model::{Auth, Body, Item, Request};
use rp_core::{bru, exec, httpfile, vars};
use rp_store::{Store, TabRecord, WindowSession};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// ------------------------------------------------------------------ test server

#[derive(Clone, Debug, Default)]
struct Wire {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Wire {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }
    fn query(&self) -> &str {
        self.target.split_once('?').map(|(_, q)| q).unwrap_or("")
    }
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

type Handler = Arc<dyn Fn(&Wire) -> Vec<u8> + Send + Sync>;

struct Server {
    addr: String,
    seen: Arc<Mutex<Vec<Wire>>>,
}

impl Server {
    fn new(handler: impl Fn(&Wire) -> Vec<u8> + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
        let addr = listener.local_addr().expect("local_addr").to_string();
        let seen: Arc<Mutex<Vec<Wire>>> = Arc::new(Mutex::new(Vec::new()));
        let handler: Handler = Arc::new(handler);
        let (h, s) = (handler, Arc::clone(&seen));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let (h, s) = (Arc::clone(&h), Arc::clone(&s));
                std::thread::spawn(move || serve(stream, &h, &s));
            }
        });
        Server { addr, seen }
    }
    fn last(&self) -> Wire {
        self.seen
            .lock()
            .expect("lock")
            .last()
            .cloned()
            .expect("no request arrived")
    }
    fn seen(&self) -> Vec<Wire> {
        self.seen.lock().expect("lock").clone()
    }
}

fn serve(stream: TcpStream, handler: &Handler, seen: &Arc<Mutex<Vec<Wire>>>) {
    let mut r = BufReader::new(stream);
    let Some(wire) = read_request(&mut r) else {
        return;
    };
    let out = handler(&wire);
    seen.lock().expect("lock").push(wire);
    let mut s = r.into_inner();
    let _ = s.write_all(&out);
    let _ = s.flush();
}

fn read_request(r: &mut BufReader<TcpStream>) -> Option<Wire> {
    let mut line = String::new();
    if r.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let mut parts = line.split_whitespace();
    let mut w = Wire {
        method: parts.next()?.to_string(),
        target: parts.next().unwrap_or("/").to_string(),
        ..Default::default()
    };
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).ok()? == 0 {
            break;
        }
        let h = h.trim_end_matches(['\r', '\n']);
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            w.headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let len: usize = w
        .header("content-length")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    if len > 0 {
        let mut body = vec![0u8; len];
        r.read_exact(&mut body).ok()?;
        w.body = body;
    }
    Some(w)
}

fn json_reply(body: &str) -> Vec<u8> {
    let mut out = format!(
        "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body.as_bytes());
    out
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(f)
}

// ------------------------------------------------------- the app's own loader
//
// A copy of `crates/app/src/session.rs`, which the binary crate hides. Kept
// deliberately literal so a divergence is obvious on sight.

#[derive(Debug, Default)]
struct Loaded {
    requests: Vec<Request>,
    environments: Vec<rp_core::model::Environment>,
    declared: Vec<(String, String)>,
}

fn flatten(items: &[Item]) -> Vec<Request> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Item::Request { request, .. } => out.push((**request).clone()),
            Item::Folder { items, .. } => out.extend(flatten(items)),
        }
    }
    out
}

fn load(path: &Path) -> anyhow::Result<Loaded> {
    if path.is_dir() {
        let c = bru::load_collection(path)?;
        return Ok(Loaded {
            requests: flatten(&c.items),
            environments: c.environments,
            declared: Vec::new(),
        });
    }
    let src = String::from_utf8_lossy(&std::fs::read(path)?).into_owned();
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bru"))
    {
        return Ok(Loaded {
            requests: vec![bru::parse(&src)?],
            ..Default::default()
        });
    }
    let (requests, declared) = httpfile::parse_with_vars(&src)?;
    Ok(Loaded {
        requests,
        environments: vars::load_env_files(path).unwrap_or_default(),
        declared,
    })
}

/// Variables of one named environment, as the sidebar's environment picker
/// hands them to the executor.
fn env_vars(loaded: &Loaded, name: &str) -> Vars {
    loaded
        .environments
        .iter()
        .find(|e| e.name == name)
        .map(|e| {
            e.vars
                .iter()
                .map(|(k, v, _)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default()
}

// ----------------------------------------------------------------- utilities

fn core_tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/tests")
}

fn http_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in [
        core_tests_dir().join("corpus/http/valid"),
        core_tests_dir().join("fixtures/http"),
    ] {
        let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        for e in entries {
            let p = e.expect("dir entry").path();
            if p.extension().is_some_and(|e| e == "http") {
                out.push(p);
            }
        }
    }
    out.sort();
    assert!(out.len() > 30, "corpus shrank: {} files", out.len());
    out
}

/// `application/x-www-form-urlencoded`, matching what reqwest's `.form()` and
/// `.query()` produce, so an expectation can be compared to the wire.
fn form_encode(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", pct(k), pct(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn pct(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'*' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Every `{{name}}` the request still references, so the test can fill it in
/// the way a user or an earlier response would. Without this, half the corpus
/// would be skipped for referring to a token nobody set.
fn fill_missing(r: &Request, vars: &mut Vars) {
    let scan = |s: &str, vars: &mut Vars| {
        for n in interp::missing(s, vars) {
            vars.insert(n.clone(), format!("filled-{n}"));
        }
    };
    scan(&r.url, vars);
    let texts: Vec<String> = r
        .headers
        .iter()
        .chain(r.query.iter())
        .flat_map(|(k, v)| [k.clone(), v.clone()])
        .chain(match &r.body {
            Body::Json(s) | Body::Text(s) | Body::Xml(s) | Body::Sparql(s) => vec![s.clone()],
            Body::GraphQl { query, variables } => vec![query.clone(), variables.clone()],
            Body::Form(f) => f.iter().flat_map(|(k, v)| [k.clone(), v.clone()]).collect(),
            _ => Vec::new(),
        })
        .collect();
    for t in texts {
        scan(&t, vars);
    }
}

/// Point a resolved request at the test server, keeping path and query. Belt and
/// braces: the corpus is full of real hosts (api.github.com, a Kubernetes
/// endpoint) and this suite must never leave loopback.
fn to_loopback(url: &str, addr: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let tail = rest.split_once('/').map(|(_, t)| t).unwrap_or("");
    Some(format!("http://{addr}/{tail}"))
}

#[derive(Default)]
struct Failures(Vec<String>);
impl Failures {
    fn check(&mut self, ok: bool, msg: impl FnOnce() -> String) {
        if !ok {
            self.0.push(format!("  {}", msg()));
        }
    }
    fn add(&mut self, msg: String) {
        self.0.push(format!("  {msg}"));
    }
    fn assert(self) {
        assert!(
            self.0.is_empty(),
            "{} failure(s):\n{}",
            self.0.len(),
            self.0.join("\n")
        );
    }
}

// ------------------------------------------- every shipped .http on the wire

#[test]
fn every_shipped_http_request_reaches_the_wire_as_the_file_described() {
    let s = Server::new(|_| json_reply(r#"{"ok":true,"id":1,"token":"t"}"#));
    let mut fail = Failures::default();
    let mut sent = 0usize;

    for file in http_files() {
        let loaded = match load(&file) {
            Ok(l) => l,
            Err(e) => {
                fail.add(format!("{}: load failed: {e:#}", file.display()));
                continue;
            }
        };
        let base = vars::merge(&env_vars(&loaded, "dev"), &loaded.declared);

        for req in &loaded.requests {
            let where_ = format!("{} [{}]", file.display(), req.name);
            // The app never sends these through `exec`: a websocket URL, a
            // CONNECT tunnel, an OAuth2 dance, or a body that reads a file the
            // corpus does not ship.
            if matches!(req.method.as_str(), "CONNECT" | "WEBSOCKET")
                || matches!(req.body, Body::File { .. } | Body::MultipartForm(_))
                || matches!(req.auth, Auth::OAuth2(_))
            {
                continue;
            }
            let mut vars = base.clone();
            fill_missing(req, &mut vars);

            let resolved_url = interp::interpolate(&req.url, &vars);
            let Some(url) = to_loopback(&resolved_url, &s.addr) else {
                continue; // ws://, a relative URL, or no scheme at all
            };
            let expect_path = format!(
                "/{}",
                resolved_url
                    .split_once("://")
                    .and_then(|(_, r)| r.split_once('/'))
                    .map(|(_, t)| t)
                    .unwrap_or("")
            );
            let expect_path = expect_path.split('?').next().unwrap_or("/").to_string();

            let mut to_send = req.clone();
            to_send.url = url;
            match block_on(exec::send(&to_send, &vars)) {
                Ok(res) => fail.check(res.status == 200, || format!("{where_}: {}", res.status)),
                Err(e) => {
                    fail.add(format!("{where_}: send failed: {e:#}"));
                    continue;
                }
            }
            sent += 1;
            let w = s.last();

            fail.check(w.method == req.method.trim().to_uppercase(), || {
                format!(
                    "{where_}: method {} on the wire, {} in the file",
                    w.method, req.method
                )
            });
            fail.check(w.path() == expect_path, || {
                format!(
                    "{where_}: path {:?} on the wire, {expect_path:?} in the file",
                    w.path()
                )
            });
            // Declared headers must arrive verbatim, repeats included; the
            // client is free to add its own (host, accept, content-length).
            let declared: Vec<(String, String)> = req
                .headers
                .iter()
                .map(|(k, v)| (interp::interpolate(k, &vars), interp::interpolate(v, &vars)))
                .collect();
            for (name, _) in &declared {
                let want: Vec<&str> = declared
                    .iter()
                    .filter(|(k, _)| k.eq_ignore_ascii_case(name))
                    .map(|(_, v)| v.as_str())
                    .collect();
                let got: Vec<&str> = w
                    .headers
                    .iter()
                    .filter(|(k, _)| k.eq_ignore_ascii_case(name))
                    .map(|(_, v)| v.as_str())
                    .collect();
                fail.check(got == want, || {
                    format!("{where_}: header {name}: wire {got:?}, file {want:?}")
                });
            }
            // Declared query parameters must all be present, encoded.
            for (k, v) in &req.query {
                let pair =
                    form_encode(&[(interp::interpolate(k, &vars), interp::interpolate(v, &vars))]);
                fail.check(w.query().contains(&pair), || {
                    format!("{where_}: query {:?} missing {pair:?}", w.query())
                });
            }
            // And the body, byte for byte.
            let expect_body = match &req.body {
                Body::Json(b) | Body::Text(b) | Body::Xml(b) | Body::Sparql(b) => {
                    Some(interp::interpolate(b, &vars))
                }
                Body::Form(f) => Some(form_encode(
                    &f.iter()
                        .map(|(k, v)| {
                            (interp::interpolate(k, &vars), interp::interpolate(v, &vars))
                        })
                        .collect::<Vec<_>>(),
                )),
                Body::GraphQl { query, variables } => {
                    let variables = interp::interpolate(variables, &vars);
                    let parsed: serde_json::Value = if variables.trim().is_empty() {
                        serde_json::Value::Null
                    } else {
                        serde_json::from_str(&variables).unwrap_or(serde_json::Value::Null)
                    };
                    Some(
                        serde_json::json!({
                            "query": interp::interpolate(query, &vars),
                            "variables": parsed,
                        })
                        .to_string(),
                    )
                }
                _ => None,
            };
            if let Some(expect) = expect_body {
                fail.check(w.text() == expect, || {
                    format!(
                        "{where_}: body\n      wire {:?}\n      file {expect:?}",
                        w.text()
                    )
                });
            } else {
                fail.check(w.body.is_empty(), || {
                    format!("{where_}: body {:?} but the file declared none", w.text())
                });
            }
        }
    }
    assert!(
        sent >= 40,
        "only {sent} requests actually reached the wire — the skip rules ate the corpus"
    );
    fail.assert();
}

// ------------------------------------------------------- the auth-token chain

/// The flow the user was blocked on: a login response's handler script sets a
/// variable, and every later request has to carry it. Driven through the real
/// parser, the real capture scanner and a real socket.
#[test]
fn a_captured_token_reaches_the_next_request_on_the_wire() {
    let s = Server::new(|w| match w.path() {
        "/api/v1/login" => json_reply(r#"{"token":"tok-from-login","user":{"id":7}}"#),
        _ => json_reply(r#"[{"id":"u-1"},{"id":"u-2"}]"#),
    });
    let file = core_tests_dir().join("fixtures/http/api.http");
    let loaded = load(&file).expect("load api.http");
    assert!(
        loaded.requests.len() >= 2,
        "api.http should hold the login plus the follow-ups"
    );

    // The environment picker on "dev", with baseUrl pointed at the test server
    // exactly as the user would point it at localhost.
    let mut env = env_vars(&loaded, "dev");
    assert_eq!(env.get("userName").map(String::as_str), Some("ada"));
    env.insert("baseUrl".into(), format!("http://{}", s.addr));
    let mut all_vars = vars::merge(&env, &loaded.declared);

    // 1. Login. The body carries the env's userName, not a placeholder.
    let login = &loaded.requests[0];
    let res = block_on(exec::send(login, &all_vars)).expect("login send");
    assert_eq!(res.status, 200);
    let sent = s.last();
    assert_eq!(sent.method, "POST");
    assert_eq!(sent.path(), "/api/v1/login");
    assert!(
        sent.text().contains("\"username\": \"ada\""),
        "login body did not interpolate userName: {:?}",
        sent.text()
    );

    // 2. The handler script captures the token, the way main.rs does on every
    //    response it observes.
    let script = login
        .post_response_script
        .as_deref()
        .expect("api.http's login has a > {% ... %} handler");
    let captured = vars::capture(script, &res.body, res.status, &res.headers);
    assert_eq!(
        captured,
        [("authToken".to_string(), "tok-from-login".to_string())],
        "the login handler must capture the token"
    );
    all_vars = vars::merge(&all_vars, &captured);

    // 3. The next request must now carry it. Before the capture it must not
    //    even be sendable — that is the error the user saw.
    let list = &loaded.requests[1];
    let before = vars::merge(&env, &loaded.declared);
    let err = block_on(exec::send(list, &before))
        .expect_err("without the captured token this request must fail early");
    assert!(err.to_string().contains("authToken"), "{err}");

    let res = block_on(exec::send(list, &all_vars)).expect("list send");
    assert_eq!(res.status, 200);
    let w = s.last();
    assert_eq!(w.method, "GET");
    assert_eq!(w.path(), "/api/v1/users");
    assert_eq!(
        w.header("authorization"),
        Some("Bearer tok-from-login"),
        "the captured token did not reach the wire"
    );
    for pair in ["page=1", "per_page=25", "sort=created_at"] {
        assert!(w.query().contains(pair), "query {:?}", w.query());
    }

    // 4. And a value captured from *that* response chains on: the third request
    //    posts a body built from the env, and `firstUserId` becomes available.
    let chained = vars::capture(
        loaded.requests[1]
            .post_response_script
            .as_deref()
            .unwrap_or(""),
        &res.body,
        res.status,
        &res.headers,
    );
    assert_eq!(
        chained,
        [("firstUserId".to_string(), "u-1".to_string())],
        "res.body[0].id must be captured"
    );
}

// ------------------------------------------------------- bruno collections

#[test]
fn a_bruno_collection_loads_and_every_request_is_resolvable_and_sendable() {
    let s = Server::new(|_| json_reply(r#"{"ok":true}"#));
    let root = core_tests_dir().join("fixtures/coll");
    let loaded = load(&root).expect("load collection");

    assert_eq!(
        loaded.requests.len(),
        3,
        "mini collection has three requests"
    );
    let names: Vec<&str> = loaded.requests.iter().map(|r| r.name.as_str()).collect();
    // `load_dir` puts folders before loose requests, each group sorted.
    assert_eq!(names, ["nested", "first", "second"], "folder order");

    // `environments/Local.bru` provides `host`; `tok` is declared secret with no
    // value, which is exactly the "fill it in yourself" case.
    let mut env = env_vars(&loaded, "Local");
    assert!(env.contains_key("host"), "Local env must define host");
    env.insert("host".into(), format!("http://{}", s.addr));
    env.insert("tok".into(), "secret-token".into());

    let mut fail = Failures::default();
    for req in &loaded.requests {
        fail.check(interp::missing(&req.url, &env).is_empty(), || {
            format!(
                "{}: unresolved {:?}",
                req.name,
                interp::missing(&req.url, &env)
            )
        });
        match block_on(exec::send(req, &env)) {
            Ok(res) => fail.check(res.status == 200, || {
                format!("{}: status {}", req.name, res.status)
            }),
            Err(e) => fail.add(format!("{}: {e:#}", req.name)),
        }
    }
    fail.assert();

    let wire = s.seen();
    assert_eq!(
        wire.iter()
            .map(|w| (w.method.as_str(), w.path()))
            .collect::<Vec<_>>(),
        [("POST", "/nested"), ("GET", "/first"), ("GET", "/second")]
    );
    let nested = &wire[0];
    assert_eq!(nested.header("authorization"), Some("Bearer secret-token"));
    assert_eq!(nested.header("content-type"), Some("application/json"));
    assert!(nested.text().contains("\"a\": 1"), "{:?}", nested.text());
}

/// The upstream Bruno testbench, loaded the way the app loads a dropped folder.
/// Nothing is sent: the point is that a 223-request collection parses whole and
/// that every variable it needs is either in its own environment or in the
/// pinned list of things the user has to supply.
#[test]
fn the_bruno_testbench_collection_loads_whole() {
    let root = core_tests_dir().join("corpus/bru/collection");
    let c = bru::load_collection(&root).expect("load testbench");
    let requests = flatten(&c.items);
    // 239 .bru files, minus the folder.bru/collection.bru settings files.
    assert_eq!(requests.len(), 223, "testbench request count changed");
    assert!(
        c.environments.iter().any(|e| e.name == "Local"),
        "environments: {:?}",
        c.environments.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    let env: Vars = c
        .environments
        .iter()
        .find(|e| e.name == "Local")
        .map(|e| {
            e.vars
                .iter()
                .map(|(k, v, _)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default();

    let mut fail = Failures::default();
    let mut needed: Vec<String> = Vec::new();
    for r in &requests {
        fail.check(!r.method.trim().is_empty(), || {
            format!("{}: no method", r.name)
        });
        fail.check(!r.url.trim().is_empty(), || format!("{}: no url", r.name));
        for n in interp::missing(&r.url, &env) {
            if !needed.contains(&n) {
                needed.push(n);
            }
        }
    }
    needed.sort();
    fail.assert();
    // Pinned: every other request's URL resolves from the Local environment
    // alone. A new name appearing here means a request references a variable
    // nothing defines, which the app can only report as "unresolved".
    assert_eq!(
        needed,
        // The testbench has one request that deliberately references a variable
        // nothing defines; everything else resolves from `environments/Local.bru`.
        ["undefinedVar"],
        "the set of URL variables the Local environment does not define changed"
    );
}

// ---------------------------------------------------------------- the store

/// A scratch directory that cleans itself up.
struct Tmp(PathBuf);
impl Tmp {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("rp-app-e2e-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Tmp(dir)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_session_survives_being_saved_closed_and_reopened() {
    let tmp = Tmp::new("session");
    let db = tmp.path("state.db");
    let windows = vec![
        WindowSession {
            tabs: vec![
                TabRecord {
                    kind: "tool".into(),
                    key: "base64-string-converter".into(),
                    detail: None,
                    pinned: false,
                    active: true,
                },
                TabRecord {
                    kind: "request".into(),
                    key: core_tests_dir()
                        .join("fixtures/http/api.http")
                        .to_string_lossy()
                        .into_owned(),
                    detail: Some("1".into()),
                    pinned: false,
                    active: true,
                },
            ],
            active_tab: Some(1),
        },
        WindowSession {
            tabs: vec![TabRecord {
                kind: "tool".into(),
                key: "json-viewer".into(),
                detail: None,
                pinned: false,
                active: true,
            }],
            active_tab: Some(0),
        },
    ];

    {
        let mut store = Store::open_at(&db).expect("open");
        store.save_session(&windows).expect("save");
    } // dropped: the app has quit

    let store = Store::open_at(&db).expect("reopen");
    assert_eq!(store.load_session().expect("load"), windows);
    // Compared against the build's own target, so adding a migration does not
    // break this test for a reason that has nothing to do with sessions.
    assert_eq!(
        store.schema_version().expect("version"),
        Store::latest_schema_version()
    );

    // The restored request tab still points at a file the loader can open.
    let restored = &store.load_session().expect("load")[0].tabs[1];
    let loaded = load(Path::new(&restored.key)).expect("reopen the restored file");
    let index: usize = restored.detail.as_deref().unwrap_or("0").parse().unwrap();
    assert!(loaded.requests.get(index).is_some(), "stale tab index");
}

#[test]
fn a_database_from_a_newer_build_is_refused_not_mangled() {
    let tmp = Tmp::new("newer");
    let db = tmp.path("state.db");
    {
        let mut store = Store::open_at(&db).expect("create");
        store
            .save_session(&[WindowSession {
                tabs: vec![TabRecord {
                    kind: "tool".into(),
                    key: "uuid-generator".into(),
                    detail: None,
                    pinned: false,
                    active: true,
                }],
                active_tab: Some(0),
            }])
            .expect("save");
    }
    let before = std::fs::read(&db).expect("read db");

    // `user_version` is a big-endian u32 at offset 60 of the SQLite header, so a
    // "written by a newer build" file needs no newer build to produce it.
    let mut bytes = before.clone();
    bytes[60..64].copy_from_slice(&99u32.to_be_bytes());
    std::fs::write(&db, &bytes).expect("write db");

    let err = Store::open_at(&db)
        .err()
        .map(|e| format!("{e:#}"))
        .unwrap_or_else(|| panic!("a version-99 database must be refused"));
    assert!(err.contains("99"), "{err}");
    assert!(err.to_lowercase().contains("newer"), "{err}");

    // Refused means untouched: the user's data is still there for the newer
    // build that wrote it.
    assert_eq!(
        std::fs::read(&db).expect("read db"),
        bytes,
        "a refused database must not be written to"
    );
}
