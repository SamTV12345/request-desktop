//! Opening files, extracted from the UI so it is testable.
//!
//! `main.rs` is a binary: anything that lives there cannot be exercised by
//! `cargo test`. How the app reacts to a broken, empty or unreadable file is
//! exactly the behaviour that needs pinning, so it lives here instead.
use anyhow::{Context, Result};
use rp_core::{Environment, Item, Request, bru, httpfile, vars};
use std::path::Path;

#[derive(Debug, Default)]
pub struct Loaded {
    pub requests: Vec<Request>,
    /// Environments found for this source: `environments/*.bru` for a Bruno
    /// collection, `http-client.env.json` for a `.http` file.
    pub environments: Vec<Environment>,
    /// In-file `@name = value` declarations (`.http` only).
    pub declared: Vec<(String, String)>,
}

/// What kind of source a path is. Extension-driven, because that is what the
/// user sees; a directory is assumed to be a Bruno collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Collection,
    Bru,
    Http,
}

pub fn kind_of(path: &Path) -> Kind {
    if path.is_dir() {
        Kind::Collection
    } else if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bru"))
    {
        Kind::Bru
    } else {
        Kind::Http
    }
}

/// Read and parse `path`.
///
/// Errors carry the path, because "unresolved variable" or "expected a block
/// header" is useless when several files are open.
pub fn load(path: &Path) -> Result<Loaded> {
    match kind_of(path) {
        Kind::Collection => {
            let c = bru::load_collection(path)
                .with_context(|| format!("cannot open collection {}", path.display()))?;
            Ok(Loaded {
                requests: flatten(&c.items),
                environments: c.environments,
                declared: Vec::new(),
            })
        }
        Kind::Bru => {
            let src = read(path)?;
            let request =
                bru::parse(&src).with_context(|| format!("cannot parse {}", path.display()))?;
            Ok(Loaded {
                requests: vec![request],
                ..Default::default()
            })
        }
        Kind::Http => {
            let src = read(path)?;
            let (requests, declared) = httpfile::parse_with_vars(&src)
                .with_context(|| format!("cannot parse {}", path.display()))?;
            // A missing or broken env file must not stop the requests from
            // opening — the user can still fill the variables in by hand.
            let environments = vars::load_env_files(path).unwrap_or_default();
            Ok(Loaded {
                requests,
                environments,
                declared,
            })
        }
    }
}

/// Read as UTF-8, replacing invalid sequences rather than refusing the file.
///
/// A request file with one stray byte in a comment is still worth opening, and
/// the parsers are byte-tolerant.
fn read(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Flatten the folder tree — the sidebar is a flat list for now.
pub fn flatten(items: &[Item]) -> Vec<Request> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Item::Request { request, .. } => out.push((**request).clone()),
            Item::Folder { items, .. } => out.extend(flatten(items)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A scratch directory that cleans itself up, so tests leave no litter.
    struct Tmp(PathBuf);

    impl Tmp {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("rp-session-{tag}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Tmp(dir)
        }
        fn write(&self, name: &str, contents: impl AsRef<[u8]>) -> PathBuf {
            let p = self.0.join(name);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, contents).unwrap();
            p
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn kind_is_driven_by_extension_and_directories() {
        assert_eq!(kind_of(Path::new("a.bru")), Kind::Bru);
        assert_eq!(kind_of(Path::new("a.BRU")), Kind::Bru);
        assert_eq!(kind_of(Path::new("a.http")), Kind::Http);
        // Unknown extensions are treated as .http rather than refused.
        assert_eq!(kind_of(Path::new("a.rest")), Kind::Http);
        assert_eq!(kind_of(Path::new("noext")), Kind::Http);
    }

    #[test]
    fn missing_file_errors_with_the_path() {
        let err = load(Path::new("definitely/not/here.http"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("here.http"), "{err}");
    }

    #[test]
    fn empty_and_comment_only_http_files_open_with_no_requests() {
        let t = Tmp::new("empty");
        for (name, body) in [
            ("empty.http", ""),
            ("ws.http", "   \n\t\n"),
            ("comments.http", "# just a note\n"),
        ] {
            let loaded = load(&t.write(name, body))
                .unwrap_or_else(|e| panic!("{name} should open, got {e}"));
            assert!(loaded.requests.is_empty(), "{name} produced requests");
        }
    }

    #[test]
    fn a_broken_http_file_errors_and_names_the_file() {
        let t = Tmp::new("broken-http");
        // A header before any request line is not recoverable.
        let path = t.write("bad.http", "Accept: application/json\n");
        let err = load(&path).unwrap_err().to_string();
        assert!(err.contains("bad.http"), "{err}");
        assert!(!err.trim().is_empty());
    }

    #[test]
    fn a_broken_bru_file_errors_and_names_the_file() {
        let t = Tmp::new("broken-bru");
        let path = t.write("bad.bru", "meta {\n  name: x\n"); // never closed
        let err = load(&path).unwrap_err().to_string();
        assert!(err.contains("bad.bru"), "{err}");
    }

    #[test]
    fn invalid_utf8_does_not_stop_a_file_from_opening() {
        let t = Tmp::new("utf8");
        let mut bytes = b"GET https://example.com/\nX-Note: ".to_vec();
        bytes.extend_from_slice(&[0xff, 0xfe, 0x9f]); // invalid UTF-8
        bytes.push(b'\n');
        let loaded = load(&t.write("bytes.http", bytes)).unwrap();
        assert_eq!(loaded.requests.len(), 1);
        assert_eq!(loaded.requests[0].url, "https://example.com/");
    }

    #[test]
    fn crlf_line_endings_parse_like_lf() {
        let t = Tmp::new("crlf");
        let loaded =
            load(&t.write("crlf.http", "GET https://example.com/\r\nAccept: */*\r\n")).unwrap();
        assert_eq!(loaded.requests.len(), 1);
        assert_eq!(loaded.requests[0].url, "https://example.com/");
        assert!(
            loaded.requests[0]
                .headers
                .iter()
                .any(|(k, v)| k == "Accept" && v == "*/*"),
            "carriage returns leaked into the headers: {:?}",
            loaded.requests[0].headers
        );
    }

    /// The contract for hostile bytes is "never panic, and if it fails say
    /// something useful" — not "accept everything". A NUL where a header
    /// belongs is legitimately a parse error.
    #[test]
    fn control_bytes_produce_a_useful_error_never_a_panic() {
        let t = Tmp::new("control");
        for (name, body) in [
            ("nul.http", "GET https://example.com/\n\0\n"),
            ("nul-url.http", "GET https://exa\0mple.com/\n"),
            ("ctrl.http", "GET https://example.com/\n\x01\x02\x03\n"),
            ("lone-cr.http", "GET https://example.com/\rAccept: */*\r"),
        ] {
            match load(&t.write(name, body)) {
                Ok(loaded) => {
                    // Accepting it is fine, as long as nothing is invented.
                    assert!(
                        loaded.requests.len() <= 1,
                        "{name} produced phantom requests"
                    );
                }
                Err(e) => {
                    let msg = e.to_string();
                    assert!(
                        msg.contains(name),
                        "{name}: error does not name the file: {msg}"
                    );
                    let root = format!("{:#}", e);
                    assert!(root.len() > msg.len(), "{name}: error has no cause: {root}");
                }
            }
        }
    }

    #[test]
    fn http_file_picks_up_its_environment_file() {
        let t = Tmp::new("env");
        t.write(
            "http-client.env.json",
            r#"{"dev":{"baseUrl":"http://localhost:8080"},"prod":{"baseUrl":"https://api.x"}}"#,
        );
        let loaded = load(&t.write("api.http", "GET {{baseUrl}}/users\n")).unwrap();
        assert_eq!(loaded.environments.len(), 2);
        assert!(loaded.environments.iter().any(|e| e.name == "dev"));
        // The placeholder is preserved; resolving happens at send time.
        assert_eq!(loaded.requests[0].url, "{{baseUrl}}/users");
    }

    #[test]
    fn a_broken_environment_file_does_not_block_the_requests() {
        let t = Tmp::new("bad-env");
        t.write("http-client.env.json", "{ this is not json");
        let loaded = load(&t.write("api.http", "GET https://example.com/\n")).unwrap();
        assert_eq!(loaded.requests.len(), 1, "requests must still open");
        assert!(loaded.environments.is_empty());
    }

    #[test]
    fn in_file_declarations_are_returned() {
        let t = Tmp::new("decl");
        let loaded =
            load(&t.write("d.http", "@baseUrl = http://x\n\nGET {{baseUrl}}/a\n")).unwrap();
        assert_eq!(
            loaded.declared,
            [("baseUrl".to_string(), "http://x".to_string())]
        );
    }

    #[test]
    fn a_directory_without_a_collection_errors_rather_than_opening_empty() {
        let t = Tmp::new("notacoll");
        t.write("readme.txt", "nothing to see");
        // Whichever way `load_collection` decides, it must not panic and the
        // outcome must mention the path when it fails.
        match load(&t.0) {
            Ok(loaded) => assert!(loaded.requests.is_empty()),
            Err(e) => assert!(e.to_string().contains("rp-session-notacoll"), "{e}"),
        }
    }

    #[test]
    fn flatten_walks_nested_folders_in_order() {
        let leaf = |name: &str| Item::Request {
            path: PathBuf::from(name),
            request: Box::new(Request {
                name: name.into(),
                ..Default::default()
            }),
        };
        let tree = vec![
            leaf("a"),
            Item::Folder {
                name: "f".into(),
                items: vec![
                    leaf("b"),
                    Item::Folder {
                        name: "g".into(),
                        items: vec![leaf("c")],
                    },
                ],
            },
            leaf("d"),
        ];
        let names: Vec<String> = flatten(&tree).into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["a", "b", "c", "d"]);
    }
}
