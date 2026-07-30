//! Talk to the real binary over real pipes.
//!
//! Everything else in this crate tests `handle_line` in-process, which cannot
//! catch the mistakes that actually break an MCP client: a stray `println!`, an
//! unflushed stdout, a log line on the protocol channel.
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Feed the server some lines, close stdin, return (stdout lines, stderr).
fn talk(requests: &[&str]) -> (Vec<Value>, String) {
    // Tests in one binary run in parallel, so the pid alone is not unique enough.
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let db = std::env::temp_dir().join(format!(
        "rp-mcp-subprocess-{}-{}.db",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&db);

    let mut child = Command::new(env!("CARGO_BIN_EXE_rp-mcp"))
        .arg("--db")
        .arg(&db)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cannot spawn rp-mcp");

    let mut stdin = child.stdin.take().unwrap();
    for r in requests {
        stdin.write_all(r.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
    }
    // Closing stdin is how an MCP client shuts a stdio server down.
    drop(stdin);

    let out = child.wait_with_output().expect("rp-mcp did not exit");
    assert!(out.status.success(), "rp-mcp exited with {:?}", out.status);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    let lines = BufReader::new(&out.stdout[..])
        .lines()
        .map(|l| l.unwrap())
        .map(|l| {
            serde_json::from_str(&l)
                .unwrap_or_else(|e| panic!("stdout line is not JSON-RPC ({e}): {l}"))
        })
        .collect();
    let _ = std::fs::remove_file(&db);
    (lines, stderr)
}

#[test]
fn a_real_session_over_real_pipes() {
    let (replies, stderr) = talk(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test-client","version":"0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"summary","arguments":{"date":"today"}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"log_time","arguments":{"task":"x","started_at":"whenever","stopped_at":"whenever"}}}"#,
    ]);

    // Three requests, three replies -- the notification must not produce one.
    assert_eq!(replies.len(), 4, "{replies:#?}");
    assert_eq!(
        replies
            .iter()
            .filter_map(|r| r["id"].as_i64())
            .collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );

    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
    assert!(replies[0]["result"]["serverInfo"]["name"].is_string());
    assert!(replies[0]["result"]["capabilities"]["tools"].is_object());

    assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(), 10);

    // The store's track_* methods are stubs today, so this call reports a failure
    // rather than a summary. Either way it must be a well-formed tool result.
    let call = &replies[2]["result"];
    assert_eq!(call["content"][0]["type"], "text");
    assert!(call["content"][0]["text"].as_str().unwrap().len() > 5);
    assert!(call["isError"].is_boolean());

    // Bad input is refused by us, not by the store, so this one is deterministic.
    let refused = &replies[3]["result"];
    assert_eq!(refused["isError"], true);
    assert!(
        refused["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("started_at")
    );

    // The banner belongs on stderr and nowhere near stdout.
    assert!(stderr.contains("ready on stdio"), "stderr: {stderr}");
}

#[test]
fn nonsense_on_stdin_neither_crashes_nor_stops_the_session() {
    let (replies, _) = talk(&[
        "{ this is not json",
        "",
        "[]",
        r#"{"jsonrpc":"2.0","id":7,"method":"no/such/method"}"#,
        r#"{"jsonrpc":"2.0","id":8,"method":"ping"}"#,
    ]);
    let codes: Vec<i64> = replies
        .iter()
        .filter_map(|r| r["error"]["code"].as_i64())
        .collect();
    assert_eq!(codes, [-32700, -32600, -32601], "{replies:#?}");
    assert!(
        replies.last().unwrap()["result"].is_object(),
        "the session must survive"
    );
}
