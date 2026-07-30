//! The Streamable HTTP transport, driven over a real socket.
//!
//! Nothing here calls a handler directly: every assertion goes through a real
//! TCP connection to a real listener, because the mistakes that break an MCP
//! client are in the framing and the headers, not in the dispatch that the stdio
//! tests already cover.
//!
//! Every server here gets its own temporary database. None of these tests may
//! ever touch the user's real one.
use rp_mcp::http::{self, Handle};
use serde_json::{Value, json};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

/// A running server plus its temporary database.
struct Live {
    handle: Handle,
    db: PathBuf,
    client: reqwest::blocking::Client,
}

impl Drop for Live {
    fn drop(&mut self) {
        // The handle is dropped first by field order, so the file is no longer open.
        let _ = std::fs::remove_file(&self.db);
    }
}

fn temp_db(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "rp-mcp-http-{tag}-{}-{}.db",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    path
}

/// Port 0: the OS picks, and the test reads back what it got.
fn live(tag: &str) -> Live {
    let db = temp_db(tag);
    let handle =
        http::start(0, http::Bind::Loopback, Some(db.clone())).expect("the server must start");
    Live {
        handle,
        db,
        client: reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap(),
    }
}

/// One request, with every header a test might want to bend.
struct Req<'a> {
    method: &'a str,
    body: Option<&'a str>,
    token: Option<&'a str>,
    session: Option<&'a str>,
    origin: Option<&'a str>,
    accept: Option<&'a str>,
    version: Option<&'a str>,
}

impl<'a> Req<'a> {
    /// A well-formed POST: correct token, both content types accepted.
    fn post(body: &'a str) -> Self {
        Self {
            method: "POST",
            body: Some(body),
            token: None,
            session: None,
            origin: None,
            accept: Some("application/json, text/event-stream"),
            version: None,
        }
    }
}

impl Live {
    fn send(&self, req: Req<'_>) -> (u16, String, Option<String>) {
        let url = self.handle.url();
        let mut builder = match req.method {
            "POST" => self.client.post(&url),
            "GET" => self.client.get(&url),
            "DELETE" => self.client.delete(&url),
            other => panic!("unsupported method {other}"),
        };
        let token = req.token.map(str::to_string).or_else(|| {
            // Default to the real token so a test only says what it changes.
            Some(self.handle.token())
        });
        if let Some(t) = token {
            builder = builder.header("Authorization", format!("Bearer {t}"));
        }
        for (name, value) in [
            ("Mcp-Session-Id", req.session),
            ("Origin", req.origin),
            ("Accept", req.accept),
            ("MCP-Protocol-Version", req.version),
        ] {
            if let Some(v) = value {
                builder = builder.header(name, v);
            }
        }
        if let Some(b) = req.body {
            builder = builder
                .header("Content-Type", "application/json")
                .body(b.to_string());
        }
        let response = builder.send().expect("the request must reach the server");
        let status = response.status().as_u16();
        let session = response
            .headers()
            .get("mcp-session-id")
            .map(|v| v.to_str().unwrap().to_string());
        (status, response.text().unwrap(), session)
    }

    /// Initialize and return the session id the server minted.
    fn initialize(&self) -> String {
        let (status, body, session) = self.send(Req::post(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
        ));
        assert_eq!(status, 200, "{body}");
        let v: Value = serde_json::from_str(&body).expect("a JSON body");
        assert_eq!(v["result"]["protocolVersion"], "2025-11-25");
        session.expect("initialize must return an Mcp-Session-Id")
    }

    /// A request inside a session, asserting 200 and returning the parsed reply.
    fn rpc(&self, session: &str, body: &str) -> Value {
        let mut req = Req::post(body);
        req.session = Some(session);
        let (status, body, _) = self.send(req);
        assert_eq!(status, 200, "{body}");
        serde_json::from_str(&body).expect("a JSON body")
    }
}

fn call(name: &str, args: Value) -> String {
    json!({"jsonrpc":"2.0","id":42,"method":"tools/call","params":{"name":name,"arguments":args}})
        .to_string()
}

// -- the happy path ----------------------------------------------------------

#[test]
fn a_client_can_initialize_list_tools_book_time_and_read_it_back() {
    let live = live("happy");
    let session = live.initialize();

    // The initialized notification is a notification: accepted, no body.
    let mut note = Req::post(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    note.session = Some(&session);
    let (status, body, _) = live.send(note);
    assert_eq!(
        status, 202,
        "a notification is accepted with no reply: {body}"
    );
    assert!(body.is_empty(), "202 must have no body, got {body}");

    let listed = live.rpc(
        &session,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    );
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 10, "{names:?}");
    assert!(names.contains(&"log_time"));

    // Book a block that ended a minute ago, so it is neither future nor over-long.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let booked = live.rpc(
        &session,
        &call(
            "log_time",
            json!({"task": "http transport", "started_at": now - 3660, "stopped_at": now - 60}),
        ),
    );
    let text = booked["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(booked["result"]["isError"], false, "{text}");
    assert!(text.contains("http transport"), "{text}");

    // And the same server can read it back out.
    let summary = live.rpc(&session, &call("summary", json!({})));
    let text = summary["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("http transport"), "{text}");
    assert!(text.contains("1h 00m"), "{text}");

    // It really went into the temporary file, not into the app's database.
    let store = rp_store::Store::open_at(&live.db).expect("the temp database");
    let tasks = store.track_tasks().unwrap();
    assert!(
        tasks.iter().any(|t| t.name == "http transport"),
        "{:?}",
        tasks.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
}

#[test]
fn a_json_reply_says_it_is_json() {
    let live = live("content-type");
    let response = live
        .client
        .post(live.handle.url())
        .header("Authorization", format!("Bearer {}", live.handle.token()))
        .header("Accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
        .send()
        .unwrap();
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.starts_with("application/json")),
        Some(true),
        "{:?}",
        response.headers()
    );
}

// -- the token: persisted, and revocable -------------------------------------

/// The point of storing it: the config the user pasted into a client keeps
/// working after the app is restarted. Proven over the socket, not by reading the
/// row.
#[test]
fn a_token_from_a_previous_run_still_works_after_a_restart() {
    let db = temp_db("token-persists");
    let first =
        http::start(0, http::Bind::Loopback, Some(db.clone())).expect("the server must start");
    let token = first.token();
    drop(first);

    let live = Live {
        handle: http::start(0, http::Bind::Loopback, Some(db.clone())).expect("restart"),
        db,
        client: reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap(),
    };
    assert_eq!(
        live.handle.token(),
        token,
        "the stored token must be reused"
    );
    let session = live.initialize();
    let mut req = Req::post(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    req.session = Some(&session);
    req.token = Some(&token);
    assert_eq!(live.send(req).0, 200, "the old token must be accepted");
}

/// And the escape hatch: rotating it locks out anything holding the old one,
/// including a session that was already initialised with it.
#[test]
fn regenerating_replaces_the_token_persists_it_and_locks_out_the_old_one() {
    let live = live("regenerate");
    let session = live.initialize();
    let old = live.handle.token();

    let fresh = live.handle.regenerate_token().expect("rotate");
    assert_ne!(fresh, old);
    assert_eq!(live.handle.token(), fresh);

    let mut req = Req::post(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    req.session = Some(&session);
    req.token = Some(&old);
    assert_eq!(live.send(req).0, 401, "the old token must be dead at once");

    let mut req = Req::post(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    req.session = Some(&session);
    req.token = Some(&fresh);
    assert_eq!(live.send(req).0, 200, "the new one takes over");

    // Stored, so the next run agrees with what the user just copied.
    let again = http::start(0, http::Bind::Loopback, Some(live.db.clone())).expect("restart");
    assert_eq!(again.token(), fresh);
}

// -- security ----------------------------------------------------------------

#[test]
fn a_request_without_a_token_is_refused_and_says_nothing_about_sessions() {
    let live = live("no-token");
    let session = live.initialize();

    let ask = |sid: Option<&str>| {
        let mut req = Req::post(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
        req.token = None;
        req.session = sid;
        // `Req::send` substitutes the real token when `token` is None, so the
        // header has to be suppressed here instead.
        let url = live.handle.url();
        let mut builder = live
            .client
            .post(&url)
            .header("Accept", "application/json, text/event-stream")
            .body(req.body.unwrap().to_string());
        if let Some(s) = sid {
            builder = builder.header("Mcp-Session-Id", s);
        }
        let r = builder.send().unwrap();
        (r.status().as_u16(), r.text().unwrap())
    };

    let (status, body) = ask(None);
    assert_eq!(status, 401, "{body}");
    // The same refusal for a real session id and for one that was never minted:
    // an unauthenticated caller must not be able to probe session ids.
    assert_eq!(ask(Some(&session)), (status, body.clone()));
    assert_eq!(ask(Some("0000000000000000")), (status, body));
}

#[test]
fn a_wrong_token_is_refused() {
    let live = live("wrong-token");
    let session = live.initialize();
    let mut req = Req::post(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    req.session = Some(&session);
    // Same length as the real one, so only the bytes differ.
    let wrong = "f".repeat(live.handle.token().len());
    req.token = Some(&wrong);
    let (status, _, _) = live.send(req);
    assert_eq!(status, 401);

    // A token-shaped prefix is not enough either.
    let mut req = Req::post(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    req.session = Some(&session);
    let prefix = live.handle.token()[..8].to_string();
    req.token = Some(&prefix);
    assert_eq!(live.send(req).0, 401);
}

/// The `Origin` check is aimed at DNS rebinding — a page on `evil.com` resolving
/// to loopback — so a foreign *web* origin is refused even with the right token.
///
/// `null` and app schemes used to be refused too, and that was wrong: they are not
/// browsers, the bearer token is what guards this server, and turning them away
/// only ever cost a native client its tool list. That is why they are in the
/// allowed list below rather than the forbidden one.
#[test]
fn a_foreign_web_origin_is_forbidden_but_a_non_browser_one_is_not() {
    let live = live("origin");
    let session = live.initialize();
    let list = |origin: &str| {
        let mut req = Req::post(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
        req.session = Some(&session);
        req.origin = Some(origin);
        live.send(req)
    };
    for origin in [
        "http://evil.example",
        "https://localhost.evil.example",
        "http://127.0.0.1.evil.example",
    ] {
        let (status, body, _) = list(origin);
        assert_eq!(status, 403, "{origin} was allowed: {body}");
    }
    for origin in [
        // A local page.
        "http://127.0.0.1:5173",
        "http://localhost",
        // Not browsers: a sandboxed context, an editor, an app scheme.
        "null",
        "vscode-file://vscode-app",
        "app://obsidian.md",
    ] {
        let (status, body, _) = list(origin);
        assert_eq!(status, 200, "{origin} was refused: {body}");
    }
}

#[test]
fn the_listener_is_not_reachable_off_loopback() {
    let live = live("loopback");
    let port = live.handle.port;
    assert!(
        live.handle.url().contains("127.0.0.1"),
        "{}",
        live.handle.url()
    );
    // Loopback answers.
    assert!(
        TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), Duration::from_secs(2))
            .is_ok()
    );
    // This machine's routable address does not. No packets are sent by a
    // connected UDP socket, so this only asks the OS which local address it
    // would use.
    let Some(outward) = outward_ipv4() else {
        eprintln!("no non-loopback IPv4 on this machine; nothing to prove");
        return;
    };
    let refused =
        TcpStream::connect_timeout(&(outward, port).into(), Duration::from_secs(2)).is_err();
    assert!(
        refused,
        "the server answered on {outward}, which is not loopback"
    );
}

fn outward_ipv4() -> Option<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect(("8.8.8.8", 53)).ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

// -- headers and sessions ----------------------------------------------------

#[test]
fn a_call_without_a_session_id_is_a_bad_request_and_an_unknown_one_is_not_found() {
    let live = live("sessions");
    live.initialize();
    let (status, body, _) = live.send(Req::post(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
    ));
    assert_eq!(status, 400, "{body}");

    let mut req = Req::post(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    req.session = Some("not-a-session");
    let (status, body, _) = live.send(req);
    assert_eq!(status, 404, "{body}");
}

#[test]
fn two_sessions_are_independent_and_ending_one_leaves_the_other_alone() {
    let live = live("two-sessions");
    let a = live.initialize();
    let b = live.initialize();
    assert_ne!(a, b, "each initialize must mint its own session");

    // Both work.
    live.rpc(&a, r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
    live.rpc(&b, r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);

    // End A.
    let (status, _, _) = live.send(Req {
        method: "DELETE",
        body: None,
        token: None,
        session: Some(&a),
        origin: None,
        accept: None,
        version: None,
    });
    assert_eq!(status, 204);

    let mut req = Req::post(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
    req.session = Some(&a);
    assert_eq!(live.send(req).0, 404, "A must be gone");
    live.rpc(&b, r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
}

#[test]
fn a_get_is_method_not_allowed_because_there_is_no_server_stream() {
    let live = live("get");
    let (status, _, _) = live.send(Req {
        method: "GET",
        body: None,
        token: None,
        session: None,
        origin: None,
        accept: Some("text/event-stream"),
        version: None,
    });
    assert_eq!(status, 405);
}

#[test]
fn an_unsupported_protocol_version_header_is_a_bad_request() {
    let live = live("version");
    let session = live.initialize();
    for (version, expected) in [("2025-11-25", 200), ("2024-11-05", 200), ("1.0.0", 400)] {
        let mut req = Req::post(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
        req.session = Some(&session);
        req.version = Some(version);
        let (status, body, _) = live.send(req);
        assert_eq!(status, expected, "{version}: {body}");
    }
}

#[test]
fn a_client_that_cannot_take_json_is_not_acceptable() {
    let live = live("accept");
    let mut req = Req::post(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
    req.accept = Some("text/plain");
    assert_eq!(live.send(req).0, 406);
}

// -- start and stop ----------------------------------------------------------

#[test]
fn a_taken_port_is_an_error_rather_than_a_panic_or_a_silent_move() {
    let squatter = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = squatter.local_addr().unwrap().port();
    let db = temp_db("taken-port");
    let err =
        http::start(port, http::Bind::Loopback, Some(db.clone())).expect_err("that port is taken");
    let message = format!("{err:#}");
    assert!(message.contains(&port.to_string()), "{message}");
    let _ = std::fs::remove_file(&db);
}

#[test]
fn stopping_releases_the_port() {
    let db = temp_db("release");
    let first = http::start(0, http::Bind::Loopback, Some(db.clone())).unwrap();
    let port = first.port;
    let first_token = first.token();
    // Occupied while it runs.
    assert!(TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_err());

    drop(first);
    // Free the moment the handle is gone -- proven by taking it.
    let taken =
        TcpListener::bind((Ipv4Addr::LOCALHOST, port)).expect("the port must be free after a stop");
    drop(taken);

    // And it can be started again on the same port.
    let again = http::start(port, http::Bind::Loopback, Some(db.clone()))
        .expect("restart on the same port");
    assert_eq!(again.port, port);
    assert_eq!(
        again.token(),
        first_token,
        "the token is stored, so a restart keeps the pasted config valid"
    );
    drop(again);
    let _ = std::fs::remove_file(&db);
}

#[test]
fn a_bad_database_path_is_reported_rather_than_bound() {
    // A directory is not a database file.
    let dir = std::env::temp_dir();
    let err = http::start(0, http::Bind::Loopback, Some(dir))
        .expect_err("a directory cannot be a database");
    assert!(!format!("{err:#}").is_empty());
}

/// The bind is what makes the server reachable from a container, so it is checked
/// on a real socket rather than by reading the setting back.
///
/// `Bind::Any` is not tested by connecting from another machine — there is no
/// second machine here. What is checked is the part that was actually wrong: a
/// loopback listener does not answer on this host's own routable address, which is
/// the address a container's `host.docker.internal` resolves to.
#[test]
fn a_loopback_bind_does_not_answer_on_a_routable_address_and_a_wide_one_does() {
    let dir = std::env::temp_dir().join("rp-mcp-bind");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("state.db");

    // A non-loopback address of this machine, if it has one. Without a network
    // there is nothing to prove, so the test says so instead of failing.
    let routable = std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            // No packet is sent; connect() just picks the outbound interface.
            s.connect("192.0.2.1:9")?;
            s.local_addr()
        })
        .map(|a| a.ip())
        .ok()
        .filter(|ip| !ip.is_loopback());
    let Some(routable) = routable else {
        eprintln!("no routable address on this machine; nothing to check");
        return;
    };

    let handle = http::start(0, http::Bind::Loopback, Some(db.clone())).expect("start");
    let port = handle.port;
    assert!(
        handle.url().contains("127.0.0.1"),
        "a loopback server must hand out a loopback URL: {}",
        handle.url()
    );
    // The listener is up on loopback...
    std::net::TcpStream::connect(("127.0.0.1", port)).expect("loopback must answer");
    // ...and not on the address a container would dial.
    assert!(
        std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::new(routable, port),
            std::time::Duration::from_millis(400),
        )
        .is_err(),
        "a loopback bind must not answer on {routable} — that is the whole bug"
    );
    drop(handle);

    let handle = http::start(0, http::Bind::Any, Some(db)).expect("start wide");
    let port = handle.port;
    assert!(
        handle.url().contains("host.docker.internal"),
        "a wide server must hand out an address a container can reach: {}",
        handle.url()
    );
    assert!(
        handle.local_url().contains("127.0.0.1"),
        "and still offer the local one: {}",
        handle.local_url()
    );
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::new(routable, port),
        std::time::Duration::from_millis(400),
    )
    .expect("a wide bind must answer on the routable address");
    std::net::TcpStream::connect(("127.0.0.1", port)).expect("and on loopback too");

    let _ = std::fs::remove_dir_all(&dir);
}
