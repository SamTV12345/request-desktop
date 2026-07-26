//! End-to-end suite for `rp_core::exec::send` against a real HTTP server.
//!
//! `src/exec.rs` unit-tests the pieces (interpolation, auth headers, body
//! encoding) and has exactly one canned-socket test. Everything that only shows
//! up once bytes cross a socket — redirects, gzip, cookies, chunked framing, a
//! connection dying mid-body — lives here.
//!
//! The server is ~120 lines of `std::net`: no new dependency, binds
//! `127.0.0.1:0` and reports the assigned port, so the suite is loopback-only
//! and never collides with anything. One thread per connection, because the
//! client keeps a process-wide pool and a sequential accept loop would deadlock
//! on a pooled connection that is still open.
//!
//! House style follows `http_corpus.rs`: failures are collected with the case
//! name instead of dying on the first one.

use rp_core::exec::send;
use rp_core::interp::Vars;
use rp_core::model::{ApiKeyPlacement, Auth, Body, OAuth2, Request};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ------------------------------------------------------------------ test server

/// One request as it actually arrived on the wire.
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

/// A handler sees the request and the 1-based sequence number of this request
/// across the whole server, and returns the raw response bytes to write.
type Handler = Arc<dyn Fn(&Wire, usize) -> Vec<u8> + Send + Sync>;

struct Server {
    addr: String,
    seen: Arc<Mutex<Vec<Wire>>>,
}

impl Server {
    fn new(handler: impl Fn(&Wire, usize) -> Vec<u8> + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
        let addr = listener.local_addr().expect("local_addr").to_string();
        let seen: Arc<Mutex<Vec<Wire>>> = Arc::new(Mutex::new(Vec::new()));
        let handler: Handler = Arc::new(handler);
        let (h, s) = (handler, Arc::clone(&seen));
        // Detached: the process is the test binary and exits when the suite ends.
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let (h, s) = (Arc::clone(&h), Arc::clone(&s));
                std::thread::spawn(move || serve(stream, &h, &s));
            }
        });
        Server { addr, seen }
    }

    /// Always the port the OS handed out — never a hardcoded one.
    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }
    fn seen(&self) -> Vec<Wire> {
        self.seen.lock().expect("seen lock").clone()
    }
    fn hits(&self) -> usize {
        self.seen.lock().expect("seen lock").len()
    }
}

fn serve(stream: TcpStream, handler: &Handler, seen: &Arc<Mutex<Vec<Wire>>>) {
    let mut r = BufReader::new(stream);
    let Some(wire) = read_request(&mut r) else {
        return;
    };
    let n = {
        let mut g = seen.lock().expect("seen lock");
        g.push(wire.clone());
        g.len()
    };
    let out = handler(&wire, n);
    let mut s = r.into_inner();
    let _ = s.write_all(&out);
    let _ = s.flush();
    // Every response carries `Connection: close`, so one request per connection
    // and no pooled half-closed sockets between cases.
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
    } else if w
        .header("transfer-encoding")
        .is_some_and(|v| v.eq_ignore_ascii_case("chunked"))
    {
        w.body = read_chunked(r)?;
    }
    Some(w)
}

fn read_chunked(r: &mut BufReader<TcpStream>) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).ok()? == 0 {
            return Some(out);
        }
        let n = usize::from_str_radix(line.trim(), 16).ok()?;
        let mut buf = vec![0u8; n + 2]; // chunk + CRLF
        r.read_exact(&mut buf).ok()?;
        if n == 0 {
            return Some(out);
        }
        out.extend_from_slice(&buf[..n]);
    }
}

/// `HTTP/1.1 <status>` plus headers plus body, with `Connection: close`.
fn reply(status: u16, reason: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status} {reason}\r\nConnection: close\r\n").into_bytes();
    for (k, v) in headers {
        out.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
    }
    out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
    out.extend_from_slice(body);
    out
}

fn ok(body: &str) -> Vec<u8> {
    reply(
        200,
        "OK",
        &[("Content-Type", "text/plain")],
        body.as_bytes(),
    )
}

/// Header-only response: no `Content-Length`, for 204/304 and HEAD.
fn bodyless(status: u16, reason: &str, headers: &[(&str, &str)]) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status} {reason}\r\nConnection: close\r\n").into_bytes();
    for (k, v) in headers {
        out.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    out
}

// -------------------------------------------------------------------- gzip

/// A valid gzip stream using *stored* (uncompressed) DEFLATE blocks.
///
/// The client turns gzip on, so a gzip body has to be tested; no compressor is
/// a dependency, and DEFLATE's stored-block form needs no compressor at all.
/// Twenty lines here beats adding `flate2` to test one code path.
fn gzip(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1f, 0x8b, 0x08, 0, 0, 0, 0, 0, 0, 0xff];
    let chunks: Vec<&[u8]> = if data.is_empty() {
        vec![&[]]
    } else {
        data.chunks(0xFFFF).collect()
    };
    let last = chunks.len() - 1;
    for (i, c) in chunks.iter().enumerate() {
        out.push(u8::from(i == last)); // BFINAL, BTYPE=00 (stored)
        let n = c.len() as u16;
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&(!n).to_le_bytes());
        out.extend_from_slice(c);
    }
    out.extend_from_slice(&crc32(data).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for b in data {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

// ------------------------------------------------------------------ helpers

fn req(method: &str, url: &str) -> Request {
    Request {
        method: method.into(),
        url: url.into(),
        ..Default::default()
    }
}

/// One runtime per test; `send` is the only async surface in the crate.
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(f)
}

fn go(r: &Request) -> anyhow::Result<rp_core::model::Response> {
    block_on(send(r, &Vars::new()))
}

/// Reports every failure at once, like `http_corpus.rs`.
#[derive(Default)]
struct Failures(Vec<String>);
impl Failures {
    fn add(&mut self, case: impl std::fmt::Display, msg: impl std::fmt::Display) {
        self.0.push(format!("  {case}: {msg}"));
    }
    fn check(&mut self, cond: bool, case: impl std::fmt::Display, msg: impl std::fmt::Display) {
        if !cond {
            self.add(case, msg);
        }
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

// ------------------------------------------------------------------- methods

#[test]
fn every_method_reaches_the_wire_with_its_body() {
    let s = Server::new(|w, _| {
        if w.method == "HEAD" {
            // A HEAD response must not carry a body even though it may
            // advertise a length.
            return bodyless(200, "OK", &[("Content-Length", "5")]);
        }
        ok(&format!("{} {}", w.method, w.text()))
    });
    let mut fail = Failures::default();

    // Bodyless methods.
    for m in ["GET", "DELETE", "OPTIONS", "TRACE", "PURGE", "PROPFIND"] {
        let res = go(&req(m, &s.url("/x"))).unwrap_or_else(|e| panic!("{m}: {e:#}"));
        fail.check(res.status == 200, m, "status");
        fail.check(
            res.body == format!("{m} ").into_bytes(),
            m,
            format!("body {:?}", res.text()),
        );
    }

    // HEAD: response headers, no body.
    let head = go(&req("HEAD", &s.url("/x"))).expect("HEAD");
    fail.check(head.status == 200, "HEAD", "status");
    fail.check(head.body.is_empty(), "HEAD", "body must be empty");
    fail.check(head.size_bytes == 0, "HEAD", "size_bytes must be 0");

    // Bodied methods.
    for m in ["POST", "PUT", "PATCH"] {
        let mut r = req(m, &s.url("/x"));
        r.body = Body::Json(format!("{{\"m\":\"{m}\"}}"));
        let res = go(&r).unwrap_or_else(|e| panic!("{m}: {e:#}"));
        fail.check(
            res.text() == format!("{m} {{\"m\":\"{m}\"}}"),
            m,
            format!("echo {:?}", res.text()),
        );
    }

    let seen = s.seen();
    fail.check(seen.len() == 10, "hits", format!("{} requests", seen.len()));
    for w in &seen {
        fail.check(w.path() == "/x", &w.method, format!("path {}", w.path()));
        fail.check(
            w.header("host").is_some(),
            &w.method,
            "no Host header on the wire",
        );
    }
    for w in seen.iter().filter(|w| w.method == "POST") {
        fail.check(
            w.header("content-type") == Some("application/json"),
            "POST",
            "content-type",
        );
    }
    fail.assert();
}

trait Text {
    fn text(&self) -> String;
}
impl Text for rp_core::model::Response {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

// -------------------------------------------------------------------- status

#[test]
fn status_codes_and_status_text_round_trip() {
    let s = Server::new(|w, _| {
        let code: u16 = w.path().trim_start_matches('/').parse().unwrap_or(500);
        match code {
            204 | 304 => bodyless(code, "", &[]),
            _ => reply(code, "", &[], b"payload"),
        }
    });
    let mut fail = Failures::default();
    let cases: &[(u16, &str)] = &[
        (200, "OK"),
        (201, "Created"),
        (202, "Accepted"),
        (204, "No Content"),
        (304, "Not Modified"),
        (400, "Bad Request"),
        (401, "Unauthorized"),
        (404, "Not Found"),
        (418, "I'm a teapot"),
        (422, "Unprocessable Entity"),
        (500, "Internal Server Error"),
        (503, "Service Unavailable"),
    ];
    for (code, reason) in cases {
        let res = go(&req("GET", &s.url(&format!("/{code}")))).unwrap_or_else(|e| panic!("{e:#}"));
        fail.check(res.status == *code, code, format!("status {}", res.status));
        fail.check(
            res.status_text == *reason,
            code,
            format!("status_text {:?}", res.status_text),
        );
        let want_body = if matches!(code, 204 | 304) {
            ""
        } else {
            "payload"
        };
        fail.check(
            res.text() == want_body,
            code,
            format!("body {:?}", res.text()),
        );
        fail.check(
            res.size_bytes == want_body.len(),
            code,
            format!("size_bytes {}", res.size_bytes),
        );
    }

    // A status with no canonical reason yields an empty status_text, not a panic.
    let odd = go(&req("GET", &s.url("/599"))).expect("599");
    fail.check(odd.status == 599, 599, "status");
    fail.check(
        odd.status_text.is_empty(),
        599,
        "status_text should be empty",
    );
    fail.assert();
}

// ----------------------------------------------------------------- redirects

#[test]
fn a_redirect_chain_is_followed_to_the_end() {
    let s = Server::new(|w, _| match w.path() {
        "/a" => bodyless(301, "Moved Permanently", &[("Location", "/b")]),
        "/b" => bodyless(302, "Found", &[("Location", "/c")]),
        "/c" => bodyless(307, "Temporary Redirect", &[("Location", "/end")]),
        _ => ok("arrived"),
    });
    let res = go(&req("GET", &s.url("/a"))).expect("redirect chain");
    assert_eq!(res.status, 200);
    assert_eq!(res.text(), "arrived");
    let paths: Vec<String> = s.seen().iter().map(|w| w.path().to_string()).collect();
    assert_eq!(
        paths,
        ["/a", "/b", "/c", "/end"],
        "every hop must be walked"
    );
}

#[test]
fn a_redirect_loop_terminates_at_the_ten_hop_limit() {
    let s = Server::new(|_, _| bodyless(301, "Moved Permanently", &[("Location", "/loop")]));
    let start = std::time::Instant::now();
    let err = go(&req("GET", &s.url("/loop")))
        .expect_err("an endless redirect loop must be an Err, not a hang")
        .to_string()
        .to_lowercase();
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "loop took {:?}",
        start.elapsed()
    );
    assert!(err.contains("redirect"), "{err}");
    // Policy::limited(10): the original request plus ten followed hops.
    assert_eq!(s.hits(), 11, "hop limit is not 10");
}

// ---------------------------------------------------------------------- gzip

#[test]
fn gzip_bodies_are_decoded() {
    let payload = "gzip works ".repeat(200);
    let p = payload.clone();
    let s = Server::new(move |w, _| {
        let body = gzip(p.as_bytes());
        let mut out = format!(
            "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Encoding: gzip\r\n\
             Content-Type: text/plain\r\nX-Accept-Encoding: {}\r\nContent-Length: {}\r\n\r\n",
            w.header("accept-encoding").unwrap_or("(none)"),
            body.len()
        )
        .into_bytes();
        out.extend_from_slice(&body);
        out
    });
    let res = go(&req("GET", &s.url("/gz"))).expect("gzip response");
    assert_eq!(res.status, 200);
    assert_eq!(res.text(), payload, "gzip body was not decoded");
    assert_eq!(res.size_bytes, payload.len());
    // The client advertised gzip in the first place — otherwise the decode above
    // would be testing nothing.
    let advertised = res
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("x-accept-encoding"))
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    assert!(advertised.contains("gzip"), "accept-encoding: {advertised}");
}

// -------------------------------------------------------------------- cookies

#[test]
fn a_set_cookie_comes_back_on_the_next_request() {
    let s = Server::new(|w, _| match w.path() {
        "/login" => reply(
            200,
            "OK",
            &[("Set-Cookie", "sid=abc123; Path=/"), ("Set-Cookie", "k=v")],
            b"logged in",
        ),
        _ => ok(w.header("cookie").unwrap_or("(no cookie)")),
    });
    let login = go(&req("GET", &s.url("/login"))).expect("login");
    assert!(
        login
            .headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
            .count()
            == 2,
        "both Set-Cookie headers must survive: {:?}",
        login.headers
    );

    let back = go(&req("GET", &s.url("/next"))).expect("next");
    let sent = back.text();
    assert!(sent.contains("sid=abc123"), "cookie not resent: {sent:?}");
    assert!(sent.contains("k=v"), "second cookie not resent: {sent:?}");
}

// -------------------------------------------------------------------- headers

#[test]
fn odd_response_headers_survive_intact() {
    let long = "L".repeat(4000);
    let l = long.clone();
    let s = Server::new(move |_, _| {
        reply(
            200,
            "OK",
            &[
                ("X-Multi", "one"),
                ("X-Multi", "two"),
                ("X-Empty", ""),
                ("X-Long", &l),
                ("X-Weird-Case-HEADER", "kept"),
            ],
            b"h",
        )
    });
    let res = go(&req("GET", &s.url("/h"))).expect("headers");
    let multi: Vec<&str> = res
        .headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("x-multi"))
        .map(|(_, v)| v.as_str())
        .collect();
    assert_eq!(multi, ["one", "two"], "repeated headers must both survive");
    assert_eq!(
        res.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-empty"))
            .map(|(_, v)| v.as_str()),
        Some(""),
        "an empty header value must be kept, not dropped"
    );
    assert_eq!(
        res.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-long"))
            .map(|(_, v)| v.len()),
        Some(4000)
    );
    assert!(
        res.headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("x-weird-case-header"))
    );
}

#[test]
fn request_headers_reach_the_wire_verbatim() {
    let s = Server::new(|_, _| ok("k"));
    let mut r = req("GET", &s.url("/k"));
    r.headers = vec![
        ("X-Dup".into(), "1".into()),
        ("X-Dup".into(), "2".into()),
        ("X-Empty".into(), String::new()),
        ("X-Long".into(), "v".repeat(3000)),
    ];
    go(&r).expect("send");
    let w = s.seen().pop().expect("one request");
    let dup: Vec<&str> = w
        .headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("x-dup"))
        .map(|(_, v)| v.as_str())
        .collect();
    assert_eq!(dup, ["1", "2"], "repeated request headers");
    assert_eq!(w.header("x-empty"), Some(""));
    assert_eq!(w.header("x-long").map(str::len), Some(3000));
}

// ------------------------------------------------------------- body framing

#[test]
fn chunked_no_body_and_204_all_produce_the_right_bytes() {
    let s = Server::new(|w, _| match w.path() {
        "/chunked" => b"HTTP/1.1 200 OK\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\n\
                        5\r\nhello\r\n7\r\n, world\r\n0\r\n\r\n"
            .to_vec(),
        "/empty" => reply(200, "OK", &[], b""),
        "/204" => bodyless(204, "No Content", &[]),
        _ => ok("?"),
    });
    let c = go(&req("GET", &s.url("/chunked"))).expect("chunked");
    assert_eq!(c.text(), "hello, world");
    assert_eq!(c.size_bytes, 12);

    let e = go(&req("GET", &s.url("/empty"))).expect("empty");
    assert!(e.body.is_empty());
    assert_eq!(e.size_bytes, 0);
    assert_eq!(e.status, 200);

    let n = go(&req("GET", &s.url("/204"))).expect("204");
    assert_eq!((n.status, n.status_text.as_str()), (204, "No Content"));
    assert!(n.body.is_empty());
}

#[test]
fn a_non_utf8_body_is_preserved_byte_for_byte() {
    // Invalid UTF-8 on purpose: a lone continuation byte, an unpaired
    // surrogate's encoding, a NUL and a truncated sequence.
    const RAW: &[u8] = &[0xff, 0xfe, 0x00, 0xed, 0xa0, 0x80, b'a', 0xe2, 0x82];
    let s = Server::new(|_, _| reply(200, "OK", &[("Content-Type", "text/plain")], RAW));
    let res = go(&req("GET", &s.url("/raw"))).expect("raw");
    assert_eq!(res.body, RAW, "body must not be lossily re-encoded");
    assert_eq!(res.size_bytes, RAW.len());
}

#[test]
fn a_connection_that_dies_mid_body_is_an_error() {
    // Announces 100 bytes, sends 10, then hangs up.
    let s = Server::new(|_, _| {
        let mut out =
            b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 100\r\n\r\n".to_vec();
        out.extend_from_slice(b"0123456789");
        out
    });
    let err = go(&req("GET", &s.url("/cut")))
        .expect_err("a truncated body must be an Err, never a partial Ok")
        .to_string();
    assert!(!err.is_empty());
}

#[test]
fn a_server_that_hangs_up_before_responding_is_an_error() {
    let s = Server::new(|_, _| Vec::new());
    assert!(
        go(&req("GET", &s.url("/nothing"))).is_err(),
        "an empty response must be an Err"
    );
}

/// A request must always end. The process-wide default is 120 s — too slow to
/// wait out here — so this drives the same mechanism through the per-request
/// `# @timeout` directive, which is the only bound a `.http` file can set.
#[test]
fn a_slow_server_times_out_instead_of_hanging() {
    let s = Server::new(|_, _| {
        // Longer than the bound below, so only the timeout can end this.
        std::thread::sleep(Duration::from_secs(30));
        ok("too late")
    });
    let mut r = req("GET", &s.url("/slow"));
    r.extra.insert("@timeout".into(), "1".into());

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(go(&r).map_err(|e| format!("{e:#}")));
    });
    match rx.recv_timeout(Duration::from_secs(15)) {
        Ok(outcome) => {
            let err = outcome.expect_err("a stalled server must be reported as an error");
            let msg = err.to_lowercase();
            assert!(
                msg.contains("timed out") || msg.contains("timeout"),
                "the error should say it timed out: {err}"
            );
        }
        Err(_) => panic!(
            "exec::send never returned: without .timeout()/.connect_timeout() on the \
             client, one unresponsive host wedges the app forever"
        ),
    }
}

// ------------------------------------------------------------------- metrics

#[test]
fn duration_ms_and_size_bytes_are_populated() {
    let body = "x".repeat(5000);
    let b = body.clone();
    let s = Server::new(move |_, _| {
        std::thread::sleep(Duration::from_millis(60));
        ok(&b)
    });
    let res = go(&req("GET", &s.url("/slowish"))).expect("send");
    assert_eq!(res.size_bytes, body.len());
    assert_eq!(res.size_bytes, res.body.len());
    assert!(
        res.duration_ms >= 40,
        "duration_ms {} did not measure a 60ms response",
        res.duration_ms
    );
    assert!(res.duration_ms < 60_000, "duration_ms {}", res.duration_ms);
}

// ---------------------------------------------------------------------- auth

#[test]
fn all_auth_kinds_arrive_on_the_wire() {
    let s = Server::new(|w, _| match w.path() {
        "/token" => reply(
            200,
            "OK",
            &[("Content-Type", "application/json")],
            br#"{"access_token":"oauth-tok","token_type":"Bearer"}"#,
        ),
        _ => ok("auth"),
    });
    let mut fail = Failures::default();

    let mut basic = req("GET", &s.url("/basic"));
    basic.auth = Auth::Basic {
        username: "ada".into(),
        password: "p:ss".into(),
    };
    go(&basic).expect("basic");

    let mut bearer = req("GET", &s.url("/bearer"));
    bearer.auth = Auth::Bearer {
        token: "tok-42".into(),
    };
    go(&bearer).expect("bearer");

    let mut key_header = req("GET", &s.url("/keyheader"));
    key_header.auth = Auth::ApiKey {
        key: "X-Api-Key".into(),
        value: "k-1".into(),
        placement: ApiKeyPlacement::Header,
    };
    go(&key_header).expect("api key header");

    let mut key_query = req("GET", &s.url("/keyquery"));
    key_query.auth = Auth::ApiKey {
        key: "api_key".into(),
        value: "k 2".into(),
        placement: ApiKeyPlacement::Query,
    };
    go(&key_query).expect("api key query");

    let mut oauth = req("GET", &s.url("/oauth"));
    oauth.auth = Auth::OAuth2(Box::new(OAuth2 {
        grant_type: "client_credentials".into(),
        access_token_url: s.url("/token"),
        client_id: "cid".into(),
        client_secret: "sec".into(),
        scope: "read".into(),
        ..Default::default()
    }));
    go(&oauth).expect("oauth2");

    let by_path = |p: &str| -> Wire {
        s.seen()
            .into_iter()
            .find(|w| w.path() == p)
            .unwrap_or_else(|| panic!("no request to {p}"))
    };
    // base64("ada:p:ss")
    fail.check(
        by_path("/basic").header("authorization") == Some("Basic YWRhOnA6c3M="),
        "basic",
        format!("{:?}", by_path("/basic").header("authorization")),
    );
    fail.check(
        by_path("/bearer").header("authorization") == Some("Bearer tok-42"),
        "bearer",
        "authorization",
    );
    fail.check(
        by_path("/keyheader").header("x-api-key") == Some("k-1"),
        "apikey header",
        "x-api-key",
    );
    fail.check(
        by_path("/keyheader").header("authorization").is_none(),
        "apikey header",
        "must not also set authorization",
    );
    let q = by_path("/keyquery");
    fail.check(
        q.query() == "api_key=k+2",
        "apikey query",
        format!("query {:?}", q.query()),
    );
    fail.check(
        q.header("api_key").is_none(),
        "apikey query",
        "must not also send a header",
    );

    let token_req = by_path("/token");
    fail.check(
        token_req.method == "POST",
        "oauth2",
        "token request must be a POST",
    );
    fail.check(
        token_req.text().contains("grant_type=client_credentials")
            && token_req.text().contains("client_id=cid")
            && token_req.text().contains("scope=read"),
        "oauth2",
        format!("token form {:?}", token_req.text()),
    );
    fail.check(
        by_path("/oauth").header("authorization") == Some("Bearer oauth-tok"),
        "oauth2",
        format!("{:?}", by_path("/oauth").header("authorization")),
    );
    fail.assert();
}

// ------------------------------------------------------------- interpolation

#[test]
fn interpolated_values_reach_the_wire() {
    let s = Server::new(|_, _| ok("interp"));
    let vars = Vars::from([
        ("host".to_string(), format!("http://{}", s.addr)),
        ("id".to_string(), "42".to_string()),
        ("tok".to_string(), "T-9".to_string()),
        ("name".to_string(), "ada".to_string()),
    ]);
    let mut r = req("POST", "{{host}}/users/{{id}}");
    r.headers = vec![("X-Id".into(), "{{id}}".into())];
    r.query = vec![("who".into(), "{{name}}".into())];
    r.body = Body::Json("{\"n\":\"{{name}}\",\"id\":{{id}}}".into());
    r.auth = Auth::Bearer {
        token: "{{tok}}".into(),
    };
    block_on(send(&r, &vars)).expect("send");

    let w = s.seen().pop().expect("one request");
    assert_eq!(w.method, "POST");
    assert_eq!(w.path(), "/users/42");
    assert_eq!(w.query(), "who=ada");
    assert_eq!(w.header("x-id"), Some("42"));
    assert_eq!(w.header("authorization"), Some("Bearer T-9"));
    assert_eq!(w.text(), "{\"n\":\"ada\",\"id\":42}");
}

#[test]
fn an_unresolved_variable_fails_before_any_connection() {
    let s = Server::new(|_, _| ok("never"));
    let vars = Vars::from([("host".to_string(), format!("http://{}", s.addr))]);
    let mut r = req("POST", "{{host}}/u");
    r.headers = vec![("Authorization".into(), "Bearer {{authToken}}".into())];
    r.body = Body::Json("{\"n\":\"{{userName}}\"}".into());

    let err = block_on(send(&r, &vars))
        .expect_err("unresolved variables must fail early")
        .to_string();
    assert!(err.contains("authToken"), "{err}");
    assert!(err.contains("userName"), "{err}");
    assert_eq!(s.hits(), 0, "no connection may be made: {err}");
}
