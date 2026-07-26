//! Corpus, invariant, malformed-input and mutation suite for the IntelliJ
//! `.http` parser.
//!
//! Layout:
//!   `tests/corpus/http/valid/`     — files that must parse, round-trip and
//!                                    match a recorded expectation.
//!   `tests/corpus/http/malformed/` — files that must produce `Ok` with sensible
//!                                    contents or `Err` with a useful message,
//!                                    and must never panic.
//!
//! Every invariant test collects *all* failures and reports them with file
//! paths, so one broken corpus file does not hide the other thirty-seven.
//!
//! No dependencies beyond what rp-core already has: the PRNG and the directory
//! walker are hand-rolled below.

use rp_core::httpfile::{parse, parse_with_vars, serialize};
use rp_core::interp::{self, Vars};
use rp_core::model::Body;
use rp_core::vars;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------- fixtures

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/http")
}

/// Hand-rolled recursive walker: `walkdir` is not a dependency and never will be
/// for something this small.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "http") {
            out.push(path);
        }
    }
}

fn files(sub: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(&corpus_dir().join(sub), &mut out);
    out.sort();
    out
}

/// Corpus files deliberately contain NUL bytes and invalid UTF-8, so everything
/// is read as bytes and lossily decoded — exactly what a real editor would hand
/// the parser.
fn read_src(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    String::from_utf8_lossy(&bytes).into_owned()
}

fn short(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

/// Reports every failure at once instead of dying on the first one.
struct Failures {
    what: &'static str,
    items: Vec<String>,
}
impl Failures {
    fn new(what: &'static str) -> Self {
        Failures {
            what,
            items: Vec::new(),
        }
    }
    fn add(&mut self, path: &Path, msg: impl std::fmt::Display) {
        self.items.push(format!("  {}: {msg}", path.display()));
    }
    fn assert(self) {
        assert!(
            self.items.is_empty(),
            "{} failure(s) in {}:\n{}",
            self.items.len(),
            self.what,
            self.items.join("\n")
        );
    }
}

// ---------------------------------------------------------------- STEP 2: invariants

/// Per-file expectation: (file name, request count, request names in order).
///
/// This is the table that catches a parser which silently merges or drops
/// requests, or which renames them. It is hand-written from the corpus, not
/// generated from parser output.
#[rustfmt::skip]
const EXPECTED: &[(&str, usize, &[&str])] = &[
    ("body-file-encoding.http", 2, &["latin1", "no-encoding"]),
    ("body-file-ref.http", 2, &["from-file", "from-file-relative-parent"]),
    ("body-form-urlencoded.http", 1, &["send-sms"]),
    ("body-graphql.http", 2, &["repo-issues", "graphql-method"]),
    ("body-json.http", 1, &["create-issue"]),
    ("body-multipart.http", 1, &["upload-avatar"]),
    ("body-text.http", 1, &["plain"]),
    ("body-xml.http", 1, &["soap-envelope"]),
    ("comments-styles.http", 2, &["first", "second"]),
    ("directives-misc.http", 4, &["no-redirect", "no-cookie-jar", "timeout", "several"]),
    ("handlers-file.http", 2, &["handler-file", "handler-file-and-response-ref"]),
    ("handlers-inline.http", 2, &["capture-token", "one-liner"]),
    ("headers-comments-interleaved.http", 1, &["interleaved"]),
    ("headers-empty-and-casing.http", 1, &["empty-and-casing"]),
    ("headers-folding.http", 1, &["folded"]),
    ("headers-repeated.http", 1, &["repeated"]),
    ("http-version.http", 3, &["v11", "v10", "v2"]),
    ("method-less.http", 3, &["absolute", "relative", "with-query"]),
    ("methods-custom.http", 4, &["purge", "lock", "mkcol", "propfind"]),
    ("methods-standard.http", 10, &[
        "get", "post", "put", "patch", "delete", "head", "options", "trace", "connect", "websocket",
    ]),
    ("names-directives.http", 3, &["loginUser", "fetchProfile", "no-directive-here"]),
    ("pre-request-inline.http", 2, &["signed", "short-pre-request"]),
    ("real-elasticsearch.http", 4, &["cluster-health", "create-index", "bulk-index", "search"]),
    ("real-github.http", 3, &["list-my-repos", "create-gist", "rate-limit"]),
    ("real-httpbin.http", 6, &[
        "get-with-args", "post-json", "post-form", "basic-auth", "status-code", "redirect",
    ]),
    ("real-jsonplaceholder.http", 7, &[
        "list-posts", "one-post", "create-post", "update-post", "patch-post", "delete-post",
        "nested",
    ]),
    ("real-keycloak-login.http", 2, &["login", "whoami"]),
    ("real-kubernetes.http", 4, &[
        "list-pods", "strategic-merge-patch", "watch-events", "delete-pod",
    ]),
    ("real-stripe.http", 3, &["create-customer", "create-payment-intent", "list-charges"]),
    ("response-refs.http", 2, &["with-ref", "ref-after-body"]),
    ("separators-named.http", 4, &[
        "plain name", "spaced name", "trailing hashes", "https://example.com/4",
    ]),
    ("separators-unnamed.http", 3, &[
        "https://example.com/a", "https://example.com/b", "https://example.com/c",
    ]),
    ("url-continuation.http", 2, &["split-path", "split-with-query"]),
    ("url-forms.http", 5, &["port", "userinfo", "ipv6", "ipv6-loopback", "percent-encoded"]),
    ("url-query-split.http", 1, &["query-lines"]),
    ("url-very-long.http", 1, &["long"]),
    ("vars-in-file.http", 2, &["list", "get-one"]),
    ("vars-placeholders.http", 2, &["templated", "templated-multipart"]),
];

#[test]
fn corpus_is_big_enough() {
    // Floor so a broken glob or a lost directory cannot make the suite pass
    // vacuously.
    let valid = files("valid");
    let malformed = files("malformed");
    assert!(
        valid.len() >= 30,
        "valid corpus shrank to {} files",
        valid.len()
    );
    assert!(
        malformed.len() >= 30,
        "malformed corpus shrank to {} files",
        malformed.len()
    );
    assert_eq!(
        valid.len(),
        EXPECTED.len(),
        "every valid corpus file needs a row in EXPECTED; have {} files and {} rows",
        valid.len(),
        EXPECTED.len()
    );
}

#[test]
fn every_valid_file_parses() {
    let mut f = Failures::new("every_valid_file_parses");
    for path in files("valid") {
        match parse(&read_src(&path)) {
            Ok(reqs) if reqs.is_empty() => f.add(&path, "parsed to zero requests"),
            Ok(_) => {}
            Err(e) => f.add(&path, e),
        }
    }
    f.assert();
}

#[test]
fn valid_files_match_recorded_expectations() {
    let mut f = Failures::new("valid_files_match_recorded_expectations");
    for path in files("valid") {
        let name = short(&path);
        let Some((_, count, names)) = EXPECTED.iter().find(|(n, ..)| *n == name) else {
            f.add(&path, "no EXPECTED row for this file");
            continue;
        };
        let reqs = match parse(&read_src(&path)) {
            Ok(r) => r,
            Err(e) => {
                f.add(&path, e);
                continue;
            }
        };
        if reqs.len() != *count {
            f.add(
                &path,
                format!("expected {count} requests, parsed {}", reqs.len()),
            );
        }
        let got: Vec<&str> = reqs.iter().map(|r| r.name.as_str()).collect();
        if got != *names {
            f.add(&path, format!("names {:?} != expected {:?}", got, names));
        }
        // `seq` must be dense file order — the sidebar depends on it.
        for (i, r) in reqs.iter().enumerate() {
            if r.seq != i as u32 {
                f.add(&path, format!("request {i} has seq {}", r.seq));
            }
            if r.method.is_empty() {
                f.add(&path, format!("request {i} has an empty method"));
            }
            if r.name.is_empty() {
                f.add(&path, format!("request {i} has an empty name"));
            }
        }
    }
    f.assert();
}

#[test]
fn round_trip_is_idempotent() {
    let mut f = Failures::new("round_trip_is_idempotent");
    for path in files("valid") {
        let Ok(a) = parse(&read_src(&path)) else {
            continue;
        }; // reported elsewhere
        let text = serialize(&a);
        match parse(&text) {
            Err(e) => f.add(&path, format!("reparse of serialized form failed: {e}")),
            Ok(b) if a != b => {
                let diff = a
                    .iter()
                    .zip(b.iter())
                    .position(|(x, y)| x != y)
                    .map(|i| format!("first differing request is #{i}: {:?} vs {:?}", a[i], b[i]))
                    .unwrap_or_else(|| format!("length {} vs {}", a.len(), b.len()));
                f.add(&path, format!("round-trip mismatch, {diff}"));
            }
            Ok(_) => {}
        }
    }
    f.assert();
}

#[test]
fn serialize_is_stable() {
    let mut f = Failures::new("serialize_is_stable");
    for path in files("valid") {
        let Ok(a) = parse(&read_src(&path)) else {
            continue;
        };
        let once = serialize(&a);
        let Ok(b) = parse(&once) else { continue };
        let twice = serialize(&b);
        if once != twice {
            let at = once
                .bytes()
                .zip(twice.bytes())
                .position(|(x, y)| x != y)
                .unwrap_or(once.len().min(twice.len()));
            f.add(
                &path,
                format!("serialize is not byte-stable, first difference at byte {at}"),
            );
        }
    }
    f.assert();
}

/// Every `{{placeholder}}` in a corpus file must survive parsing verbatim —
/// either into the serialized requests or into the `@var` declarations.
#[test]
fn placeholders_survive_verbatim() {
    let mut f = Failures::new("placeholders_survive_verbatim");
    let mut total = 0usize;
    for path in files("valid") {
        let src = read_src(&path);
        let Ok((reqs, decls)) = parse_with_vars(&src) else {
            continue;
        };
        let mut haystack = serialize(&reqs);
        for (k, v) in &decls {
            haystack.push_str(k);
            haystack.push('=');
            haystack.push_str(v);
            haystack.push('\n');
        }
        for ph in placeholders(&src) {
            total += 1;
            if !haystack.contains(&ph) {
                f.add(
                    &path,
                    format!("placeholder {ph} did not survive parse+serialize"),
                );
            }
        }
    }
    assert!(
        total >= 40,
        "corpus should exercise many placeholders, found {total}"
    );
    f.assert();
}

/// All `{{...}}` spans in `src`, deduplicated, in first-seen order.
fn placeholders(src: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = src;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let span = format!("{{{{{}}}}}", &after[..end]);
        if !after[..end].contains('{') && !out.contains(&span) {
            out.push(span);
        }
        rest = &after[end + 2..];
    }
    out
}

#[test]
fn corpus_env_files_load() {
    let file = corpus_dir().join("valid/real-keycloak-login.http");
    let envs = vars::load_env_files(&file).expect("load_env_files");
    let names: Vec<&str> = envs.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        ["dev", "prod"],
        "corpus ships a dev and a prod environment"
    );

    let dev = &envs[0];
    let get = |k: &str| dev.vars.iter().find(|(n, ..)| n == k).cloned();
    assert_eq!(get("authBase").unwrap().1, "http://localhost:8081");
    assert_eq!(
        get("port").unwrap().1,
        "8080",
        "non-string values are stringified"
    );
    assert_eq!(get("verbose").unwrap().1, "true");
    // the .private. file supplies `password` and marks it secret
    let (_, pw, secret) = get("password").expect("private env var");
    assert_eq!(pw, "hunter2");
    assert!(
        secret,
        "values from http-client.private.env.json must be flagged secret"
    );
    assert!(
        !get("authBase").unwrap().2,
        "shared env values are not secret"
    );
}

// ---------------------------------------------------------------- STEP 3: malformed

/// What a malformed file is contractually allowed to do. Never a panic, never a
/// hang: always `Ok` with contents we then assert, or `Err` whose message
/// contains the recorded substring.
/// Named `Reqs`/`Fails` rather than `Ok`/`Err` so it cannot shadow `Result`.
#[derive(Clone, Copy)]
enum Out {
    /// `Ok`, with this many requests.
    Reqs(usize),
    /// `Err`, and the message must contain this.
    Fails(&'static str),
}
use Out::{Fails as E, Reqs as O};

/// (file, outcome, the defect the file encodes).
///
/// Most malformed files carry a `# defect: …` comment naming the defect, but
/// some cannot (an empty file, a whitespace-only file, a BOM prefix, a leading
/// `###`) because a comment would destroy the very defect under test. The third
/// column here is the authoritative description for all of them.
#[rustfmt::skip]
const MALFORMED: &[(&str, Out, &str)] = &[
    ("absurd-header-value.http", O(1), "a 100000-character header value"),
    ("body-without-blank-line.http", E("malformed header line"), "body not separated by a blank line"),
    ("bom-prefix.http", O(1), "UTF-8 BOM before the request line"),
    ("deep-indentation.http", O(1), "every line indented by eight spaces"),
    ("duplicate-name-directives.http", O(1), "two conflicting @name directives"),
    ("empty.http", O(0), "completely empty file"),
    ("garbage-method.http", O(1), "garbage where the method and URI belong"),
    ("gt-no-path.http", E("empty response handler reference"), "`>` with no path"),
    ("header-before-request-line.http", E("expected a request line"), "header where the request line belongs"),
    ("header-colon-no-name.http", E("malformed header line"), "colon with no header name"),
    ("header-no-colon.http", E("malformed header line"), "header line with no colon"),
    ("huge-single-line-url.http", O(1), "a 200000-character URL"),
    ("invalid-utf8.http", O(1), "raw non-UTF-8 bytes"),
    ("json-body-unterminated.http", O(1), "JSON body that is not valid JSON"),
    ("lone-cr.http", O(1), "lone CR used as the line ending"),
    ("lt-at-no-path.http", E("needs a file path"), "`<@` with no path"),
    ("lt-no-path.http", E("needs a file path"), "`<` with no path"),
    ("method-only.http", E("has no URL"), "request line that is only a method"),
    ("mixed-crlf-lf.http", O(2), "CRLF and LF line endings mixed in one file"),
    ("multipart-boundary-mismatch.http", E("multipart boundary mismatch"), "Content-Type boundary does not match the body"),
    ("multipart-no-boundary.http", E("boundary"), "multipart with no boundary anywhere"),
    ("multipart-no-closing-boundary.http", O(1), "multipart with no closing --boundary--"),
    ("multipart-no-content-disposition.http", O(1), "multipart part with no Content-Disposition"),
    ("nested-double-braces.http", O(1), "nested and doubled {{ }} braces"),
    ("nul-bytes.http", O(1), "NUL bytes inside the URL and a header"),
    ("only-comments.http", O(0), "nothing but comments"),
    ("only-separators.http", O(0), "nothing but ### separators"),
    ("request-line-looks-like-header.http", E("expected a request line"), "sole request line parses as a header"),
    ("sep-first-and-last.http", O(1), "### as both the first and the last line"),
    ("stray-percent-brace.http", O(1), "stray %} with no opening {%"),
    ("tabs-instead-of-spaces.http", O(1), "tabs instead of spaces throughout"),
    ("ten-thousand-requests.http", O(10_000), "ten thousand requests in one file"),
    ("unclosed-double-brace.http", O(1), "a {{ that is never closed"),
    ("unterminated-inline-handler.http", E("unterminated"), "`> {%` handler never closed"),
    ("unterminated-pre-request-handler.http", E("unterminated"), "`< {%` handler never closed"),
    ("var-decl-no-equals.http", E("malformed variable declaration"), "@var declaration with no `=`"),
    ("whitespace-only.http", O(0), "whitespace only"),
];

/// Error messages the user cannot act on. The user was bitten by exactly this
/// class of message ("builder error") and it must never come back.
const USELESS: &[&str] = &[
    "builder error",
    "error",
    "invalid",
    "failed",
    "unknown",
    "unexpected",
    "parse error",
    "internal error",
    "bad request",
    "none",
];

fn check_error_message(path: &Path, msg: &str, f: &mut Failures) {
    let t = msg.trim();
    if t.is_empty() {
        f.add(path, "error message is empty");
        return;
    }
    if t.len() < 12 {
        f.add(path, format!("error message is uselessly short: {t:?}"));
    }
    let lower = t.to_ascii_lowercase();
    let lower = lower.trim_end_matches(['.', '!']);
    if USELESS.contains(&lower) {
        f.add(path, format!("error message says nothing: {t:?}"));
    }
    if !t.chars().any(|c| c.is_ascii_lowercase()) {
        f.add(path, format!("error message is not a sentence: {t:?}"));
    }
}

#[test]
fn malformed_files_never_panic_and_errors_are_useful() {
    let mut f = Failures::new("malformed_files_never_panic_and_errors_are_useful");
    let mut covered = 0usize;
    for path in files("malformed") {
        let name = short(&path);
        let Some((_, expect, defect)) = MALFORMED.iter().find(|(n, ..)| *n == name) else {
            f.add(&path, "no MALFORMED row for this file");
            continue;
        };
        covered += 1;
        let src = read_src(&path);
        let got = std::panic::catch_unwind(|| parse(&src));
        let got = match got {
            Ok(r) => r,
            Err(_) => {
                f.add(&path, format!("PANICKED on: {defect}"));
                continue;
            }
        };
        match (expect, got) {
            (O(want), Ok(reqs)) => {
                if reqs.len() != *want {
                    f.add(
                        &path,
                        format!("{defect}: expected {want} requests, got {}", reqs.len()),
                    );
                }
                // Whatever came back must be structurally sane, not half-built.
                for (i, r) in reqs.iter().enumerate() {
                    if r.method.is_empty() {
                        f.add(&path, format!("request {i} has an empty method"));
                    }
                    if r.name.is_empty() && !r.url.is_empty() {
                        f.add(&path, format!("request {i} has an empty name"));
                    }
                    if r.seq != i as u32 {
                        f.add(&path, format!("request {i} has seq {}", r.seq));
                    }
                }
                // Serializing junk must not panic either.
                if std::panic::catch_unwind(|| serialize(&reqs)).is_err() {
                    f.add(&path, "serialize panicked on the parsed result");
                }
            }
            (O(want), Err(e)) => {
                f.add(
                    &path,
                    format!("{defect}: expected Ok({want}), got Err({e})"),
                );
            }
            (E(_), Ok(reqs)) => {
                f.add(
                    &path,
                    format!("{defect}: expected Err, got Ok({} requests)", reqs.len()),
                );
            }
            (E(needle), Err(e)) => {
                let msg = format!("{e:#}");
                check_error_message(&path, &msg, &mut f);
                if !msg.contains(needle) {
                    f.add(&path, format!("error {msg:?} does not mention {needle:?}"));
                }
            }
        }
    }
    assert!(
        covered >= 30,
        "expected at least 30 malformed files, checked {covered}"
    );
    f.assert();
}

// ---------------------------------------------------------------- STEP 4: mutations

/// A 64-bit LCG. Deterministic, seeded, zero dependencies — the point is
/// reproducibility, not statistical quality.
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 11
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
}

/// Base seed. Printed on every failure so any case can be replayed by pinning
/// this constant.
const SEED: u64 = 0x00DE_CAFB_ADD0_0D1E;

/// Corpus files above this size only get a handful of mutations each, so the
/// whole test stays well under the ~30s CI budget. The 10k-request file and the
/// 200KB-URL file are the only ones affected.
const BIG: usize = 32 * 1024;

fn mutate(bytes: &[u8], kind: u64, rng: &mut Lcg) -> Vec<u8> {
    let mut lines: Vec<&[u8]> = bytes.split(|b| *b == b'\n').collect();
    match kind % 7 {
        0 if !lines.is_empty() => {
            lines.remove(rng.below(lines.len()));
            lines.join(&b'\n')
        }
        1 if !lines.is_empty() => {
            let i = rng.below(lines.len());
            lines.insert(i, lines[i]);
            lines.join(&b'\n')
        }
        2 if lines.len() > 1 => {
            let (a, b) = (rng.below(lines.len()), rng.below(lines.len()));
            lines.swap(a, b);
            lines.join(&b'\n')
        }
        3 if !bytes.is_empty() => {
            let mut v = bytes.to_vec();
            let i = rng.below(v.len());
            v[i] ^= 1 << rng.below(8);
            v
        }
        4 => inject(bytes, b"###\n", rng),
        5 => inject(bytes, b"{{", rng),
        6 => inject(bytes, b"%}\n", rng),
        _ => bytes.to_vec(),
    }
}

fn inject(bytes: &[u8], what: &[u8], rng: &mut Lcg) -> Vec<u8> {
    let at = rng.below(bytes.len() + 1);
    let mut v = Vec::with_capacity(bytes.len() + what.len());
    v.extend_from_slice(&bytes[..at]);
    v.extend_from_slice(what);
    v.extend_from_slice(&bytes[at..]);
    v
}

#[test]
fn mutations_never_panic() {
    let mut f = Failures::new("mutations_never_panic");
    let mut cases = 0usize;
    for path in files("valid").into_iter().chain(files("malformed")) {
        let bytes = std::fs::read(&path).unwrap();
        let big = bytes.len() > BIG;
        // A per-file seed, derived from the base seed and the file name, so one
        // file's mutations do not shift when another file changes.
        let file_seed = short(&path)
            .bytes()
            .fold(SEED, |a, b| a.rotate_left(7) ^ b as u64);
        let mut rng = Lcg::new(file_seed);

        // Deterministic truncations at 1/16th boundaries.
        let steps = if big { 4 } else { 16 };
        for n in 0..steps {
            let cut = bytes.len() * n / steps;
            cases += 1;
            try_parse(
                &path,
                &bytes[..cut],
                "truncation",
                n as u64,
                file_seed,
                &mut f,
            );
        }
        // Sampled random mutations. Deterministic: same seed, same cases.
        let rounds = if big { 4 } else { 32 };
        for _ in 0..rounds {
            let kind = rng.next_u64();
            let m = mutate(&bytes, kind, &mut rng);
            cases += 1;
            try_parse(&path, &m, "mutation", kind % 7, file_seed, &mut f);
        }
    }
    assert!(
        cases >= 2000,
        "mutation coverage collapsed to {cases} cases"
    );
    f.assert();
}

fn try_parse(path: &Path, bytes: &[u8], what: &str, kind: u64, file_seed: u64, f: &mut Failures) {
    let src = String::from_utf8_lossy(bytes).into_owned();
    let res = std::panic::catch_unwind(|| parse(&src));
    match res {
        Err(_) => f.add(
            path,
            format!("PANIC on {what} kind {kind} (SEED={SEED:#x}, file_seed={file_seed:#x})"),
        ),
        // An Err is fine; so is an Ok. Anything we got back must also survive
        // serialization without panicking.
        Ok(Ok(reqs)) => {
            if std::panic::catch_unwind(|| serialize(&reqs)).is_err() {
                f.add(
                    path,
                    format!(
                        "PANIC serializing {what} kind {kind} (SEED={SEED:#x}, \
                         file_seed={file_seed:#x})"
                    ),
                );
            }
        }
        Ok(Err(_)) => {}
    }
}

// ---------------------------------------------------------------- STEP 5: chaining

#[test]
fn login_chain_leaves_nothing_unresolved() {
    let file = corpus_dir().join("valid/real-keycloak-login.http");
    let src = read_src(&file);
    let (reqs, decls) = parse_with_vars(&src).expect("parse");
    assert_eq!(decls, vec![("realm".to_string(), "master".to_string())]);

    let envs = vars::load_env_files(&file).expect("env files");
    let dev = envs
        .iter()
        .find(|e| e.name == "dev")
        .expect("dev environment");
    let base: Vars = dev
        .vars
        .iter()
        .map(|(k, v, _)| (k.clone(), v.clone()))
        .collect();
    let mut v = vars::merge(&base, &decls);

    // The login request resolves entirely from env + in-file declarations.
    let login = &reqs[0];
    assert!(interp::missing(&login.url, &v).is_empty(), "{}", login.url);
    assert_eq!(
        interp::interpolate(&login.url, &v),
        "http://localhost:8081/realms/master/protocol/openid-connect/token"
    );
    let Body::Form(pairs) = &login.body else {
        panic!("expected a form body: {:?}", login.body)
    };
    for (k, val) in pairs {
        let resolved = interp::interpolate(val, &v);
        assert!(
            !resolved.contains("{{"),
            "form field {k} still unresolved: {resolved}"
        );
    }
    assert!(
        pairs
            .iter()
            .any(|(k, val)| k == "password" && interp::interpolate(val, &v) == "hunter2"),
        "the private env file must supply the password: {pairs:?}"
    );

    // A realistic Keycloak token response feeds the next request.
    let body = br#"{
        "access_token": "eyJhbGciOiJSUzI1NiJ9.access",
        "expires_in": 300,
        "refresh_expires_in": 1800,
        "refresh_token": "eyJhbGciOiJIUzI1NiJ9.refresh",
        "token_type": "Bearer",
        "not-before-policy": 0,
        "session_state": "8a1c...",
        "scope": "profile email"
    }"#;
    let captured = vars::capture(
        login
            .post_response_script
            .as_deref()
            .expect("login has a response handler"),
        body,
        200,
        &[("Content-Type".into(), "application/json".into())],
    );
    assert_eq!(
        captured,
        vec![
            (
                "accessToken".to_string(),
                "eyJhbGciOiJSUzI1NiJ9.access".to_string()
            ),
            (
                "refreshToken".to_string(),
                "eyJhbGciOiJIUzI1NiJ9.refresh".to_string()
            ),
        ]
    );
    v = vars::merge(&v, &captured);

    // ... and the authenticated request now has no unresolved placeholder left
    // anywhere: URL, header names or header values.
    let whoami = &reqs[1];
    assert!(
        interp::missing(&whoami.url, &v).is_empty(),
        "{}",
        whoami.url
    );
    assert!(!interp::interpolate(&whoami.url, &v).contains("{{"));
    for (k, val) in &whoami.headers {
        assert!(interp::missing(k, &v).is_empty(), "header name {k}");
        assert!(interp::missing(val, &v).is_empty(), "header {k}: {val}");
    }
    assert_eq!(
        interp::interpolate(&whoami.headers[0].1, &v),
        "Bearer eyJhbGciOiJSUzI1NiJ9.access"
    );
}

#[test]
fn capture_pulls_a_token_out_of_a_real_login_response() {
    // The GitHub corpus file captures out of a top-level JSON array.
    let src = read_src(&corpus_dir().join("valid/real-github.http"));
    let reqs = parse(&src).unwrap();
    let got = vars::capture(
        reqs[0].post_response_script.as_deref().unwrap(),
        br#"[{"full_name":"octocat/hello-world","private":false},{"full_name":"octocat/spoon"}]"#,
        200,
        &[],
    );
    assert_eq!(
        got,
        [("firstRepo".to_string(), "octocat/hello-world".to_string())]
    );

    // The OAuth corpus file captures out of a nested object, and a handler that
    // also runs assertions must not confuse the scanner.
    let src = read_src(&corpus_dir().join("valid/handlers-inline.http"));
    let reqs = parse(&src).unwrap();
    let got = vars::capture(
        reqs[0].post_response_script.as_deref().unwrap(),
        br#"{"access_token":"AT-9","token_type":"Bearer","expires_in":3600}"#,
        200,
        &[],
    );
    assert_eq!(got, [("authToken".to_string(), "AT-9".to_string())]);

    // A non-JSON error page must not produce garbage captures.
    let got = vars::capture(
        reqs[0].post_response_script.as_deref().unwrap(),
        b"<html><body>502 Bad Gateway</body></html>",
        502,
        &[],
    );
    assert!(got.is_empty(), "{got:?}");
}

/// `exec::check_resolved` is private, but `exec::send` runs it before touching
/// the network, so an unresolved request fails offline — which is what this
/// asserts, including that the message names every missing variable.
#[test]
fn undefined_variables_are_named_before_the_wire() {
    let src = "\
### needs-vars
POST {{baseUrl}}/api/{{missingPath}}?key={{missingQuery}}
Authorization: Bearer {{missingToken}}
Content-Type: application/json

{\"who\": \"{{missingBody}}\"}
";
    let reqs = parse(src).unwrap();
    let known: Vars = [("baseUrl".to_string(), "http://127.0.0.1:1".to_string())].into();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let err = rt
        .block_on(rp_core::exec::send(&reqs[0], &known))
        .expect_err("an unresolved request must never reach the wire");
    let msg = format!("{err:#}");
    for name in ["missingPath", "missingQuery", "missingToken", "missingBody"] {
        assert!(msg.contains(name), "error must name {name}: {msg}");
    }
    assert!(
        !msg.contains("baseUrl"),
        "resolved variables must not be reported: {msg}"
    );
    assert!(msg.len() > 30, "error message is not actionable: {msg}");

    // And the same names, from the primitive the executor uses.
    assert_eq!(interp::missing(&reqs[0].url, &known), ["missingPath"]);
    assert_eq!(
        interp::missing(&reqs[0].query[0].1, &known),
        ["missingQuery"]
    );
}

// ---------------------------------------------------------------- known bugs
//
// Each of these was a real defect found by this suite. All are fixed; the tests
// stay as regressions.

#[test]
fn bug_custom_method_is_swallowed_by_the_url() {
    let r = &parse("PURGE https://cdn.example.com/asset.css\n").unwrap()[0];
    assert_eq!(r.method, "PURGE");
    assert_eq!(r.url, "https://cdn.example.com/asset.css");
}

#[test]
fn bug_variable_as_header_name_is_a_parse_error() {
    let reqs = parse("GET https://example.com/x\n{{headerName}}: value\n")
        .expect("a variable is legal in a header name");
    assert_eq!(
        reqs[0].headers,
        vec![("{{headerName}}".to_string(), "value".to_string())]
    );
}

#[test]
fn bug_at_name_with_equals_keeps_the_equals_sign() {
    let r = &parse("# @name = loginUser\nGET https://example.com/x\n").unwrap()[0];
    assert_eq!(r.name, "loginUser");
}

#[test]
fn bug_bom_prefix_mangles_the_request_line() {
    let r = &parse("\u{feff}GET https://example.com/bom\n").unwrap()[0];
    assert_eq!(r.method, "GET");
    assert_eq!(r.url, "https://example.com/bom");
}

#[test]
fn bug_response_redirect_is_treated_as_a_handler_file() {
    let r = &parse("GET https://example.com/x\n\n>> ./out/response.json\n").unwrap()[0];
    // The handler-file key must not be reused for a response *redirect*.
    assert_ne!(
        r.extra.get("post_response_script_file").map(String::as_str),
        Some("> ./out/response.json")
    );
    assert!(
        r.extra
            .keys()
            .any(|k| k.contains("redirect") || k.contains("output")),
        "expected a distinct key for the response redirect, got {:?}",
        r.extra
    );
}

#[test]
fn bug_pre_request_script_file_does_not_round_trip() {
    let mut r = rp_core::model::Request {
        name: "with-pre-script".into(),
        method: "GET".into(),
        url: "https://example.com/x".into(),
        ..Default::default()
    };
    r.extra
        .insert("pre_request_script_file".into(), "./pre.js".into());
    let text = serialize(std::slice::from_ref(&r));
    let back = parse(&text)
        .unwrap_or_else(|e| panic!("serialize produced unparseable text:\n{text}\n{e}"));
    assert_eq!(back[0].url, "https://example.com/x");
}

#[test]
fn bug_in_file_var_declarations_are_lost_on_serialize() {
    let src = "@host = https://api.example.com\n\n### one\nGET {{host}}/x\n";
    let (reqs, decls) = parse_with_vars(src).unwrap();
    let (_, again) = parse_with_vars(&serialize(&reqs)).unwrap();
    assert_eq!(again, decls, "@var declarations must survive a save");
}

#[test]
fn bug_empty_query_value_loses_its_equals_sign() {
    let reqs = parse("GET https://example.com/x?a=&b=1\n").unwrap();
    assert_eq!(
        reqs[0].query,
        vec![("a".into(), "".into()), ("b".into(), "1".into())]
    );
    assert!(serialize(&reqs).contains("?a=&b=1"), "{}", serialize(&reqs));
}

#[test]
fn bug_multipart_boundary_mismatch_silently_drops_the_body() {
    let src = read_src(&corpus_dir().join("malformed/multipart-boundary-mismatch.http"));
    // The body can no longer be silently thrown away: a mismatch is now an error
    // that names both boundaries so the user can see which one to change.
    let err = parse(&src).expect_err("a boundary mismatch must not be swallowed");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("AAA"),
        "must name the declared boundary: {msg}"
    );
    assert!(msg.contains("BBB"), "must name the body's boundary: {msg}");
    assert!(msg.contains("line 2"), "must say where: {msg}");
}

#[test]
fn bug_graphql_blank_line_inside_the_query_splits_it() {
    let src = "\
POST https://example.com/graphql
Content-Type: application/graphql

query Two {
  a
}

query AlsoPartOfTheQuery {
  b
}
";
    let r = &parse(src).unwrap()[0];
    let Body::GraphQl { variables, .. } = &r.body else {
        panic!("{:?}", r.body)
    };
    assert!(
        variables.is_empty() || variables.trim_start().starts_with('{'),
        "variables must be a JSON object, got {variables:?}"
    );
}

#[test]
fn bug_dotted_header_name_is_rejected() {
    let reqs = parse("GET https://example.com/x\nX.Legacy.Trace: 1\n")
        .expect("`.` is a legal header-name character");
    assert_eq!(
        reqs[0].headers,
        vec![("X.Legacy.Trace".to_string(), "1".to_string())]
    );
}

#[test]
fn bug_dynamic_variables_are_reported_as_missing() {
    let src = "POST https://example.com/x\nX-Request-Id: {{$uuid}}\n\n{\"t\":\"{{$timestamp}}\"}\n";
    let r = &parse(src).unwrap()[0];
    let empty = Vars::new();
    assert!(
        interp::missing(&r.headers[0].1, &empty).is_empty(),
        "{{$uuid}} must be generated, not reported as an undefined variable"
    );
}

#[test]
fn bug_var_decl_without_equals_reports_the_wrong_line() {
    let err = parse("@baseUrl http://localhost:8080\nGET https://example.com/x\n")
        .expect_err("today this fails");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("@baseUrl"),
        "the error should point at the bad declaration: {msg}"
    );
}
