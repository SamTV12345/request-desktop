//! Newline-delimited JSON-RPC 2.0 over stdio, and the MCP methods on top of it.
//!
//! Hand-rolled on `serde_json`: see README-mcp.md for why there is no SDK here.
//!
//! Two invariants the rest of the crate leans on:
//!
//! * **Nothing in this module can panic on input.** Every malformed line, unknown
//!   method or missing field is a response, because a crash takes the user's
//!   session with it.
//! * **A request with no `id` is a notification** and gets no reply at all, not
//!   even an error. Answering one corrupts the client's request bookkeeping.
use crate::{time::Clock, tools};
use anyhow::Result;
use rp_store::Store;
use serde_json::{Value, json};

/// The revision this server is written against.
pub const PROTOCOL_VERSION: &str = "2025-11-25";

/// Revisions we will answer with if the client asks for one of them. Ordered
/// newest first; the shapes this server uses are identical across all of them.
const SUPPORTED_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

pub struct Server {
    pub store: Store,
    pub clock: Clock,
}

impl Server {
    pub fn new(store: Store) -> Result<Self> {
        Ok(Self {
            store,
            clock: Clock::new()?,
        })
    }

    /// Handle one line of input. `None` means "send nothing back".
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            // No id is recoverable from unparseable input, so it is null per spec.
            Err(e) => {
                return Some(error_str(
                    Value::Null,
                    PARSE_ERROR,
                    &format!("Parse error: {e}"),
                ));
            }
        };
        // ponytail: batches (an array at the top level) were removed from MCP in
        // 2025-06-18 and no client sends them. Rejected rather than implemented.
        let obj = match msg.as_object() {
            Some(o) => o,
            None => {
                return Some(error_str(
                    Value::Null,
                    INVALID_REQUEST,
                    "Invalid Request: expected a single JSON-RPC object per line",
                ));
            }
        };
        // Absent id = notification. `"id": null` is not a valid request id either,
        // so both are treated as "do not reply".
        let id = obj.get("id").filter(|v| !v.is_null()).cloned();
        let method = match obj.get("method").and_then(Value::as_str) {
            Some(m) => m,
            None => {
                return id.map(|id| {
                    error_str(
                        id,
                        INVALID_REQUEST,
                        "Invalid Request: \"method\" must be a string",
                    )
                });
            }
        };
        let params = obj.get("params").cloned().unwrap_or(Value::Null);

        let outcome = self.dispatch(method, &params);
        // Notifications get silence even when they were nonsense.
        let id = id?;
        Some(match outcome {
            Ok(result) => to_line(&json!({"jsonrpc": "2.0", "id": id, "result": result})),
            Err((code, message)) => error_str(id, code, &message),
        })
    }

    fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, (i64, String)> {
        match method {
            "initialize" => Ok(self.initialize(params)),
            // Both spelled forms show up in the wild; neither needs a reply.
            "notifications/initialized" | "initialized" => Ok(Value::Null),
            n if n.starts_with("notifications/") => Ok(Value::Null),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::definitions() })),
            "tools/call" => tools::call(self, params),
            _ => Err((METHOD_NOT_FOUND, format!("Method not found: {method}"))),
        }
    }

    fn initialize(&self, params: &Value) -> Value {
        // Echo the client's revision when we know it, otherwise offer ours and let
        // the client decide whether to keep talking.
        let asked = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or("");
        let version = if SUPPORTED_VERSIONS.contains(&asked) {
            asked
        } else {
            PROTOCOL_VERSION
        };
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "rp-time-tracker",
                "title": "Request Pilot time tracker",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "instructions": "Local time tracking shared live with the Request Pilot \
        desktop app. When you start working on a task for the user, call start_timer; call \
        stop_timer when you finish or switch away. Book work you did before this conversation \
        with log_time. At the end of the day, summary gives the user a readable report of \
        where the time went.",
        })
    }
}

fn to_line(v: &Value) -> String {
    // A Value built here cannot fail to serialise, but a panic in the protocol
    // loop is never worth it.
    serde_json::to_string(v).unwrap_or_else(|_| {
        r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Internal error"}}"#
            .to_string()
    })
}

fn error_str(id: Value, code: i64, message: &str) -> String {
    to_line(&json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server::new(Store::open_in_memory().unwrap()).unwrap()
    }

    /// Send a line, expect a reply, parse it.
    fn ask(srv: &mut Server, line: &str) -> Value {
        let reply = srv.handle_line(line).expect("expected a reply");
        assert!(!reply.contains('\n'), "a reply must be one line: {reply}");
        let v: Value = serde_json::from_str(&reply).expect("reply must be JSON");
        assert_eq!(v["jsonrpc"], "2.0");
        v
    }

    #[test]
    fn initialize_answers_with_a_version_capabilities_and_server_info() {
        let mut srv = server();
        let v = ask(
            &mut srv,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"c","version":"1"}}}"#,
        );
        assert_eq!(v["id"], 1);
        let r = &v["result"];
        assert_eq!(
            r["protocolVersion"], "2025-06-18",
            "a known revision must be echoed"
        );
        assert!(
            r["capabilities"]["tools"].is_object(),
            "the tools capability must be declared"
        );
        assert!(r["serverInfo"]["name"].is_string());
        assert!(r["serverInfo"]["version"].is_string());
        assert!(r["error"].is_null());
    }

    #[test]
    fn an_unknown_protocol_version_gets_ours_rather_than_an_error() {
        let mut srv = server();
        let v = ask(
            &mut srv,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1.0.0"}}"#,
        );
        assert_eq!(v["result"]["protocolVersion"], PROTOCOL_VERSION);
    }

    #[test]
    fn initialize_survives_missing_params() {
        let mut srv = server();
        let v = ask(
            &mut srv,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
        );
        assert_eq!(v["result"]["protocolVersion"], PROTOCOL_VERSION);
    }

    #[test]
    fn the_initialized_notification_gets_no_reply() {
        let mut srv = server();
        assert_eq!(
            srv.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
            None
        );
        // Unknown notifications are ignored too, not answered with an error.
        assert_eq!(
            srv.handle_line(
                r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#
            ),
            None
        );
        // A null id is not a request id.
        assert_eq!(
            srv.handle_line(r#"{"jsonrpc":"2.0","id":null,"method":"notifications/initialized"}"#),
            None
        );
    }

    #[test]
    fn tools_list_describes_every_tool_with_a_usable_object_schema() {
        let mut srv = server();
        let v = ask(
            &mut srv,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        );
        let tools = v["result"]["tools"]
            .as_array()
            .expect("tools must be an array");

        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        for expected in [
            "start_timer",
            "stop_timer",
            "log_time",
            "current_timer",
            "list_entries",
            "summary",
            "summary_month",
            "list_tasks",
            "delete_entry",
        ] {
            assert!(
                names.contains(&expected),
                "missing tool {expected}: {names:?}"
            );
        }
        assert_eq!(names.len(), 9, "unexpected tool list: {names:?}");

        for t in tools {
            let name = t["name"].as_str().unwrap();
            let desc = t["description"].as_str().unwrap_or("");
            assert!(
                desc.len() > 40,
                "{name} needs a description an agent can act on"
            );
            let schema = &t["inputSchema"];
            assert_eq!(schema["type"], "object", "{name} schema must be an object");
            // Every declared property needs a type and a description, or the
            // agent guesses.
            if let Some(props) = schema["properties"].as_object() {
                for (prop, spec) in props {
                    assert!(spec["type"].is_string(), "{name}.{prop} has no type");
                    assert!(
                        spec["description"].is_string(),
                        "{name}.{prop} has no description"
                    );
                }
            }
            // `required` must name properties that exist.
            for req in schema["required"].as_array().unwrap_or(&vec![]) {
                let req = req.as_str().unwrap();
                assert!(
                    schema["properties"].get(req).is_some(),
                    "{name} requires {req}, which is not in properties"
                );
            }
        }
    }

    #[test]
    fn an_unknown_method_is_method_not_found() {
        let mut srv = server();
        let v = ask(
            &mut srv,
            r#"{"jsonrpc":"2.0","id":3,"method":"resources/list"}"#,
        );
        assert_eq!(v["error"]["code"], METHOD_NOT_FOUND);
        assert!(
            v["error"]["message"]
                .as_str()
                .unwrap()
                .contains("resources/list")
        );
        assert!(v["result"].is_null());
    }

    #[test]
    fn malformed_input_is_a_parse_error_not_a_crash() {
        let mut srv = server();
        for bad in [
            "{not json",
            r#"{"jsonrpc":"2.0","id":1,"method":"#, // truncated line
            "\u{feff}{}",
        ] {
            let v = ask(&mut srv, bad);
            assert_eq!(v["error"]["code"], PARSE_ERROR, "{bad}");
            assert!(v["id"].is_null());
        }
    }

    #[test]
    fn a_non_object_or_method_less_message_is_an_invalid_request() {
        let mut srv = server();
        assert_eq!(ask(&mut srv, "[]")["error"]["code"], INVALID_REQUEST);
        assert_eq!(ask(&mut srv, "42")["error"]["code"], INVALID_REQUEST);
        let v = ask(&mut srv, r#"{"jsonrpc":"2.0","id":9,"params":{}}"#);
        assert_eq!(v["error"]["code"], INVALID_REQUEST);
        assert_eq!(v["id"], 9, "an id we do have must come back");
        // Without an id there is nobody to answer.
        assert_eq!(srv.handle_line(r#"{"jsonrpc":"2.0","params":{}}"#), None);
    }

    #[test]
    fn blank_lines_are_skipped() {
        let mut srv = server();
        assert_eq!(srv.handle_line(""), None);
        assert_eq!(srv.handle_line("   \r\n"), None);
    }

    #[test]
    fn a_string_id_comes_back_as_a_string() {
        let mut srv = server();
        let v = ask(
            &mut srv,
            r#"{"jsonrpc":"2.0","id":"abc-1","method":"ping"}"#,
        );
        assert_eq!(v["id"], "abc-1");
        assert!(v["result"].is_object());
    }

    #[test]
    fn calling_an_unknown_tool_is_invalid_params() {
        let mut srv = server();
        let v = ask(
            &mut srv,
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
        );
        assert_eq!(v["error"]["code"], INVALID_PARAMS);
        assert!(v["error"]["message"].as_str().unwrap().contains("nope"));
    }

    #[test]
    fn a_tools_call_without_a_name_is_invalid_params() {
        let mut srv = server();
        let v = ask(
            &mut srv,
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{}}"#,
        );
        assert_eq!(v["error"]["code"], INVALID_PARAMS);
    }

    #[test]
    fn bad_tool_arguments_come_back_as_is_error_not_a_protocol_error() {
        let mut srv = server();
        // Missing a required argument.
        let v = ask(
            &mut srv,
            r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"start_timer","arguments":{}}}"#,
        );
        assert!(
            v["error"].is_null(),
            "a validation failure is a result, not an RPC error"
        );
        assert_eq!(v["result"]["isError"], true);
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(v["result"]["content"][0]["type"], "text");
        assert!(
            text.to_lowercase().contains("task"),
            "unhelpful message: {text}"
        );

        // Wrong type.
        let v = ask(
            &mut srv,
            r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"start_timer","arguments":{"task":42}}}"#,
        );
        assert_eq!(v["result"]["isError"], true);
        assert!(
            v["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("task")
        );

        // Unparseable timestamp: the case that would otherwise book the wrong day.
        let v = ask(
            &mut srv,
            r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"log_time","arguments":{"task":"x","started_at":"yesterday afternoon","stopped_at":"later"}}}"#,
        );
        assert_eq!(v["result"]["isError"], true);
        assert!(
            v["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("started_at")
        );

        // Arguments of the wrong shape entirely.
        let v = ask(
            &mut srv,
            r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"summary","arguments":[1,2]}}"#,
        );
        assert_eq!(v["result"]["isError"], true);
    }

    #[test]
    fn a_no_argument_tool_may_be_called_without_an_arguments_field() {
        let mut srv = server();
        // The store stubs make this fail, but it must fail as a tool error with a
        // readable message rather than a protocol error or a panic.
        let v = ask(
            &mut srv,
            r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"list_tasks"}}"#,
        );
        assert!(v["error"].is_null());
        assert!(v["result"]["content"][0]["text"].is_string());
    }
}
