//! MCP **prompts**, which is what a client turns into slash commands.
//!
//! Tools and prompts are different things and the difference is not obvious from
//! the outside. A *tool* is called by the model; there is no way for a user to
//! invoke one by typing. A *prompt* is a canned message the user picks, and Claude
//! Code surfaces each as `/mcp__<server>__<name>`. Someone who has just seen nine
//! tools listed and then types `/log_work` gets nothing, and nothing explains why.
//!
//! So the prompts here are the typed door to the same nine tools: short, one job
//! each, and worded so the model reaches for the right tool with the right
//! arguments rather than inventing a CLI.
use serde_json::{Value, json};

/// Reads one argument, returning "" when it was not given. Passed to a prompt's
/// `render` so a missing argument is one code path rather than a `match` per use.
type Args<'a> = &'a dyn Fn(&str) -> String;

/// One prompt: its name, what it is for, and its arguments.
struct Prompt {
    name: &'static str,
    title: &'static str,
    description: &'static str,
    /// `(name, description, required)`
    arguments: &'static [(&'static str, &'static str, bool)],
    render: fn(Args<'_>) -> String,
}

/// ponytail: four prompts, not nine. One per thing a person actually types in the
/// morning or the evening; the rest of the tools are for the model to reach for on
/// its own. Add a fifth when someone misses it.
const PROMPTS: &[Prompt] = &[
    Prompt {
        name: "log_work",
        title: "Book time already worked",
        description: "Record a block of work that is already done, e.g. \"1h Keycloak debugging\".",
        arguments: &[
            (
                "what",
                "What was worked on, e.g. \"Keycloak debugging\".",
                true,
            ),
            (
                "how_long",
                "How long, e.g. \"1h\", \"90m\", \"2.5h\". Defaults to one hour.",
                false,
            ),
            (
                "when",
                "When it happened, e.g. \"this morning\", \"yesterday 14:00\". Defaults to \
                 finishing now.",
                false,
            ),
        ],
        render: |arg| {
            let what = arg("what");
            let how_long = or(arg("how_long"), "1h");
            let when = or(arg("when"), "ending now");
            format!(
                "Book {how_long} of work on \"{what}\", {when}, with the time tracker's log_time \
                 tool. Work out the start and end timestamps yourself and pass them explicitly; \
                 do not start or stop a running timer. Check list_tasks first and reuse an \
                 existing task name if one matches, so related work totals up instead of \
                 splitting across near-duplicates. Tell me the entry it created."
            )
        },
    },
    Prompt {
        name: "start_work",
        title: "Start a timer",
        description: "Begin tracking time on a task from now.",
        arguments: &[("what", "What you are starting work on.", true)],
        render: |arg| {
            let what = arg("what");
            format!(
                "Start the time tracker on \"{what}\" with start_timer. Check list_tasks first \
                 and reuse an existing name if one matches. Any timer already running is stopped \
                 automatically, so tell me what it stopped if it stopped something."
            )
        },
    },
    Prompt {
        name: "stop_work",
        title: "Stop the running timer",
        description: "Stop whatever is being tracked and report how long it ran.",
        arguments: &[],
        render: |_| {
            "Stop the running timer with stop_timer and tell me the task and how long it ran. If \
             nothing was running, say so rather than starting anything."
                .to_string()
        },
    },
    Prompt {
        name: "day_summary",
        title: "Summarise a day",
        description: "The evening report: what the day went on, per task.",
        arguments: &[(
            "date",
            "\"today\" (default), \"yesterday\", or YYYY-MM-DD.",
            false,
        )],
        render: |arg| {
            let date = or(arg("date"), "today");
            format!(
                "Give me the time tracker's summary for {date}. If a timer is still running, say \
                 so and how long it has been going, since that time is not in the totals yet."
            )
        },
    },
];

fn or(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

/// The `prompts/list` result.
pub fn definitions() -> Value {
    Value::Array(
        PROMPTS
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "title": p.title,
                    "description": p.description,
                    "arguments": p.arguments.iter().map(|(name, description, required)| json!({
                        "name": name,
                        "description": description,
                        "required": required,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// The `prompts/get` result: one user message, ready to send.
///
/// `Err` for an unknown name or a missing required argument — a prompt that
/// silently renders "Book 1h of work on """ would have the model book nonsense.
pub fn get(params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((crate::proto::INVALID_PARAMS, "name is required".to_string()))?;
    let prompt = PROMPTS.iter().find(|p| p.name == name).ok_or((
        crate::proto::INVALID_PARAMS,
        format!("Unknown prompt: {name}"),
    ))?;

    let args = params.get("arguments");
    let lookup = |key: &str| -> String {
        args.and_then(|a| a.get(key))
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default()
    };
    for (arg, _, required) in prompt.arguments {
        if *required && lookup(arg).trim().is_empty() {
            return Err((
                crate::proto::INVALID_PARAMS,
                format!("{name} needs the {arg} argument"),
            ));
        }
    }

    Ok(json!({
        "description": prompt.description,
        "messages": [{
            "role": "user",
            "content": { "type": "text", "text": (prompt.render)(&lookup) },
        }],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_prompt_is_listed_with_its_arguments() {
        let listed = definitions();
        let names: Vec<&str> = listed
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            ["log_work", "start_work", "stop_work", "day_summary"]
        );
        for p in listed.as_array().unwrap() {
            assert!(p["title"].is_string(), "{p}");
            assert!(
                p["description"].as_str().is_some_and(|d| d.len() > 20),
                "a one-word description is no help in a command palette: {p}"
            );
            assert!(p["arguments"].is_array(), "{p}");
        }
    }

    /// The whole point is that the message names the tool, or the model is back to
    /// guessing at a CLI.
    #[test]
    fn a_rendered_prompt_names_the_tool_it_wants_used() {
        for (name, args, tool) in [
            (
                "log_work",
                json!({"what": "Keycloak debugging"}),
                "log_time",
            ),
            ("start_work", json!({"what": "invoices"}), "start_timer"),
            ("stop_work", json!({}), "stop_timer"),
            ("day_summary", json!({}), "summary"),
        ] {
            let got = get(&json!({"name": name, "arguments": args})).expect(name);
            let text = got["messages"][0]["content"]["text"].as_str().unwrap();
            assert!(text.contains(tool), "{name} does not name {tool}: {text}");
            assert_eq!(got["messages"][0]["role"], "user");
        }
    }

    #[test]
    fn defaults_fill_in_and_given_values_win() {
        let text = |v: Value| {
            get(&v).unwrap()["messages"][0]["content"]["text"]
                .as_str()
                .unwrap()
                .to_string()
        };
        // Defaults.
        let d = text(json!({"name": "log_work", "arguments": {"what": "x"}}));
        assert!(d.contains("1h"), "{d}");
        assert!(d.contains("ending now"), "{d}");
        // And an argument that is present but blank counts as absent.
        let blank = text(json!({"name": "log_work", "arguments": {"what": "x", "how_long": "  "}}));
        assert!(blank.contains("1h"), "{blank}");
        // Given values.
        let g = text(json!({
            "name": "log_work",
            "arguments": {"what": "Keycloak", "how_long": "90m", "when": "yesterday afternoon"},
        }));
        assert!(
            g.contains("90m") && g.contains("yesterday afternoon"),
            "{g}"
        );
        assert!(!g.contains("1h"), "the default leaked in: {g}");
        // A number is accepted where a string was expected, rather than rendering
        // `Some(3)` or nothing at all.
        let n = text(json!({"name": "day_summary", "arguments": {"date": 20260726}}));
        assert!(n.contains("20260726"), "{n}");
    }

    #[test]
    fn a_missing_required_argument_is_an_error_not_an_empty_sentence() {
        for bad in [
            json!({"name": "log_work"}),
            json!({"name": "log_work", "arguments": {}}),
            json!({"name": "log_work", "arguments": {"what": "   "}}),
            json!({"name": "start_work", "arguments": {"how_long": "1h"}}),
        ] {
            let err = get(&bad).expect_err(&format!("{bad} should be refused"));
            assert_eq!(err.0, crate::proto::INVALID_PARAMS);
            assert!(err.1.contains("what"), "{}", err.1);
        }
        // An unknown prompt names itself.
        let err = get(&json!({"name": "nope"})).expect_err("unknown");
        assert!(err.1.contains("nope"), "{}", err.1);
        // And a params object with no name at all.
        assert!(get(&json!({})).is_err());
    }
}
