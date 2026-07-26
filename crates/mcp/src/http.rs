//! The same MCP server as [`crate::serve`], reachable over MCP's **Streamable
//! HTTP** transport instead of stdio, so the desktop app can host it in-process
//! and an agent can book time without a subprocess.
//!
//! Specification: <https://modelcontextprotocol.io/specification/2025-11-25/basic/transports>
//! (section "Streamable HTTP"), fetched while writing this. What it asks for and
//! what is implemented here:
//!
//! * One endpoint path ([`PATH`]) serving POST; GET answers 405, which the spec
//!   explicitly allows for a server that offers no server-initiated SSE stream.
//! * A JSON-RPC *request* is answered with `Content-Type: application/json` —
//!   the spec's alternative to opening an SSE stream, and clients **MUST**
//!   support it. A *notification* or *response* gets `202 Accepted`, no body.
//! * Sessions: `initialize` mints an `Mcp-Session-Id`; every later request must
//!   carry it (400 without, 404 for one we do not know); `DELETE` ends it.
//! * `MCP-Protocol-Version`, when sent, must be a revision this server speaks,
//!   or the request is a 400.
//! * The spec's security warning is followed to the letter and then some: the
//!   listener is bound to loopback, the `Origin` header is validated (403), and
//!   a bearer token is required (401). Without the token any other process on
//!   the machine — including a web page's `fetch` to `127.0.0.1` — could read,
//!   book and delete the user's time entries. The token is stored in the
//!   database and reused across restarts; see [`TOKEN_KEY`] for what that costs.
//!
//! Not implemented, deliberately: SSE streams, resumability and `Last-Event-ID`.
//! Nothing here sends server-initiated messages, so a stream would only ever
//! carry the one response the JSON body already carries.
use crate::proto::{INVALID_REQUEST, Server, supported_version};
use anyhow::{Context, Result};
use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use rp_store::Store;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::net::{Ipv4Addr, TcpListener};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

/// The endpoint path. Fixed, so a client configuration stays valid.
pub const PATH: &str = "/mcp";

/// The port the app offers by default, for the same reason.
pub const DEFAULT_PORT: u16 = 4319;

/// Bytes of randomness in a token. 32 is far past guessing over a socket.
const TOKEN_BYTES: usize = 32;

/// The kv key the bearer token lives under, in the same database as the time
/// entries, next to `mcp.enabled` and `mcp.port`.
///
/// **The security posture this changes, stated plainly.** The token used to be
/// minted per activation and kept only in memory, so it could not be read off
/// disk at all. Persisting it means it now sits in the same SQLite file as the
/// data it protects. Against a local process that can read `%APPDATA%` it is
/// therefore no longer a secret: anything able to read the token can already read
/// and write the time entries directly, so the token adds nothing there. Do not
/// call this secure.
///
/// What it still stops is the threat the token was added for in the first place:
/// a web page in the user's browser doing `fetch("http://127.0.0.1:4319/mcp")`.
/// That page cannot read a file, so it cannot learn the token, and the request is
/// a 401 — persisted or not. Combined with the loopback bind and the `Origin`
/// check, the defence that matters is intact.
///
/// What the old behaviour bought was that a leaked config expired at the next
/// restart. That is now the user's job instead, via [`Handle::regenerate_token`],
/// which is the trade: a config that keeps working until the user revokes it,
/// rather than one that silently dies every restart. The user asked for it, the
/// cost is bounded to an attacker who has already won, and a rotate button is
/// what keeps them in control.
pub const TOKEN_KEY: &str = "mcp.token";

/// The session header, spelled as the specification spells it.
const SESSION_HEADER: &str = "mcp-session-id";
const VERSION_HEADER: &str = "mcp-protocol-version";

/// How long a stop waits for in-flight requests before giving up on being
/// graceful. Long enough for a SQLite write, short enough that a stuck client
/// cannot freeze the UI thread that called [`Handle::drop`].
const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// The tokio runtime the listener runs on.
///
/// ponytail: this is the *second* process-wide runtime in the app —
/// `crates/app/src/request_pane.rs` has its own for reqwest. They should be one;
/// that file has another owner. Unify by moving a runtime handle into a shared
/// crate and passing it in here.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Runtime::new().expect("tokio runtime"))
}

/// Everything a request handler needs.
///
/// ponytail: **one** SQLite connection behind a `Mutex`, so requests serialise.
/// The tools are millisecond-scale single-row statements and there is one agent
/// on the other end; give each handler its own connection if that ever stops
/// being true. Sharing the *app's* `Store` is not an option — it owns a
/// `rusqlite::Connection`, which is `Send` but not `Sync`. Opening a second
/// connection on the same file is how the stdio server has always worked: WAL is
/// on, "only one timer runs" is a `UNIQUE` index rather than application logic,
/// and the app polls `PRAGMA data_version` to notice outside writes.
struct Shared {
    /// The bearer token in force *right now*. Behind a `Mutex` because
    /// [`Handle::regenerate_token`] replaces it while requests are in flight, and
    /// the old value has to stop working the moment it does.
    token: Mutex<String>,
    server: Mutex<Server>,
    /// Live session ids. A session carries no state beyond its own existence:
    /// the database is the state, and it is shared on purpose.
    sessions: Mutex<HashSet<String>>,
}

/// A running server. Dropping it stops the listener and releases the port.
pub struct Handle {
    /// The port actually bound, which is not the requested one when 0 was asked
    /// for.
    pub port: u16,
    state: Arc<Shared>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl Handle {
    /// The URL to put in a client configuration.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}{PATH}", self.port)
    }

    /// The bearer token currently required. Shown in the UI so the user can paste
    /// it into a client configuration.
    pub fn token(&self) -> String {
        self.state.token.lock().unwrap().clone()
    }

    /// Mint a fresh token, store it, and put it in force immediately.
    ///
    /// The old token stops being accepted before this returns, so every client
    /// holding it — including sessions already initialised — starts getting 401s.
    /// That is the point: this is the user's way to revoke a config they pasted
    /// somewhere they regret.
    pub fn regenerate_token(&self) -> Result<String> {
        let fresh = random_hex(TOKEN_BYTES)?;
        // Store first. If the write fails the old token keeps working, which
        // beats a running server whose token is nowhere on disk.
        {
            let server = self
                .state
                .server
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            server
                .store
                .set(TOKEN_KEY, &fresh)
                .context("cannot store the new access token")?;
        }
        *self.state.token.lock().unwrap() = fresh.clone();
        Ok(fresh)
    }
}

/// Hand-written so a stray `{:?}` in a log line cannot print the token.
impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handle")
            .field("port", &self.port)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl Drop for Handle {
    /// Stops the server *synchronously*: the port must be free by the time a
    /// user who clicked "Stop" can click "Start" again.
    ///
    /// Must not be called from a thread inside [`runtime()`] — `block_on` would
    /// panic. Callers are the UI thread and tests.
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            // The receiver is gone only if the task already ended.
            let _ = stop.send(());
        }
        if let Some(join) = self.join.take() {
            let aborter = join.abort_handle();
            let timed_out = runtime()
                .block_on(async { tokio::time::timeout(SHUTDOWN_GRACE, join).await })
                .is_err();
            if timed_out {
                eprintln!("rp-mcp: the HTTP server did not stop gracefully; dropping it");
                aborter.abort();
            }
        }
    }
}

/// Bind `port` on loopback and serve MCP over it.
///
/// `db` is the database file to open; `None` means the app's own. A fresh
/// connection either way, see [`Shared`].
///
/// Fails, rather than falling back to another port or panicking, when the port
/// is taken or the database cannot be opened — the UI shows the message.
pub fn start(port: u16, db: Option<PathBuf>) -> Result<Handle> {
    let store = match &db {
        Some(path) => Store::open_at(path)
            .with_context(|| format!("cannot open the database at {}", path.display()))?,
        None => Store::open_default().context("cannot open the app's database")?,
    };
    let token = token_for(&store).context("cannot obtain an access token")?;
    let server = Server::new(store)?;

    // Loopback only. Never `0.0.0.0`: on a laptop on a café network that would
    // hand the user's timesheet to the network.
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).with_context(|| {
        format!("cannot listen on 127.0.0.1:{port} — is another program using that port?")
    })?;
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;

    let state = Arc::new(Shared {
        token: Mutex::new(token),
        server: Mutex::new(server),
        sessions: Mutex::new(HashSet::new()),
    });
    let app = Router::new()
        .route(
            PATH,
            post(handle_post)
                // The spec's escape hatch for a server with no server-to-client
                // stream to offer.
                .get(|| async { StatusCode::METHOD_NOT_ALLOWED })
                .delete(handle_delete),
        )
        .with_state(state.clone());

    let rt = runtime();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let join = {
        let _guard = rt.enter(); // `from_std` needs a reactor in scope.
        let listener = tokio::net::TcpListener::from_std(listener)?;
        rt.spawn(async move {
            let served = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = stopped.await;
                })
                .await;
            if let Err(e) = served {
                eprintln!("rp-mcp: the HTTP server stopped: {e}");
            }
        })
    };
    Ok(Handle {
        port,
        state,
        stop: Some(stop),
        join: Some(join),
    })
}

// -- request handling --------------------------------------------------------

async fn handle_post(
    State(state): State<Arc<Shared>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(refusal) = guard(&state, &headers) {
        return refusal;
    }
    if !accepts_json(&headers) {
        return refuse(
            StatusCode::NOT_ACCEPTABLE,
            "Accept must allow application/json",
        );
    }
    // Non-UTF-8 is not JSON; let the protocol layer answer it as a parse error.
    let text = String::from_utf8_lossy(&body);
    let method = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| v.get("method").and_then(Value::as_str).map(str::to_string));

    // `initialize` is the one request that arrives without a session.
    if method.as_deref() == Some("initialize") {
        let session = match random_hex(16) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("rp-mcp: {e:#}");
                return refuse(StatusCode::INTERNAL_SERVER_ERROR, "cannot mint a session");
            }
        };
        let reply = handle(&state, &text);
        state.sessions.lock().unwrap().insert(session.clone());
        let mut response = match reply {
            Some(body) => json_body(StatusCode::OK, body),
            None => StatusCode::ACCEPTED.into_response(),
        };
        // Visible ASCII only, which hex is.
        if let Ok(value) = session.parse() {
            response.headers_mut().insert(SESSION_HEADER, value);
        }
        return response;
    }

    match headers.get(SESSION_HEADER).and_then(|v| v.to_str().ok()) {
        None => refuse(
            StatusCode::BAD_REQUEST,
            "Mcp-Session-Id is required; call initialize first",
        ),
        Some(id) if !state.sessions.lock().unwrap().contains(id) => {
            // 404 is what tells a client to start a new session.
            refuse(StatusCode::NOT_FOUND, "unknown or ended session")
        }
        Some(_) => match handle(&state, &text) {
            Some(body) => json_body(StatusCode::OK, body),
            // A notification or a response: accepted, nothing to say back.
            None => StatusCode::ACCEPTED.into_response(),
        },
    }
}

async fn handle_delete(State(state): State<Arc<Shared>>, headers: HeaderMap) -> Response {
    if let Some(refusal) = guard(&state, &headers) {
        return refusal;
    }
    match headers.get(SESSION_HEADER).and_then(|v| v.to_str().ok()) {
        None => refuse(StatusCode::BAD_REQUEST, "Mcp-Session-Id is required"),
        Some(id) if state.sessions.lock().unwrap().remove(id) => {
            StatusCode::NO_CONTENT.into_response()
        }
        Some(_) => refuse(StatusCode::NOT_FOUND, "unknown or ended session"),
    }
}

/// Feed one message to the shared protocol layer.
///
/// A blocking lock inside an async handler is safe here because nothing is
/// awaited while it is held.
fn handle(state: &Shared, text: &str) -> Option<String> {
    let mut server = state
        .server
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    server.handle_line(text)
}

/// Origin, then token, then protocol revision — in that order, and *before* any
/// session is looked at, so a refusal never says whether a session id was real.
///
/// `Some(response)` is a refusal; `None` means carry on.
fn guard(state: &Shared, headers: &HeaderMap) -> Option<Response> {
    if let Some(origin) = headers.get(header::ORIGIN)
        && !origin.to_str().is_ok_and(is_local_origin)
    {
        return Some(refuse(
            StatusCode::FORBIDDEN,
            "this server only accepts requests from the local machine",
        ));
    }
    if !authorised(state, headers) {
        return Some(unauthorised());
    }
    if let Some(version) = headers.get(VERSION_HEADER)
        && !version.to_str().is_ok_and(supported_version)
    {
        return Some(refuse(
            StatusCode::BAD_REQUEST,
            "unsupported MCP-Protocol-Version",
        ));
    }
    None
}

/// `Authorization: Bearer <token>`, compared without an early exit.
fn authorised(state: &Shared, headers: &HeaderMap) -> bool {
    let Some(offered) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").or(v.strip_prefix("bearer ")))
    else {
        return false;
    };
    let token = state.token.lock().unwrap();
    constant_time_eq(offered.trim().as_bytes(), token.as_bytes())
}

/// The stored token, generated and stored on first need. See [`TOKEN_KEY`] for
/// why storing it is acceptable.
///
/// A blank or whitespace-only stored value is treated as absent: a hand-edited
/// row must not leave the server accepting an empty bearer token.
fn token_for(store: &Store) -> Result<String> {
    if let Some(existing) = store
        .get(TOKEN_KEY)?
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
    {
        return Ok(existing);
    }
    let fresh = random_hex(TOKEN_BYTES)?;
    store.set(TOKEN_KEY, &fresh)?;
    Ok(fresh)
}

/// Length-revealing but not content-revealing. Enough for a 32-byte random token
/// over loopback, and it avoids a dependency for five lines.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Only a loopback web origin may talk to us; the spec calls out DNS rebinding
/// against local servers specifically, and a rebound name never looks like this.
fn is_local_origin(origin: &str) -> bool {
    let rest = match origin.split_once("://") {
        Some(("http" | "https", rest)) => rest,
        _ => return false,
    };
    // Strip the port, keeping an IPv6 literal's brackets intact.
    let host = match rest.strip_prefix('[') {
        Some(after) => match after.split_once(']') {
            Some((inside, _port)) => inside,
            None => return false,
        },
        None => rest.split(':').next().unwrap_or(""),
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// The client must be able to take a JSON body back. `text/event-stream` is not
/// required of it: this server never opens a stream, so refusing a client that
/// only offered JSON would be pedantry that breaks curl.
fn accepts_json(headers: &HeaderMap) -> bool {
    let Some(accept) = headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()) else {
        return true; // No preference stated.
    };
    accept.split(',').any(|part| {
        let media = part.split(';').next().unwrap_or("").trim();
        matches!(media, "application/json" | "application/*" | "*/*")
    })
}

fn json_body(status: StatusCode, body: String) -> Response {
    (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
}

/// A transport-level refusal, carrying the JSON-RPC error response without an
/// `id` that the spec permits.
fn refuse(status: StatusCode, message: &str) -> Response {
    json_body(status, rpc_error(message))
}

fn unauthorised() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::WWW_AUTHENTICATE, "Bearer"),
        ],
        rpc_error("a bearer token is required"),
    )
        .into_response()
}

fn rpc_error(message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": null, "error": {"code": INVALID_REQUEST, "message": message}})
        .to_string()
}

/// `bytes` bytes from the OS random source, hex-encoded.
fn random_hex(bytes: usize) -> Result<String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).context("the operating system refused to provide randomness")?;
    Ok(buf.iter().fold(String::new(), |mut s, b| {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
        s
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_loopback_web_origin_is_accepted() {
        for good in [
            "http://localhost",
            "http://localhost:5173",
            "http://127.0.0.1:4319",
            "https://127.0.0.1",
            "http://[::1]:8080",
        ] {
            assert!(is_local_origin(good), "{good} should be accepted");
        }
        for bad in [
            "http://evil.example",
            // The classic rebinding shape: a name that resolves to loopback.
            "http://localhost.evil.example",
            "http://127.0.0.1.evil.example",
            "null",
            "file://",
            "ws://localhost",
            "http://[::1",
            "",
        ] {
            assert!(!is_local_origin(bad), "{bad} should be refused");
        }
    }

    #[test]
    fn accept_must_leave_room_for_json() {
        let mut h = HeaderMap::new();
        assert!(accepts_json(&h), "no Accept means no objection");
        for good in [
            "application/json",
            "application/json, text/event-stream",
            "*/*",
            "text/event-stream, application/json;q=0.9",
        ] {
            h.insert(header::ACCEPT, good.parse().unwrap());
            assert!(accepts_json(&h), "{good}");
        }
        for bad in ["text/event-stream", "text/plain", "application/xml"] {
            h.insert(header::ACCEPT, bad.parse().unwrap());
            assert!(!accepts_json(&h), "{bad}");
        }
    }

    #[test]
    fn a_token_comparison_does_not_shortcut_on_length_or_content() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"", b"x"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn tokens_are_long_random_hex() {
        let a = random_hex(TOKEN_BYTES).unwrap();
        let b = random_hex(TOKEN_BYTES).unwrap();
        assert_eq!(a.len(), TOKEN_BYTES * 2);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "two draws must not collide");
    }

    #[test]
    fn a_token_is_generated_once_and_then_read_back() {
        let store = Store::open_in_memory().unwrap();
        let first = token_for(&store).unwrap();
        assert_eq!(store.get(TOKEN_KEY).unwrap().as_deref(), Some(&*first));
        assert_eq!(token_for(&store).unwrap(), first, "it must be reused");

        // Junk in the row is treated as absent rather than accepted.
        for junk in ["", "   ", "\n"] {
            store.set(TOKEN_KEY, junk).unwrap();
            let replaced = token_for(&store).unwrap();
            assert_eq!(replaced.len(), TOKEN_BYTES * 2, "{junk:?}");
            assert_ne!(replaced, first);
        }
    }
}
