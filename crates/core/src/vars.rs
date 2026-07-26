//! Where a `{{var}}` value comes from: an IntelliJ `http-client.env.json`, an
//! in-file `@name = value` declaration, or a previous response captured by a
//! `> {% client.global.set(...) %}` handler.
//!
//! The handler scripts are JavaScript, but we do not run JavaScript. We scan for
//! the handful of assignment forms people actually write and evaluate those.
use crate::interp::Vars;
use crate::model::Environment;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::Path;

const SHARED: &str = "http-client.env.json";
const PRIVATE: &str = "http-client.private.env.json";
const PREFIX: &str = "http-client.";
const SUFFIX: &str = ".env.json";

/// The environment files in `dir`, in the order they are applied.
///
/// **Exactly the two names IntelliJ knows**, and no others: `http-client.env.json`
/// is the one you check in, `http-client.private.env.json` the one you do not, and
/// the private file's values override the shared file's.
///
/// Reading any `http-client.<anything>.env.json` as a third overlay was tried here
/// and reverted: a file that resolves variables in this app and not in IntelliJ is
/// a worse outcome than one that loads nowhere, because the divergence only shows
/// up when someone else opens the project. [`stray_env_files`] is the answer
/// instead — such a file is still not read, but nobody has to guess why.
///
/// The `bool` is "treat this as a secret", which is what the private file is for.
fn env_files_in(dir: &Path) -> [(std::path::PathBuf, bool); 2] {
    [(dir.join(SHARED), false), (dir.join(PRIVATE), true)]
}

/// Files next to `http_file` that look like environment files but are not read.
///
/// A `http-client.secret.env.json` is a reasonable thing to have invented, and
/// before this it simply did nothing: no error, no note, just variables that never
/// resolved. Naming it beats guessing. Searches the same directories as
/// [`load_env_files`], nearest first, so the closest surprise is reported first.
pub fn stray_env_files(http_file: &Path) -> Vec<String> {
    let dir = http_file.parent().unwrap_or(Path::new("."));
    let mut out = Vec::new();
    for d in dir.ancestors() {
        let Ok(entries) = std::fs::read_dir(d) else {
            continue;
        };
        let mut here: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| {
                name.starts_with(PREFIX)
                    && name.ends_with(SUFFIX)
                    && name != SHARED
                    && name != PRIVATE
            })
            .collect();
        here.sort();
        out.extend(here);
    }
    out
}

/// Environments for a `.http` file: `{ "dev": { "baseUrl": "..." } }`, read from
/// the file's directory and every parent, nearer directories winning, and the
/// overlay files winning over the shared one. Missing files are not an error.
pub fn load_env_files(http_file: &Path) -> Result<Vec<Environment>> {
    let dir = http_file.parent().unwrap_or(Path::new("."));
    let mut envs: BTreeMap<String, BTreeMap<String, (String, bool)>> = BTreeMap::new();
    let mut dirs: Vec<&Path> = dir.ancestors().collect();
    dirs.reverse(); // root first, so a nearer directory overrides it
    for d in dirs {
        for (path, secret) in env_files_in(d) {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let parsed: serde_json::Value = serde_json::from_str(&text)
                .with_context(|| format!("{} is not valid JSON", path.display()))?;
            let Some(obj) = parsed.as_object() else {
                continue;
            };
            for (env_name, vars) in obj {
                let Some(vars) = vars.as_object() else {
                    continue;
                };
                let slot = envs.entry(env_name.clone()).or_default();
                for (k, v) in vars {
                    slot.insert(k.clone(), (stringify(v), secret));
                }
            }
        }
    }
    Ok(envs
        .into_iter()
        .map(|(name, vars)| Environment {
            name,
            vars: vars.into_iter().map(|(k, (v, s))| (k, v, s)).collect(),
        })
        .collect())
}

/// Variables a response handler script sets, in script order. Unsupported
/// expressions and paths that don't resolve are skipped, never an error.
pub fn capture(
    script: &str,
    response_body: &[u8],
    status: u16,
    headers: &[(String, String)],
) -> Vec<(String, String)> {
    const SETTERS: &[&str] = &["client.global.set", "request.variables.set"];
    let json: Option<serde_json::Value> = serde_json::from_slice(response_body).ok();
    let mut out = Vec::new();
    for (i, _) in script.char_indices() {
        let rest = &script[i..];
        let Some(setter) = SETTERS.iter().find(|s| rest.starts_with(**s)) else {
            continue;
        };
        let Some(args) = args_of(rest[setter.len()..].trim_start()) else {
            continue;
        };
        if args.len() < 2 {
            continue;
        }
        let Some(name) = literal(&args[0]) else {
            continue;
        };
        if let Some(v) = eval(&args[1], json.as_ref(), status, headers) {
            out.push((name, v));
        }
    }
    out
}

/// The executor's variable map: `base`, then `overrides` (later wins), with one
/// pass of `{{ref}}` substitution between them.
pub fn merge(base: &Vars, overrides: &[(String, String)]) -> Vars {
    let mut out = base.clone();
    out.extend(overrides.iter().cloned());
    let snapshot = out.clone();
    for (k, v) in out.iter_mut() {
        if !v.contains("{{") {
            continue;
        }
        // ponytail: one pass, and a var can't see itself — that is the whole
        // recursion guard. `@a = {{a}}` stays verbatim, as it would anywhere else.
        let mut src = snapshot.clone();
        src.remove(k);
        *v = crate::interp::interpolate(v, &src);
    }
    out
}

// ---------------------------------------------------------------- scanner

fn stringify(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Splits `(a, b)` — starting at the `(` — into its top-level arguments.
fn args_of(s: &str) -> Option<Vec<String>> {
    let body = s.strip_prefix('(')?;
    let mut args = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for (i, c) in body.char_indices() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '(' | '[' | '{' => depth += 1,
                ')' if depth == 0 => {
                    args.push(body[start..i].to_string());
                    return Some(args);
                }
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    args.push(body[start..i].to_string());
                    start = i + 1;
                }
                _ => {}
            },
        }
    }
    None // unbalanced
}

/// A quoted string literal, unquoted.
fn literal(expr: &str) -> Option<String> {
    let t = expr.trim();
    let q = t.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let inner = t.strip_prefix(q)?.strip_suffix(q)?;
    Some(inner.replace("\\\"", "\"").replace("\\'", "'"))
}

fn eval(
    expr: &str,
    json: Option<&serde_json::Value>,
    status: u16,
    headers: &[(String, String)],
) -> Option<String> {
    let t = expr.trim().trim_end_matches(';').trim();
    if let Some(s) = literal(t) {
        return Some(s);
    }
    if t.parse::<f64>().is_ok() || t == "true" || t == "false" {
        return Some(t.to_string());
    }
    if t == "response.status" {
        return Some(status.to_string());
    }
    if t.starts_with("response.headers.") {
        let want = literal(&args_of(&t[t.find('(')?..])?.first()?.clone())?;
        return headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(&want))
            .map(|(_, v)| v.clone());
    }
    let path = t.strip_prefix("response.body")?;
    body_path(json?, path)
}

/// `.foo.bar`, `["a"]["b"]`, `[0].id` against a parsed response body.
fn body_path(json: &serde_json::Value, mut p: &str) -> Option<String> {
    let mut cur = json;
    loop {
        p = p.trim();
        if let Some(r) = p.strip_prefix('.') {
            let end = r
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
                .unwrap_or(r.len());
            if end == 0 {
                return None;
            }
            cur = cur.get(&r[..end])?;
            p = &r[end..];
        } else if let Some(r) = p.strip_prefix('[') {
            let end = r.find(']')?;
            let key = r[..end].trim().trim_matches(|c| c == '"' || c == '\'');
            cur = match key.parse::<usize>() {
                Ok(n) => cur.get(n)?,
                Err(_) => cur.get(key)?,
            };
            p = &r[end + 1..];
        } else if p.is_empty() {
            return Some(stringify(cur));
        } else {
            return None; // something we don't understand: skip it
        }
    }
}

#[cfg(test)]
mod env_file_tests {
    use super::*;

    struct Tmp(std::path::PathBuf);

    impl Tmp {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("rp-env-{tag}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Tmp(dir)
        }
        fn write(&self, name: &str, body: &str) -> std::path::PathBuf {
            let p = self.0.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, body).unwrap();
            p
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn value(envs: &[Environment], env: &str, key: &str) -> Option<(String, bool)> {
        envs.iter()
            .find(|e| e.name == env)
            .and_then(|e| e.vars.iter().find(|(k, _, _)| k == key))
            .map(|(_, v, secret)| (v.clone(), *secret))
    }

    /// The split the pair of files is *for*: URLs checked in, passwords beside them
    /// in the untracked file, and the untracked value winning.
    #[test]
    fn the_private_file_overrides_the_checked_in_one_and_is_marked_secret() {
        let t = Tmp::new("private");
        t.write(
            "http-client.env.json",
            r#"{"dev":{"baseUrl":"https://dev.example","password":"placeholder"}}"#,
        );
        t.write(
            "http-client.private.env.json",
            r#"{"dev":{"password":"s3cret"}}"#,
        );
        let file = t.write(
            "api.http",
            "GET {{baseUrl}}/x
",
        );
        let envs = load_env_files(&file).unwrap();

        assert_eq!(
            value(&envs, "dev", "baseUrl"),
            Some(("https://dev.example".into(), false)),
            "the shared file's value, and not marked secret"
        );
        assert_eq!(
            value(&envs, "dev", "password"),
            Some(("s3cret".into(), true)),
            "the private file wins, and its values are secrets"
        );
    }

    /// Exactly two names, as in IntelliJ. A third file under a name of one's own is
    /// **not** read — and is reported rather than ignored, which is the whole
    /// difference between this and the silence it replaces.
    #[test]
    fn a_file_under_another_name_is_not_read_but_is_named() {
        let t = Tmp::new("stray");
        t.write("http-client.env.json", r#"{"dev":{"token":"shared"}}"#);
        t.write(
            "http-client.secret.env.json",
            r#"{"dev":{"token":"secret"}}"#,
        );
        let file = t.write("api.http", "GET {{token}}\n");

        let envs = load_env_files(&file).unwrap();
        assert_eq!(
            value(&envs, "dev", "token"),
            Some(("shared".into(), false)),
            "a name IntelliJ does not know must not resolve variables here either"
        );
        assert_eq!(stray_env_files(&file), ["http-client.secret.env.json"]);

        // Renaming it to the name IntelliJ uses makes it work, and the report goes
        // away — which is the fix the report points at.
        std::fs::rename(
            t.0.join("http-client.secret.env.json"),
            t.0.join("http-client.private.env.json"),
        )
        .unwrap();
        assert_eq!(
            value(&load_env_files(&file).unwrap(), "dev", "token"),
            Some(("secret".into(), true))
        );
        assert!(stray_env_files(&file).is_empty());
    }

    /// Only `http-client.*.env.json` is even a candidate for the report. A directory
    /// holds all sorts of JSON, and calling `package.json` a stray environment file
    /// would be noise nobody can act on.
    #[test]
    fn unrelated_json_files_are_neither_read_nor_reported() {
        let t = Tmp::new("unrelated");
        t.write("http-client.env.json", r#"{"dev":{"a":"1"}}"#);
        t.write("package.json", r#"{"dev":{"a":"stolen"}}"#);
        t.write("env.json", r#"{"dev":{"a":"stolen"}}"#);
        t.write("http-client.env.json.bak", r#"{"dev":{"a":"stolen"}}"#);
        t.write("secret.env.json", r#"{"dev":{"a":"stolen"}}"#);
        let file = t.write(
            "api.http",
            "GET {{a}}
",
        );
        let envs = load_env_files(&file).unwrap();
        assert_eq!(value(&envs, "dev", "a"), Some(("1".into(), false)));
        assert!(
            stray_env_files(&file).is_empty(),
            "{:?}",
            stray_env_files(&file)
        );
    }

    /// A nearer directory wins, and an overlay up the tree still applies where a
    /// nearer file does not mention the name.
    #[test]
    fn a_nearer_directory_overrides_a_parent_but_does_not_erase_it() {
        let t = Tmp::new("nested");
        t.write(
            "http-client.env.json",
            r#"{"dev":{"baseUrl":"https://root","shared":"root"}}"#,
        );
        t.write("http-client.private.env.json", r#"{"dev":{"pw":"rootpw"}}"#);
        t.write(
            "deep/http-client.env.json",
            r#"{"dev":{"baseUrl":"https://deep"}}"#,
        );
        let file = t.write(
            "deep/api.http",
            "GET {{baseUrl}}
",
        );
        let envs = load_env_files(&file).unwrap();
        assert_eq!(
            value(&envs, "dev", "baseUrl"),
            Some(("https://deep".into(), false))
        );
        assert_eq!(value(&envs, "dev", "shared"), Some(("root".into(), false)));
        assert_eq!(
            value(&envs, "dev", "pw"),
            Some(("rootpw".into(), true)),
            "a secret from a parent still applies"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway directory tree under the system temp dir.
    struct Tmp(std::path::PathBuf);
    impl Tmp {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!("rp-vars-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Tmp(p)
        }
        fn write(&self, rel: &str, text: &str) -> std::path::PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, text).unwrap();
            p
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn vars_of(envs: &[Environment], name: &str) -> Vars {
        envs.iter()
            .find(|e| e.name == name)
            .map(|e| {
                e.vars
                    .iter()
                    .map(|(k, v, _)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn env_files_missing_is_empty() {
        let t = Tmp::new("missing");
        let f = t.write("a.http", "GET /\n");
        assert!(load_env_files(&f).unwrap().is_empty());
    }

    #[test]
    fn private_file_overrides_and_marks_secret() {
        let t = Tmp::new("private");
        t.write(
            SHARED,
            r#"{"dev": {"baseUrl": "http://localhost:8080", "token": "public"},
                "prod": {"baseUrl": "https://api.example.com"}}"#,
        );
        t.write(PRIVATE, r#"{"dev": {"token": "s3cret"}}"#);
        let f = t.write("a.http", "GET /\n");

        let envs = load_env_files(&f).unwrap();
        assert_eq!(
            envs.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["dev", "prod"]
        );
        let dev = &envs[0];
        assert_eq!(vars_of(&envs, "dev")["token"], "s3cret");
        assert_eq!(vars_of(&envs, "dev")["baseUrl"], "http://localhost:8080");
        assert!(
            dev.vars.iter().find(|(k, ..)| k == "token").unwrap().2,
            "private = secret"
        );
        assert!(!dev.vars.iter().find(|(k, ..)| k == "baseUrl").unwrap().2);
    }

    #[test]
    fn walks_up_parent_directories_nearest_wins() {
        let t = Tmp::new("walk");
        t.write(
            SHARED,
            r#"{"dev": {"baseUrl": "http://root", "shared": "yes"}}"#,
        );
        t.write(
            &format!("deep/{SHARED}"),
            r#"{"dev": {"baseUrl": "http://deep"}}"#,
        );
        let f = t.write("deep/a.http", "GET /\n");

        let envs = load_env_files(&f).unwrap();
        let dev = vars_of(&envs, "dev");
        assert_eq!(dev["baseUrl"], "http://deep");
        assert_eq!(dev["shared"], "yes");
    }

    #[test]
    fn non_string_env_values_are_stringified() {
        let t = Tmp::new("types");
        t.write(
            SHARED,
            r#"{"dev": {"port": 8080, "debug": true, "list": [1,2]}}"#,
        );
        let f = t.write("a.http", "GET /\n");
        let dev = vars_of(&load_env_files(&f).unwrap(), "dev");
        assert_eq!(dev["port"], "8080");
        assert_eq!(dev["debug"], "true");
        assert_eq!(dev["list"], "[1,2]");
    }

    #[test]
    fn in_file_declarations() {
        let src = "\
@baseUrl = http://localhost:8080
@token = {{baseUrl}}/token
# @name notAVar
GET {{baseUrl}}/x
Accept: */*
";
        let (reqs, decls) = crate::httpfile::parse_with_vars(src).unwrap();
        assert_eq!(
            decls,
            vec![
                ("baseUrl".to_string(), "http://localhost:8080".to_string()),
                ("token".to_string(), "{{baseUrl}}/token".to_string()),
            ]
        );
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].name, "notAVar");
        assert_eq!(reqs[0].headers.len(), 1);

        // and a declaration that references another resolves through merge()
        let v = merge(&Vars::new(), &decls);
        assert_eq!(v["token"], "http://localhost:8080/token");
    }

    #[test]
    fn merge_later_wins_and_guards_recursion() {
        let base: Vars = [("a".to_string(), "1".to_string())].into_iter().collect();
        let v = merge(
            &base,
            &[("a".into(), "2".into()), ("self".into(), "{{self}}".into())],
        );
        assert_eq!(v["a"], "2");
        assert_eq!(v["self"], "{{self}}"); // left verbatim, no hang
    }

    // ------------------------------------------------------------- capture

    const BODY: &[u8] = br#"{"token":"abc123","user":{"id":7,"name":"ada"},
        "items":[{"id":"i0"},{"id":"i1"}]}"#;

    fn cap(script: &str) -> Vec<(String, String)> {
        capture(script, BODY, 201, &[("X-Token".into(), "hdr-tok".into())])
    }

    #[test]
    fn capture_body_paths() {
        assert_eq!(
            cap(r#"client.global.set("authToken", response.body.token);"#),
            [("authToken".to_string(), "abc123".to_string())]
        );
        assert_eq!(
            cap(r#"client.global.set("n", response.body.user.name);"#)[0].1,
            "ada"
        );
        assert_eq!(
            cap(r#"client.global.set("n", response.body["user"]["id"]);"#)[0].1,
            "7"
        );
        assert_eq!(
            cap(r#"client.global.set("n", response.body.items[1].id);"#)[0].1,
            "i1"
        );
    }

    #[test]
    fn capture_headers_status_and_literals() {
        assert_eq!(
            cap(r#"client.global.set("t", response.headers.valueOf("x-token"));"#)[0].1,
            "hdr-tok"
        );
        assert_eq!(
            cap(r#"client.global.set("t", response.headers.header("X-Token"));"#)[0].1,
            "hdr-tok"
        );
        assert_eq!(
            cap(r#"client.global.set("t", "a literal, string");"#)[0].1,
            "a literal, string"
        );
        assert_eq!(cap(r#"client.global.set("t", 42);"#)[0].1, "42");
        assert_eq!(
            cap(r#"client.global.set("t", response.status);"#)[0].1,
            "201"
        );
    }

    #[test]
    fn capture_request_variables_and_ignores_the_rest() {
        let got = cap("\
client.log(\"hello\");
client.test(\"ok\", function() { client.assert(response.status === 201); });
request.variables.set(\"id\", response.body.user.id);
client.global.set(\"missing\", response.body.nope.deep);
client.global.set(\"weird\", someFunction(1));
");
        assert_eq!(got, [("id".to_string(), "7".to_string())]);
    }

    #[test]
    fn capture_survives_non_json_body() {
        let got = capture(
            r#"client.global.set("a", response.body.token); client.global.set("b", "lit");"#,
            b"<html>not json</html>",
            200,
            &[],
        );
        assert_eq!(got, [("b".to_string(), "lit".to_string())]);
    }

    #[test]
    fn capture_of_broken_script_is_empty() {
        assert!(cap(r#"client.global.set("a", response.body.token"#).is_empty()); // unbalanced
        assert!(cap("client.global.set()").is_empty());
    }

    // ------------------------------------------------------------- fixture

    #[test]
    fn shipped_fixture_resolves_base_url() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/http");
        let file = dir.join("api.http");
        let src = std::fs::read_to_string(&file).unwrap();
        let (reqs, decls) = crate::httpfile::parse_with_vars(&src).unwrap();

        let envs = load_env_files(&file).unwrap();
        let mut vars = merge(&vars_of(&envs, "dev"), &decls);
        assert!(vars.contains_key("baseUrl"), "dev env must define baseUrl");

        // the login handler feeds authToken to every later request
        let login = &reqs[0];
        let captured = capture(
            login.post_response_script.as_deref().unwrap(),
            br#"{"token":"tok-42"}"#,
            200,
            &[],
        );
        vars = merge(&vars, &captured);
        assert_eq!(vars["authToken"], "tok-42");

        let url = crate::interp::interpolate(&reqs[1].url, &vars);
        assert!(url.starts_with("http://"), "{url}");
        assert!(!url.contains("{{"), "{url}");
        let auth = crate::interp::interpolate(&reqs[1].headers[1].1, &vars);
        assert_eq!(auth, "Bearer tok-42");
    }
}
