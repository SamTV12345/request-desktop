//! The shipped example sheet, checked without a network.
//!
//! `examples/movies.http` is what a user is handed to try the app out, so it has
//! to keep parsing and every `{{variable}}` in it has to be answerable. It also
//! doubles as the coverage sheet for the `.http` features — writing it is what
//! turned up two directives that were parsed and then ignored.
//!
//! Deliberately offline: the three APIs it calls are free and unauthenticated,
//! and none of that makes them a dependency of the test suite.
use rp_core::{httpfile, interp, vars};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn sheet() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/movies.http")
        .canonicalize()
        .expect("examples/movies.http must exist — it is shipped, not generated")
}

/// The names the sheet expects the file, an environment or an earlier response to
/// provide, and the one name it is deliberately missing.
const DELIBERATELY_UNSET: &str = "gibtEsNicht";

#[test]
fn the_example_sheet_parses_and_every_variable_is_answerable() {
    let path = sheet();
    let src = std::fs::read_to_string(&path).expect("read the sheet");
    let (requests, decls) = httpfile::parse_with_vars(&src).expect("the sheet must parse");
    assert!(
        requests.len() >= 10,
        "the sheet is meant to cover the format broadly, got {} requests",
        requests.len()
    );

    // Start from what the sheet and its environment file declare.
    let mut known: BTreeSet<String> = decls.iter().map(|(k, _)| k.clone()).collect();
    let envs = vars::load_env_files(&path).expect("the env file next to the sheet must parse");
    assert!(
        envs.len() >= 2,
        "the sheet documents switching environments, so there must be more than one: {:?}",
        envs.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    for env in &envs {
        for (name, _, _) in &env.vars {
            known.insert(name.clone());
        }
    }

    // Walk the requests in order, adding what each response handler promises to
    // capture — which is exactly how the sheet is meant to be sent.
    let mut unresolved = Vec::new();
    for req in &requests {
        let mut vars = interp::Vars::new();
        for name in &known {
            vars.insert(name.clone(), "x".into());
        }
        for name in interp::missing(&req.url, &vars) {
            unresolved.push((req.name.clone(), name));
        }
        for (k, v) in req.headers.iter().chain(req.query.iter()) {
            for name in interp::missing(k, &vars)
                .into_iter()
                .chain(interp::missing(v, &vars))
            {
                unresolved.push((req.name.clone(), name));
            }
        }
        if let Some(script) = &req.post_response_script {
            // `capture` needs a body to evaluate against; the names it *would*
            // set are what matters here, so a body with every path is not
            // required — the setter's first argument is a literal.
            for name in setter_names(script) {
                known.insert(name);
            }
        }
    }

    let unexpected: Vec<_> = unresolved
        .iter()
        .filter(|(_, name)| name != DELIBERATELY_UNSET)
        .collect();
    assert!(
        unexpected.is_empty(),
        "the sheet references variables nothing provides: {unexpected:?}"
    );
    assert!(
        unresolved
            .iter()
            .any(|(_, name)| name == DELIBERATELY_UNSET),
        "the sheet is supposed to end with one deliberately unset variable, so the \
         'unresolved variable' message has something to show"
    );
}

/// The first argument of every `client.global.set` / `request.variables.set` in a
/// handler script.
fn setter_names(script: &str) -> Vec<String> {
    let mut out = Vec::new();
    for setter in ["client.global.set", "request.variables.set"] {
        let mut rest = script;
        while let Some(at) = rest.find(setter) {
            rest = &rest[at + setter.len()..];
            let Some(open) = rest.find('(') else { break };
            let after = &rest[open + 1..];
            let Some(first) = after.find('"') else { break };
            let tail = &after[first + 1..];
            if let Some(end) = tail.find('"') {
                out.push(tail[..end].to_string());
            }
        }
    }
    out
}

/// The sheet exists to exercise the format, so the features it is supposed to
/// carry are asserted rather than assumed. Each of these was verified against the
/// live APIs when the sheet was written.
#[test]
fn the_example_sheet_covers_the_features_it_claims_to() {
    let src = std::fs::read_to_string(sheet()).expect("read the sheet");
    let (requests, decls) = httpfile::parse_with_vars(&src).expect("parse");

    assert!(
        decls.iter().any(|(k, _)| k == "pageSize"),
        "the local/environment precedence demo needs a local @pageSize"
    );
    assert!(
        requests.iter().any(|r| r.name == "listFilms"),
        "`# @name` must survive parsing"
    );

    let has = |f: fn(&rp_core::Request) -> bool| requests.iter().any(f);
    assert!(
        has(|r| r.extra.contains_key("@no-redirect")),
        "`# @no-redirect`"
    );
    assert!(has(|r| r.extra.contains_key("@timeout")), "`# @timeout`");
    assert!(
        has(|r| r.extra.contains_key("response_redirect_force")),
        "`>>! file`"
    );
    assert!(
        has(|r| r.post_response_script.is_some()),
        "`> {{% ... %}}` response handlers"
    );
    assert!(
        has(|r| matches!(r.body, rp_core::Body::Json(_))),
        "a JSON body"
    );
    assert!(
        has(|r| matches!(r.body, rp_core::Body::Form(_))),
        "a form-urlencoded body"
    );
    assert!(has(|r| !r.query.is_empty()), "query parameters");
    assert!(has(|r| r.method.eq_ignore_ascii_case("POST")), "a POST");

    // Dynamic variables: resolved by `interp`, so they must never show up as
    // missing even though nothing declares them.
    let dynamics = ["$uuid", "$timestamp", "$isoTimestamp", "$randomInt"];
    for name in dynamics {
        assert!(
            src.contains(&format!("{{{{{name}}}}}")),
            "the sheet should demonstrate {name}"
        );
    }
    let empty = interp::Vars::new();
    for name in dynamics {
        assert!(
            interp::missing(&format!("{{{{{name}}}}}"), &empty).is_empty(),
            "{name} must resolve without being declared"
        );
    }

    // More than one host, so the sheet shows that captured values are not tied
    // to the one they came from.
    let hosts: BTreeSet<&str> = requests
        .iter()
        .filter_map(|r| r.url.split('/').nth(2))
        .collect();
    assert!(hosts.len() >= 2, "one host only: {hosts:?}");
}
