//! An MCP server that books time into the desktop app's local time tracker, so
//! an agent can track its own work and the user sees it live.
//!
//! Two transports onto one protocol layer: [`serve`] is newline-delimited
//! JSON-RPC on stdio, used by the `rp-mcp` binary; [`http`] is MCP's Streamable
//! HTTP transport, started from the desktop app's UI. Both call the same
//! [`Server::handle_line`], so the tools cannot drift apart.
//!
//! This library exists so the protocol layer can be tested without spawning a
//! process. See README-mcp.md for registration.
//!
//! **stdout carries the protocol** for the stdio transport. Nothing in this crate
//! may write to it except [`serve`]. All diagnostics go to stderr.
pub mod http;
pub mod proto;
pub mod time;
pub mod tools;

pub use proto::Server;

use anyhow::{Context, Result};
use std::io::{BufRead, Write};

/// Longest line accepted. A client sending more than this is broken or hostile;
/// truncating bounds memory and the truncated line comes back as a parse error.
const MAX_LINE: usize = 1 << 20;

/// Read requests until stdin closes, writing one JSON-RPC message per line.
///
/// Returns `Ok(())` on a clean EOF, which is how an MCP client shuts a stdio
/// server down.
pub fn serve(server: &mut Server, input: impl BufRead, mut output: impl Write) -> Result<()> {
    let mut input = input;
    let mut line = Vec::with_capacity(4096);
    while read_line_capped(&mut input, &mut line, MAX_LINE)? {
        // Non-UTF-8 is not JSON; lossy conversion lets it fail as a parse error
        // with the offending line visible, instead of ending the session.
        let text = String::from_utf8_lossy(&line);
        if let Some(reply) = server.handle_line(&text) {
            output.write_all(reply.as_bytes())?;
            output.write_all(b"\n")?;
            // Unflushed, the client waits forever and calls us a hung server.
            output.flush().context("cannot write to stdout")?;
        }
    }
    Ok(())
}

/// Read one `\n`-terminated line into `out`, keeping at most `cap` bytes.
///
/// `Ok(false)` means EOF with nothing left over. Deliberately not
/// [`BufRead::read_line`]: that would let one unterminated line grow until the
/// process is killed.
fn read_line_capped(
    input: &mut impl BufRead,
    out: &mut Vec<u8>,
    cap: usize,
) -> std::io::Result<bool> {
    out.clear();
    let mut byte = [0u8; 1];
    loop {
        match input.read(&mut byte) {
            Ok(0) => return Ok(!out.is_empty()),
            Ok(_) => {}
            // A signal-interrupted read is not the end of the stream.
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
        if byte[0] == b'\n' {
            return Ok(true);
        }
        if out.len() < cap {
            out.push(byte[0]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rp_store::Store;

    fn run(input: &str) -> String {
        let mut srv = Server::new(Store::open_in_memory().unwrap()).unwrap();
        let mut out = Vec::new();
        serve(
            &mut srv,
            std::io::BufReader::new(input.as_bytes()),
            &mut out,
        )
        .unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn one_reply_per_request_and_nothing_for_notifications() {
        let out = run(concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            "\n",
        ));
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "got {lines:?}");
        for line in &lines {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(v["jsonrpc"], "2.0");
        }
    }

    #[test]
    fn a_final_line_without_a_newline_is_still_answered() {
        let out = run(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
        assert!(out.contains("\"result\""), "{out}");
        assert!(out.ends_with('\n'), "every reply is newline-terminated");
    }

    #[test]
    fn a_closed_stdin_ends_the_loop_cleanly() {
        assert_eq!(run(""), "");
        assert_eq!(run("\n\n\n"), "", "blank lines are not requests");
    }

    #[test]
    fn an_over_long_line_is_a_parse_error_and_the_session_continues() {
        let huge = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"ping","pad":"{}"}}"#,
            "x".repeat(MAX_LINE + 10_000)
        );
        let out = run(&format!(
            "{huge}\n{}\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#
        ));
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("-32700"), "{}", lines[0]);
        assert!(
            lines[1].contains("\"result\""),
            "the next request must still work"
        );
    }

    #[test]
    fn invalid_utf8_does_not_end_the_session() {
        let mut srv = Server::new(Store::open_in_memory().unwrap()).unwrap();
        let mut out = Vec::new();
        let mut input: Vec<u8> = vec![b'{', 0xff, 0xfe, b'}', b'\n'];
        input.extend_from_slice(br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
        serve(&mut srv, std::io::BufReader::new(&input[..]), &mut out).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert_eq!(out.lines().count(), 2, "{out}");
        assert!(out.lines().next().unwrap().contains("-32700"));
    }
}
