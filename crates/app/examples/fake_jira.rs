//! A fake Jira you can point the real app at, so the booking flow can be clicked
//! through end to end without a licence key.
//!
//!     cargo run -p request-pilot --example fake_jira
//!
//! Then in the app's settings: Kind = **Data Center**, Site URL
//! `http://127.0.0.1:8080`, any user name, any token — this accepts every
//! credential and says so on every request, which is the whole point of it being a
//! fake. Then in Track: Preview, Book to Jira, edit an entry, Book again, Close day.
//! Every request prints here, so the dedup is something you watch rather than
//! something you trust.
//!
//! It keeps worklogs in memory, hands out ids like Jira does, and honours POST, GET
//! and PUT on them. Restarting it empties Jira — which is itself a case worth
//! seeing: the app then reports `re-added`, because its local record points at a
//! worklog that no longer exists.
//!
//! # Why this exists next to the tests
//!
//! `crates/app/src/jira.rs` already has a stub server in its tests, and this is
//! deliberately not shared with it. That one exists to make assertions and lives
//! behind `#[cfg(test)]` in a binary crate, where an example cannot reach it;
//! exporting it would mean putting test scaffolding into the app's public library
//! for one caller. Two copies of forty lines of socket handling, each with one job,
//! beats one copy plus the plumbing to share it.
//!
//! What it is not: an implementation of Jira. It answers the four endpoints this app
//! calls, in the shapes it reads, and nothing else. A wrong path gets a 404 that
//! says so.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

/// Where it listens. The same port the Docker Jira would use, so switching between
/// the two needs no change in the app's settings.
const ADDR: &str = "127.0.0.1:8080";

/// One worklog, as this fake remembers it.
struct Worklog {
    id: String,
    issue: String,
    started: String,
    seconds: i64,
    comment: String,
}

fn main() {
    let listener = match TcpListener::bind(ADDR) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot listen on {ADDR}: {e}");
            eprintln!(
                "(the Docker Jira may still be up — `docker compose -f docker/jira-dev/compose.yaml down`)"
            );
            std::process::exit(1);
        }
    };
    println!("fake Jira on http://{ADDR} — Kind = Data Center, any user, any token");
    println!("Ctrl+C to stop. Worklogs live in memory and die with this process.\n");

    let mut worklogs: Vec<Worklog> = Vec::new();
    // Ids that look like Jira's, and ascending, so the log is readable.
    let mut next_id = 10_000;

    for stream in listener.incoming().flatten() {
        let mut reader = BufReader::new(&stream);
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).is_err() {
            continue;
        }
        let mut parts = request_line.split_whitespace();
        let (method, target) = (
            parts.next().unwrap_or_default().to_string(),
            parts.next().unwrap_or_default().to_string(),
        );

        let mut length = 0usize;
        loop {
            let mut header = String::new();
            match reader.read_line(&mut header) {
                Ok(_) if !header.trim().is_empty() => {
                    if let Some(v) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                }
                _ => break,
            }
        }
        let mut body = vec![0u8; length];
        if length > 0 && reader.read_exact(&mut body).is_err() {
            continue;
        }
        // A body that does not parse is a 400, not an empty one. This used to be an
        // `unwrap_or(Value::Null)`, and the first thing that hit it — a mangled
        // em dash from a shell — was answered with `200 {"id":"10000"}` and a
        // worklog of zero seconds. A fake whose whole job is to make the booking
        // observable must not be the thing that hides a broken request.
        let body = match (body.is_empty(), serde_json::from_slice(&body)) {
            (true, _) => serde_json::Value::Null,
            (false, Ok(value)) => value,
            (false, Err(e)) => {
                println!("  ! body is not JSON ({e}) → 400");
                let reply = error(400, &format!("request body is not valid JSON: {e}"));
                let mut out = &stream;
                let _ = out.write_all(reply.as_bytes());
                let _ = out.flush();
                continue;
            }
        };

        // The path without its query string, split into segments, so matching reads
        // as the endpoints rather than as string surgery.
        let path = target.split('?').next().unwrap_or_default();
        let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
        // Printed before anything is decided, so the order of the log is the order of
        // the work — that is the only reason this program prints at all.
        println!("{method} {target}");

        let response = match (method.as_str(), segments.as_slice()) {
            // Whoever the token belongs to. Always the same person: this fake has
            // one user, and every worklog it reports is theirs, which is what makes
            // the overlap check testable at all.
            ("GET", ["rest", "api", _, "myself"]) => ok(serde_json::json!({
                "name": "sam",
                "accountId": "acc-1",
                "displayName": "Sam (fake Jira)",
            })),

            ("GET", ["rest", "api", _, "issue", issue, "worklog"]) => {
                let mine: Vec<serde_json::Value> = worklogs
                    .iter()
                    .filter(|w| w.issue.eq_ignore_ascii_case(issue))
                    .map(|w| {
                        serde_json::json!({
                            "id": w.id,
                            "started": w.started,
                            "timeSpentSeconds": w.seconds,
                            "comment": w.comment,
                            "author": { "name": "sam", "accountId": "acc-1" },
                        })
                    })
                    .collect();
                println!("  → {} worklog(s) on {issue}", mine.len());
                ok(serde_json::json!({
                    "startAt": 0,
                    "maxResults": mine.len(),
                    "total": mine.len(),
                    "worklogs": mine,
                }))
            }

            ("POST", ["rest", "api", _, "issue", issue, "worklog"]) => {
                let id = next_id.to_string();
                next_id += 1;
                let w = Worklog {
                    id: id.clone(),
                    issue: issue.to_string(),
                    started: field(&body, "started"),
                    seconds: body
                        .get("timeSpentSeconds")
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or(0),
                    comment: comment_text(&body),
                };
                println!(
                    "  + BOOKED {issue} #{id}  {}s  {}  \"{}\"",
                    w.seconds, w.started, w.comment
                );
                worklogs.push(w);
                // Jira answers a create with the whole worklog; the app only reads
                // `id`, and that id is what keeps the next run from booking twice.
                ok(serde_json::json!({ "id": id }))
            }

            ("PUT", ["rest", "api", _, "issue", issue, "worklog", id]) => {
                match worklogs.iter_mut().find(|w| w.id == *id) {
                    Some(w) => {
                        w.started = field(&body, "started");
                        w.seconds = body
                            .get("timeSpentSeconds")
                            .and_then(serde_json::Value::as_i64)
                            .unwrap_or(w.seconds);
                        w.comment = comment_text(&body);
                        println!(
                            "  ~ CORRECTED {issue} #{id}  now {}s  \"{}\"",
                            w.seconds, w.comment
                        );
                        // 204 with no body, which is what real Jira does here and
                        // what must not be read as a parse failure.
                        "HTTP/1.1 204 No Content\r\ncontent-length: 0\r\n\r\n".to_string()
                    }
                    None => {
                        println!("  ! PUT on unknown worklog #{id} → 404");
                        error(404, &format!("no worklog {id}"))
                    }
                }
            }

            _ => {
                println!("  ! unhandled {method} {path} → 404");
                error(
                    404,
                    "this fake only answers /myself and the issue worklog endpoints",
                )
            }
        };

        let mut out = &stream;
        let _ = out.write_all(response.as_bytes());
        let _ = out.flush();
    }
}

/// A string field of the request body, or empty.
fn field(body: &serde_json::Value, key: &str) -> String {
    body.get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The comment, from either flavour's shape.
///
/// Plain text on Data Center, an Atlassian Document Format tree on Cloud. Reading
/// both is what lets this fake be pointed at with either setting, which is worth the
/// four lines: the ADF path would otherwise never be exercised outside a unit test.
fn comment_text(body: &serde_json::Value) -> String {
    let comment = &body["comment"];
    if let Some(text) = comment.as_str() {
        return text.to_string();
    }
    comment["content"][0]["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

fn ok(value: serde_json::Value) -> String {
    let body = value.to_string();
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    )
}

/// Jira's own error shape, so the app's message extraction is exercised too.
fn error(status: u16, message: &str) -> String {
    let body = serde_json::json!({ "errorMessages": [message], "errors": {} }).to_string();
    let reason = if status == 404 { "Not Found" } else { "Error" };
    format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    )
}
