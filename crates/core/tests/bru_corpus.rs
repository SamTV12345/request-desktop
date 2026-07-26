//! SQLite-grade test suite for the `.bru` parser.
//!
//! Three layers:
//!
//! 1. **Corpus invariants** — every real-world `.bru` file in
//!    `tests/corpus/bru/` (275 files vendored from upstream Bruno, see
//!    `corpus/bru/PROVENANCE.md`) must parse, round-trip and serialize stably.
//! 2. **Malformed input hardening** — `tests/corpus/bru/malformed/` holds
//!    hand-written broken files. The contract is *never panic, never hang,
//!    always either `Ok` or `Err`*.
//! 3. **Systematic mutation testing** — a seeded LCG mutates every corpus file
//!    and asserts the same contract.
//!
//! Every corpus test collects *all* failures with file paths and reports them
//! at once. Dying on the first failure hides how broad a regression is.
//!
//! No dependencies beyond `std` and the crate under test: the PRNG, the
//! directory walker and the timeout harness are all hand-rolled.

use rp_core::bru::{load_collection, parse, parse_environment, serialize};
use rp_core::model::{Auth, Body, Item};
use std::cell::Cell;
use std::fmt::Write as _;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

// ---------------------------------------------------------------------------
// corpus discovery
// ---------------------------------------------------------------------------

/// Minimum number of real-world corpus files. A corpus test that silently
/// iterates zero files is worse than no test at all, so this floor is what
/// stops a bad glob or a half-finished copy from making the whole suite
/// vacuously green.
const CORPUS_FLOOR: usize = 250;
/// Minimum number of hand-written malformed files.
const MALFORMED_FLOOR: usize = 25;

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/bru")
}

/// Recursive `.bru` walker. `std::fs` only — no `walkdir`.
fn walk_bru(dir: &Path, skip_malformed: bool, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .collect();
    // Sort so iteration order — and therefore every failure report and every
    // mutation — is identical on every machine and every run.
    entries.sort();
    for path in entries {
        if path.is_dir() {
            if skip_malformed && path.file_name().is_some_and(|n| n == "malformed") {
                continue;
            }
            walk_bru(&path, skip_malformed, out);
        } else if path.extension().is_some_and(|e| e == "bru") {
            out.push(path);
        }
    }
}

/// The vendored upstream corpus, excluding the hand-written `malformed/` set.
fn corpus_files() -> Vec<PathBuf> {
    let mut v = Vec::new();
    walk_bru(&corpus_root(), true, &mut v);
    assert!(
        v.len() >= CORPUS_FLOOR,
        "only {} corpus files under {} — expected at least {CORPUS_FLOOR}. \
         The vendored corpus is missing or the walker is broken; refusing to \
         run a vacuous test.",
        v.len(),
        corpus_root().display()
    );
    v
}

fn malformed_files() -> Vec<PathBuf> {
    let mut v = Vec::new();
    walk_bru(&corpus_root().join("malformed"), false, &mut v);
    assert!(
        v.len() >= MALFORMED_FLOOR,
        "only {} malformed files — expected at least {MALFORMED_FLOOR}",
        v.len()
    );
    v
}

fn rel(path: &Path) -> String {
    path.strip_prefix(corpus_root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Reads a corpus file as text. Lossy on purpose: `malformed/invalid_utf8.bru`
/// exists precisely so the parser gets fed replacement characters, and a
/// mutation can turn any file into invalid UTF-8.
fn read_text(path: &Path) -> String {
    String::from_utf8_lossy(
        &fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
    )
    .into_owned()
}

/// Reports every collected failure at once, capped so a total regression does
/// not produce a megabyte of output.
fn report(label: &str, failures: Vec<String>) {
    if failures.is_empty() {
        return;
    }
    const CAP: usize = 25;
    let mut msg = format!("{label}: {} failure(s)\n", failures.len());
    for f in failures.iter().take(CAP) {
        let _ = writeln!(msg, "  - {f}");
    }
    if failures.len() > CAP {
        let _ = writeln!(msg, "  ... and {} more", failures.len() - CAP);
    }
    panic!("{msg}");
}

// ---------------------------------------------------------------------------
// panic capture
// ---------------------------------------------------------------------------

thread_local! {
    /// Set while a deliberate `catch_unwind` is in flight, so the fuzz loops do
    /// not spray thousands of panic backtraces onto stderr.
    static QUIET: Cell<bool> = const { Cell::new(false) };
}

/// Installs, once per test binary, a panic hook that respects `QUIET`.
/// Swapping the global hook per call would race with the other tests the
/// harness runs in parallel; a thread-local flag does not.
fn install_hook() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let prev = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if !QUIET.with(Cell::get) {
                prev(info);
            }
        }));
    });
}

/// Runs `f`, converting a panic into `Err(message)`.
fn catch<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    install_hook();
    QUIET.with(|q| q.set(true));
    let r = panic::catch_unwind(AssertUnwindSafe(f));
    QUIET.with(|q| q.set(false));
    r.map_err(|e| {
        e.downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| e.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string())
    })
}

/// Runs `f` on a worker thread and fails the test if it does not finish in
/// `secs`. This is the only way to turn a hang into a test failure: there is no
/// way to cancel a runaway thread in safe Rust, so on timeout the worker is
/// leaked and the process is left to the harness to tear down. That is
/// acceptable for a test binary and is the whole point — a hang must not wedge
/// CI forever.
fn with_timeout(secs: u64, label: &'static str, f: impl FnOnce() + Send + 'static) {
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        f();
        let _ = tx.send(());
    });
    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(()) => worker.join().expect("worker thread panicked"),
        // Sender dropped without sending => the worker panicked. Re-raise its
        // payload so the real assertion message survives.
        Err(mpsc::RecvTimeoutError::Disconnected) => match worker.join() {
            Ok(()) => {}
            Err(p) => panic::resume_unwind(p),
        },
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("{label}: did not terminate within {secs}s — the parser is hanging")
        }
    }
}

// ---------------------------------------------------------------------------
// hand-rolled PRNG
// ---------------------------------------------------------------------------

/// The one seed for the whole mutation suite. Printed in every failure message
/// so a failing case is reproducible; change it only to go hunting for new bugs.
const SEED: u64 = 0x5EED_1234_ABCD_5678;

/// 64-bit LCG (constants from Knuth/MMIX via Numerical Recipes). The low bits
/// of an LCG are famously non-random, so only the high bits are handed out.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
    /// Uniform-enough index in `0..n`. Returns 0 for `n == 0`.
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            self.next_u32() as usize % n
        }
    }
}

/// FNV-1a over the relative path, so each file gets a stable per-file seed that
/// does not depend on iteration order, `DefaultHasher`'s unspecified algorithm,
/// or the platform.
fn path_seed(path: &Path) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in rel(path).as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    SEED ^ h
}

// ---------------------------------------------------------------------------
// mutations
// ---------------------------------------------------------------------------

/// Number of randomised mutations of each kind per file. The full cross-product
/// of every kind at every position would be millions of parses; instead each
/// kind is sampled `ROUNDS` times at seeded-random positions. With ~275 files
/// that is ~275 * (16 + 7 * 4) = ~12k parses, comfortably inside the runtime
/// budget while still churning through a different set of positions the moment
/// `SEED` changes.
const ROUNDS: usize = 4;

/// All mutations of one file, as `(label, bytes)`. Bytes rather than `String`
/// because byte flips and truncations must be allowed to produce invalid UTF-8.
fn mutations(src: &[u8], seed: u64) -> Vec<(String, Vec<u8>)> {
    let mut rng = Lcg::new(seed);
    let mut out = Vec::new();

    // Truncate at every 1/16th boundary. Deterministic, no PRNG needed. Cuts
    // land mid-token, mid-block and mid-multiline on purpose.
    for k in 1..=16 {
        let cut = src.len() * k / 16;
        out.push((format!("truncate({k}/16)"), src[..cut].to_vec()));
    }

    let lines: Vec<&[u8]> = src.split(|b| *b == b'\n').collect();
    let join = |ls: &[Vec<u8>]| ls.join(&b'\n');

    for _ in 0..ROUNDS {
        // delete a random line
        if !lines.is_empty() {
            let i = rng.below(lines.len());
            let mut ls: Vec<Vec<u8>> = lines.iter().map(|l| l.to_vec()).collect();
            ls.remove(i);
            out.push((format!("delete_line({i})"), join(&ls)));

            // duplicate a random line
            let i = rng.below(lines.len());
            let mut ls: Vec<Vec<u8>> = lines.iter().map(|l| l.to_vec()).collect();
            ls.insert(i, ls[i].clone());
            out.push((format!("duplicate_line({i})"), join(&ls)));

            // swap two lines
            let (a, b) = (rng.below(lines.len()), rng.below(lines.len()));
            let mut ls: Vec<Vec<u8>> = lines.iter().map(|l| l.to_vec()).collect();
            ls.swap(a, b);
            out.push((format!("swap_lines({a},{b})"), join(&ls)));

            // insert a stray delimiter on its own line
            for stray in [&b"}"[..], &b"{"[..], &b"'''"[..]] {
                let i = rng.below(lines.len() + 1);
                let mut ls: Vec<Vec<u8>> = lines.iter().map(|l| l.to_vec()).collect();
                ls.insert(i, stray.to_vec());
                out.push((
                    format!("insert({},{i})", String::from_utf8_lossy(stray)),
                    join(&ls),
                ));
            }
        }

        // flip a random bit of a random byte
        if !src.is_empty() {
            let i = rng.below(src.len());
            let bit = rng.below(8);
            let mut b = src.to_vec();
            b[i] ^= 1 << bit;
            out.push((format!("flip_byte({i},bit {bit})"), b));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// STEP 2 — corpus invariants
// ---------------------------------------------------------------------------

#[test]
fn corpus_has_a_meaningful_number_of_files() {
    let corpus = corpus_files(); // asserts the floor internally
    let malformed = malformed_files();
    assert!(
        corpus_root().join("PROVENANCE.md").is_file(),
        "the vendored corpus must carry its PROVENANCE.md (source, commit, license)"
    );
    // Loud on purpose: `cargo test -- --nocapture` should show what was covered.
    println!(
        "corpus: {} upstream files, {} malformed files",
        corpus.len(),
        malformed.len()
    );
}

#[test]
fn every_corpus_file_parses() {
    let mut failures = Vec::new();
    for path in corpus_files() {
        let src = read_text(&path);
        // Environment files are a different grammar; they get their own test.
        let r = if is_environment(&path) {
            parse_environment(&src).map(|_| ())
        } else {
            parse(&src).map(|_| ())
        };
        if let Err(e) = r {
            failures.push(format!("{}: {e}", rel(&path)));
        }
    }
    report("files that failed to parse", failures);
}

#[test]
fn round_trip_is_idempotent() {
    let mut failures = Vec::new();
    for path in corpus_files() {
        if is_environment(&path) {
            continue; // no `serialize` for environments
        }
        let src = read_text(&path);
        let Ok(a) = parse(&src) else { continue }; // covered by every_corpus_file_parses
        let text = serialize(&a);
        match parse(&text) {
            Ok(b) if a == b => {}
            Ok(b) => failures.push(format!(
                "{}: parse(serialize(parse(x))) != parse(x)\n      first : {a:?}\n      second: {b:?}",
                rel(&path)
            )),
            Err(e) => failures.push(format!(
                "{}: serialized output does not parse: {e}\n{text}",
                rel(&path)
            )),
        }
    }
    report("files that did not round-trip", failures);
}

#[test]
fn serialize_is_stable() {
    let mut failures = Vec::new();
    for path in corpus_files() {
        if is_environment(&path) {
            continue;
        }
        let src = read_text(&path);
        let Ok(a) = parse(&src) else { continue };
        let once = serialize(&a);
        let Ok(b) = parse(&once) else { continue }; // reported by round_trip_is_idempotent
        let twice = serialize(&b);
        if once != twice {
            failures.push(format!(
                "{}: serialize is not a fixed point\n--- once ---\n{once}\n--- twice ---\n{twice}",
                rel(&path)
            ));
        }
    }
    report("files whose serialization is not byte-stable", failures);
}

#[test]
fn parse_never_panics_on_the_corpus() {
    let mut failures = Vec::new();
    for path in corpus_files() {
        let src = read_text(&path);
        // Both entry points, on every file, regardless of which grammar the
        // file actually is — feeding a request to `parse_environment` and vice
        // versa must still be Ok-or-Err.
        if let Err(p) = catch(|| parse(&src)) {
            failures.push(format!("{}: parse panicked: {p}", rel(&path)));
        }
        if let Err(p) = catch(|| parse_environment(&src)) {
            failures.push(format!("{}: parse_environment panicked: {p}", rel(&path)));
        }
        // And serialize must not panic on anything parse produced.
        if let Ok(Ok(req)) = catch(|| parse(&src))
            && let Err(p) = catch(|| serialize(&req))
        {
            failures.push(format!("{}: serialize panicked: {p}", rel(&path)));
        }
    }
    report("panics on the corpus", failures);
}

fn is_environment(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .is_some_and(|n| n == "environments")
}

// ---------------------------------------------------------------------------
// load_collection over the vendored tree
// ---------------------------------------------------------------------------

/// Every upstream collection root vendored under `corpus/bru/`, with the
/// collection name `load_collection` must read out of its `bruno.json`.
const COLLECTION_ROOTS: &[(&str, &str)] = &[
    ("collection", "bruno-testbench"),
    ("collection_level_oauth2", "collection_level_oauth2"),
    ("collection_oauth2", "OAuth2 Demo"),
    ("keycloak-authorization_code", "keycloak-authorization_code"),
    ("keycloak-client-credentials", "keycloak-client-credentials"),
    (
        "keycloak-password-credentials",
        "keycloak-password-credentials",
    ),
    ("sandwich_exec", "sandwich_exec"),
    ("sequential_exec", "sequential_exec"),
];

/// Requests `load_collection` is expected to find, per root. These are exact:
/// `load_collection` silently skips files it cannot parse, so an exact count is
/// the only thing that catches a file quietly dropping out of the tree.
const EXPECTED_REQUESTS: &[(&str, usize)] = &[
    ("collection", 223),
    ("collection_level_oauth2", 1),
    ("collection_oauth2", 8),
    ("keycloak-authorization_code", 3),
    ("keycloak-client-credentials", 3),
    ("keycloak-password-credentials", 3),
    ("sandwich_exec", 1),
    ("sequential_exec", 1),
];

fn count_requests(items: &[Item]) -> usize {
    items
        .iter()
        .map(|i| match i {
            Item::Request { .. } => 1,
            Item::Folder { items, .. } => count_requests(items),
        })
        .sum()
}

/// `.bru` files under `dir` that `load_collection` is supposed to turn into
/// requests: everything except `folder.bru`, `collection.bru` and the top-level
/// `environments/` directory.
fn expected_request_files(dir: &Path, is_root: bool) -> usize {
    let mut n = 0;
    for entry in fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("entry").path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if path.is_dir() {
            if is_root && name == "environments" {
                continue;
            }
            n += expected_request_files(&path, false);
        } else if path.extension().is_some_and(|e| e == "bru")
            && name != "folder.bru"
            && name != "collection.bru"
        {
            n += 1;
        }
    }
    n
}

#[test]
fn load_collection_over_the_vendored_tree() {
    let mut failures = Vec::new();
    for (dir, expected_name) in COLLECTION_ROOTS {
        let root = corpus_root().join(dir);
        let c = match load_collection(&root) {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!("{dir}: load_collection failed: {e}"));
                continue;
            }
        };
        if c.name != *expected_name {
            failures.push(format!(
                "{dir}: name is {:?}, expected {expected_name:?}",
                c.name
            ));
        }

        let found = count_requests(&c.items);
        let expected = EXPECTED_REQUESTS
            .iter()
            .find(|(d, _)| d == dir)
            .map(|(_, n)| *n)
            .unwrap_or(0);
        if found != expected {
            failures.push(format!("{dir}: {found} requests, expected {expected}"));
        }
        // Cross-check the hardcoded number against the tree on disk, so a
        // corpus refresh cannot be papered over by editing one constant. Any
        // gap means `load_collection` dropped a file it should have parsed.
        let on_disk = expected_request_files(&root, true);
        if found != on_disk {
            failures.push(format!(
                "{dir}: load_collection produced {found} requests but {on_disk} \
                 request-shaped .bru files exist on disk — files are being \
                 silently skipped"
            ));
        }
    }
    report("load_collection over the corpus", failures);
}

#[test]
fn load_collection_preserves_seq_ordering_and_folder_first_layout() {
    let mut failures = Vec::new();
    for (dir, _) in COLLECTION_ROOTS {
        let root = corpus_root().join(dir);
        let c = load_collection(&root).expect("load_collection");
        check_ordering(&c.items, dir, &mut failures);
    }
    report("ordering violations", failures);
}

fn check_ordering(items: &[Item], path: &str, failures: &mut Vec<String>) {
    // Folders come first, then requests. No folder may follow a request.
    let first_request = items.iter().position(|i| matches!(i, Item::Request { .. }));
    if let Some(fr) = first_request
        && items[fr..].iter().any(|i| matches!(i, Item::Folder { .. }))
    {
        failures.push(format!("{path}: a folder appears after a request"));
    }

    // Within one folder, requests are ordered by (seq, name).
    let keys: Vec<(u32, String)> = items
        .iter()
        .filter_map(|i| match i {
            Item::Request { request, .. } => Some((request.seq, request.name.clone())),
            Item::Folder { .. } => None,
        })
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    if keys != sorted {
        failures.push(format!(
            "{path}: requests are not ordered by (seq, name): {keys:?}"
        ));
    }

    for i in items {
        if let Item::Folder { name, items } = i {
            check_ordering(items, &format!("{path}/{name}"), failures);
        }
    }
}

// ---------------------------------------------------------------------------
// STEP 3 — malformed input hardening
// ---------------------------------------------------------------------------

/// What each hand-written malformed file is wrong about.
///
/// The description lives here rather than as a comment inside the file on
/// purpose: `.bru` has no comment syntax, so a leading `# ...` line would be
/// the *first* thing the parser rejects and would mask the actual defect the
/// fixture exists to exercise. The filename plus this table is the comment.
const MALFORMED: &[(&str, &str)] = &[
    (
        "annotation_unclosed.bru",
        "an @description(''' annotation that is never closed",
    ),
    (
        "bom_prefixed.bru",
        "a UTF-8 BOM before the first block header",
    ),
    (
        "bracket_closed_with_brace.bru",
        "a `[` block closed with `}`",
    ),
    (
        "brace_closed_with_bracket.bru",
        "a `{` block closed with `]`",
    ),
    (
        "close_brace_indented.bru",
        "the closing `}` is indented instead of at column 0",
    ),
    ("close_brace_only.bru", "a lone closing brace with no block"),
    (
        "control_chars.bru",
        "C0 control characters scattered through keys and values",
    ),
    (
        "deep_brackets.bru",
        "2000 nested `[` plus an unterminated list value",
    ),
    (
        "duplicate_blocks.bru",
        "duplicate meta, verb and headers blocks",
    ),
    ("empty.bru", "a completely empty file"),
    (
        "empty_body_and_docs.bru",
        "an empty body:json block and an empty docs block",
    ),
    ("extra_close_brace.bru", "one closing brace too many"),
    (
        "header_without_name.bru",
        "a block header with no name, just `{`",
    ),
    ("huge_key.bru", "a 64 KB key"),
    ("huge_single_line.bru", "a single ~1 MB line"),
    ("invalid_utf8.bru", "raw invalid UTF-8 byte sequences"),
    ("lone_cr.bru", "a bare \\r as the only line terminator"),
    ("many_blocks.bru", "10k blocks"),
    (
        "mixed_line_endings.bru",
        "CRLF, LF and bare CR mixed in one file",
    ),
    (
        "multiline_delimiter_soup.bru",
        "''' delimiters doubled and in the wrong places",
    ),
    ("multiline_never_closed.bru", "''' opened and never closed"),
    (
        "nested_block_headers.bru",
        "a block header nested inside another block",
    ),
    (
        "no_verb_block_with_body.bru",
        "a body block with no verb block at all",
    ),
    (
        "nul_bytes.bru",
        "NUL bytes inside keys, values and structure",
    ),
    (
        "body_mode_without_block.bru",
        "a body: mode declaring a block that does not exist",
    ),
    (
        "quoted_key_unterminated.bru",
        "a quoted key whose closing quote is missing",
    ),
    (
        "seq_not_a_number.bru",
        "meta seq that is not a number, and an empty type",
    ),
    (
        "tab_indentation.bru",
        "tab-indented body and tab-indented closing brace",
    ),
    ("tilde_on_nothing.bru", "a `~` disable marker on nothing"),
    (
        "toplevel_colon_lines.bru",
        "bare `key: value` lines at the top level",
    ),
    (
        "unicode_keys.bru",
        "non-ASCII and emoji in block values and keys",
    ),
    (
        "unknown_block_names.bru",
        "syntactically valid but unmodelled block names",
    ),
    ("unterminated_block.bru", "a block that is never closed"),
    ("whitespace_only.bru", "nothing but whitespace"),
];

#[test]
fn every_malformed_file_is_documented() {
    let on_disk: Vec<String> = malformed_files().iter().map(|p| rel(p)).collect();
    let mut failures = Vec::new();
    for name in &on_disk {
        let base = name.rsplit('/').next().unwrap();
        if !MALFORMED.iter().any(|(n, _)| *n == base) {
            failures.push(format!("{name} has no entry in the MALFORMED table"));
        }
    }
    for (name, _) in MALFORMED {
        if !on_disk.iter().any(|p| p.ends_with(name)) {
            failures.push(format!("MALFORMED lists {name} but the file is missing"));
        }
    }
    report("malformed corpus bookkeeping", failures);
}

/// The core contract: for every deliberately broken file, `parse` and
/// `parse_environment` return `Ok` or `Err`. Never a panic, never a hang.
/// Where the result is `Ok`, something sensible is asserted about it.
#[test]
fn malformed_files_never_panic() {
    with_timeout(60, "malformed_files_never_panic", || {
        let mut failures = Vec::new();
        for path in malformed_files() {
            let name = rel(&path);
            let src = read_text(&path);
            match catch(|| parse(&src)) {
                Err(p) => failures.push(format!("{name}: parse panicked: {p}")),
                // An `Ok` must survive a serialize round without panicking.
                Ok(Ok(req)) => {
                    if let Err(p) = catch(|| serialize(&req)) {
                        failures.push(format!("{name}: parsed, then serialize panicked: {p}"));
                    }
                }
                Ok(Err(_)) => {}
            }
            if let Err(p) = catch(|| parse_environment(&src)) {
                failures.push(format!("{name}: parse_environment panicked: {p}"));
            }
        }
        report("panics on malformed input", failures);
    });
}

#[test]
fn malformed_files_that_must_be_rejected() {
    // Structural damage the scanner is documented to catch.
    for name in [
        "unterminated_block.bru",
        "close_brace_indented.bru",
        "header_without_name.bru",
        "close_brace_only.bru",
        "extra_close_brace.bru",
        "bracket_closed_with_brace.bru",
        "brace_closed_with_bracket.bru",
        "tab_indentation.bru",
        // A `:` line with an empty name is not a single-line block header.
        "toplevel_colon_lines.bru",
    ] {
        let src = read_text(&corpus_root().join("malformed").join(name));
        assert!(parse(&src).is_err(), "{name} should be rejected but parsed");
    }
}

#[test]
fn malformed_files_that_are_accepted_produce_something_sensible() {
    let m = |name: &str| read_text(&corpus_root().join("malformed").join(name));

    // Empty and whitespace-only input is a valid (empty) file, not an error.
    let empty = parse(&m("empty.bru")).expect("empty file");
    assert_eq!(empty.name, "");
    assert_eq!(empty.method, "");
    assert_eq!(empty.seq, 1);
    assert!(parse(&m("whitespace_only.bru")).is_ok());
    assert!(parse_environment(&m("empty.bru")).unwrap().vars.is_empty());

    // Unknown block names are kept verbatim rather than dropped.
    let unknown = parse(&m("unknown_block_names.bru")).expect("unknown blocks");
    assert_eq!(unknown.extra["block.zzz-unknown"], "a: 1");
    assert_eq!(unknown.extra["block.deeply:nested:unknown"], "b: 2");

    // Duplicate blocks: last meta wins, last verb wins, headers accumulate.
    let dup = parse(&m("duplicate_blocks.bru")).expect("duplicate blocks");
    assert_eq!(dup.name, "second");
    assert_eq!(dup.seq, 2);
    assert_eq!(dup.method, "POST");
    assert_eq!(dup.url, "http://b");
    assert_eq!(
        dup.headers.len(),
        2,
        "both headers blocks are kept: {:?}",
        dup.headers
    );

    // A non-numeric seq falls back to 1 instead of failing the parse.
    let seq = parse(&m("seq_not_a_number.bru")).expect("bad seq");
    assert_eq!(seq.seq, 1);

    // A declared body mode with no matching block leaves the body empty but
    // keeps the declaration.
    let mode = parse(&m("body_mode_without_block.bru")).expect("body mode");
    assert_eq!(mode.body, Body::None);
    assert_eq!(mode.extra["http.body"], "multipartForm");
    // ...and an auth kind with no block is Auth::None, not a panic.
    assert_eq!(mode.auth, Auth::None);
    assert_eq!(mode.extra["http.auth"], "awsv4");

    // `~` on nothing: the marker-only lines have no `:` so they are skipped.
    let tilde = parse(&m("tilde_on_nothing.bru")).expect("tilde");
    assert_eq!(
        tilde.headers,
        vec![
            ("~".to_string(), "1".to_string()),
            ("~~~".to_string(), "2".to_string())
        ],
        "a bare `~` line is dropped; `~: 1` keeps the marker as the whole key"
    );

    // Unicode survives keys, values and URLs.
    let uni = parse(&m("unicode_keys.bru")).expect("unicode");
    assert_eq!(uni.name, "юникод");
    assert_eq!(uni.url, "http://x/☃");
    assert!(uni.headers.iter().any(|(k, _)| k == "ключ"));
    assert!(uni.headers.iter().any(|(k, _)| k == "🔑"));
    assert!(uni.headers.iter().any(|(k, _)| k == "sp ace🙂"));

    // An unterminated `'''` is *tolerated*, not rejected: `read_multiline` runs
    // to the end of the enclosing block and keeps everything it saw. The block
    // itself is still closed by its `}`, so the file parses.
    let ml = parse(&m("multiline_never_closed.bru")).expect("unterminated '''");
    assert_eq!(
        ml.headers,
        vec![("m".to_string(), "line one\nline two".to_string())]
    );

    // NUL bytes are just bytes inside a value, not terminators...
    let nul = parse("get {\n  url: http://x\0y\n}\n").expect("NUL in a value");
    assert!(
        nul.url.contains('\0'),
        "NUL is carried through, not truncated at"
    );
    // ...and a NUL alone on a line between blocks is whitespace, exactly as in
    // Bruno (ohm's built-in `space` is `\x00..\x20`), so the fixture with a
    // trailing NUL after the final `}` is accepted. Updated when the
    // `malformed/nul_bytes.bru [parse]` divergence was fixed.
    let nul_file = parse(&m("nul_bytes.bru")).expect("trailing NUL line");
    assert_eq!(nul_file.url, "http://x\0y");
    assert_eq!(
        nul_file.headers,
        vec![("a\0b".to_string(), "1".to_string())]
    );
    assert!(parse(&m("control_chars.bru")).is_ok());

    // Invalid UTF-8 becomes replacement characters and still parses.
    let bad = parse(&m("invalid_utf8.bru")).expect("invalid utf8");
    assert!(bad.url.starts_with("http://x/"));
    // The real-world path is `fs::read_to_string`, which rejects it outright.
    assert!(fs::read_to_string(corpus_root().join("malformed/invalid_utf8.bru")).is_err());

    // A bare CR is not a line terminator for this parser, so the whole file is
    // one line and there is no block header.
    assert!(parse(&m("lone_cr.bru")).is_err());
    // CRLF is stripped, but a bare CR is not. In `mixed_line_endings.bru` that
    // glues meta's closing brace onto a value line, so the scanner keeps
    // reading until the *next* `}` at column 0 and swallows the whole `get`
    // block. Ok, silently wrong, no panic — pinned as-is.
    let mixed = parse(&m("mixed_line_endings.bru")).expect("mixed line endings");
    assert_eq!(mixed.name, "mixed");
    assert_eq!(mixed.method, "", "the get block was absorbed into meta");

    // A block header nested inside another block is not rejected either: the
    // inner header has no `:` so `parse_dict` skips it and its contents are
    // flattened into the outer block's pairs.
    let nested = parse(&m("nested_block_headers.bru")).expect("nested headers");
    assert_eq!(nested.url, "http://x");
    assert!(
        nested.headers.is_empty(),
        "the nested block is not promoted"
    );

    // Top-level `key: value` lines are the single-line block form used by
    // environment files, so on their own they parse as (unknown) blocks.
    let single = parse("color: red\nname: x\n").expect("single-line blocks");
    assert_eq!(single.extra["block.color"], "red");

    // An unclosed `@description('''` is tolerated as well: the annotation
    // skipper runs to the end of the block body and the block's own `}` still
    // closes it. Here the annotation is inside `vars:pre-request`, which is
    // stored as raw text, so it survives verbatim.
    let ann = parse(&m("annotation_unclosed.bru")).expect("unclosed annotation");
    assert!(ann.extra["vars:pre-request"].contains("@description('''"));

    // Pathological sizes terminate and are handled, not rejected on size.
    assert!(parse(&m("huge_key.bru")).is_ok());
    let huge = parse(&m("huge_single_line.bru")).expect("1 MB line");
    assert!(matches!(huge.body, Body::Text(t) if t.len() >= 1_000_000));
    let many = parse(&m("many_blocks.bru")).expect("10k blocks");
    assert!(
        many.extra.len() >= 10_000,
        "kept {} blocks",
        many.extra.len()
    );
    assert!(
        parse(&m("deep_brackets.bru")).is_ok(),
        "no recursion, so no stack overflow"
    );
}

// ---------------------------------------------------------------------------
// mutation testing
// ---------------------------------------------------------------------------

#[test]
fn mutations_never_panic() {
    // 60s ceiling; the loop itself runs in ~1s. Generous so a slow CI box does
    // not flake, tight enough that a genuine hang is caught.
    with_timeout(60, "mutations_never_panic", || {
        let mut failures = Vec::new();
        let mut total = 0usize;
        for path in corpus_files() {
            let bytes = fs::read(&path).expect("read");
            for (label, mutant) in mutations(&bytes, path_seed(&path)) {
                let src = String::from_utf8_lossy(&mutant).into_owned();
                total += 1;
                if let Err(p) = catch(|| parse(&src)) {
                    failures.push(format!(
                        "SEED={SEED:#x} {} [{label}]: parse panicked: {p}",
                        rel(&path)
                    ));
                }
                if let Err(p) = catch(|| parse_environment(&src)) {
                    failures.push(format!(
                        "SEED={SEED:#x} {} [{label}]: parse_environment panicked: {p}",
                        rel(&path)
                    ));
                }
            }
        }
        assert!(
            total > 5_000,
            "only {total} mutations generated — the mutator is broken"
        );
        println!("mutations: {total} parses, SEED={SEED:#x}");
        report("panics on mutated input", failures);
    });
}

/// A mutant that still parses must also serialize and re-parse without
/// panicking. Round-tripping mutated input is *not* asserted to be lossless —
/// a mutant is by definition not a well-formed file — only that it is safe.
#[test]
fn mutants_that_parse_also_serialize_safely() {
    with_timeout(60, "mutants_that_parse_also_serialize_safely", || {
        let mut failures = Vec::new();
        for path in corpus_files() {
            let bytes = fs::read(&path).expect("read");
            for (label, mutant) in mutations(&bytes, path_seed(&path)) {
                let src = String::from_utf8_lossy(&mutant).into_owned();
                let Ok(Ok(req)) = catch(|| parse(&src)) else {
                    continue;
                };
                match catch(|| serialize(&req)) {
                    Err(p) => failures.push(format!(
                        "SEED={SEED:#x} {} [{label}]: serialize panicked: {p}",
                        rel(&path)
                    )),
                    Ok(text) => {
                        if let Err(p) = catch(|| parse(&text)) {
                            failures.push(format!(
                                "SEED={SEED:#x} {} [{label}]: reparse of serialized mutant panicked: {p}",
                                rel(&path)
                            ));
                        }
                    }
                }
            }
        }
        report("panics serializing mutated input", failures);
    });
}

#[test]
fn prng_is_deterministic() {
    // Pins the PRNG so a "harmless" refactor cannot silently change which
    // mutations the suite actually explores.
    let mut a = Lcg::new(SEED);
    let first: Vec<u32> = (0..4).map(|_| a.next_u32()).collect();
    let mut b = Lcg::new(SEED);
    let again: Vec<u32> = (0..4).map(|_| b.next_u32()).collect();
    assert_eq!(first, again);
    assert_ne!(first[0], first[1], "an LCG that repeats is not an LCG");
    assert_eq!(
        Lcg::new(SEED).below(0),
        0,
        "below(0) must not divide by zero"
    );
    // And the per-file seed must be stable, not hash-order dependent.
    assert_eq!(
        path_seed(Path::new("a/b.bru")),
        path_seed(Path::new("a/b.bru"))
    );
    assert_ne!(
        path_seed(Path::new("a/b.bru")),
        path_seed(Path::new("a/c.bru"))
    );
}

#[test]
fn timeout_harness_actually_fires() {
    // Guards the guard: if `with_timeout` silently never timed out, every
    // hang-detection test above would be decoration.
    let r = catch(|| {
        with_timeout(1, "deliberate hang", || {
            thread::sleep(Duration::from_secs(30))
        })
    });
    let msg = r.expect_err("with_timeout should have panicked on a hang");
    assert!(
        msg.contains("did not terminate"),
        "unexpected message: {msg}"
    );
}

// ---------------------------------------------------------------------------
// STEP 4 — targeted semantics
// ---------------------------------------------------------------------------

/// Reads a corpus file by its path relative to corpus/bru/.
fn corpus_src(name: &str) -> String {
    read_text(&corpus_root().join(name))
}

#[test]
fn parse_environment_over_real_environment_files() {
    let mut failures = Vec::new();
    let mut seen = 0;
    for path in corpus_files() {
        if !is_environment(&path) {
            continue;
        }
        seen += 1;
        match parse_environment(&read_text(&path)) {
            Ok(env) => {
                if env.vars.is_empty() {
                    failures.push(format!("{}: parsed but produced no vars", rel(&path)));
                }
                // Every var name must be non-empty and untrimmed-whitespace-free.
                for (n, _, _) in &env.vars {
                    if n.trim() != n || n.is_empty() {
                        failures.push(format!("{}: bad var name {n:?}", rel(&path)));
                    }
                }
            }
            Err(e) => failures.push(format!("{}: {e}", rel(&path))),
        }
    }
    assert!(
        seen >= 9,
        "only {seen} environment files found in the corpus"
    );
    report("environment files", failures);
}

#[test]
fn parse_environment_typed_and_secret_vars() {
    let env = parse_environment(&corpus_src("collection/environments/Prod.bru")).unwrap();
    assert_eq!(
        env.vars[0],
        (
            "host".into(),
            "https://testbench-sanity.usebruno.com".into(),
            false
        )
    );
    // `@number` / `@boolean` / `@object` annotations are dropped; the value is
    // kept as text and the type is lost. Pinned deliberately.
    assert!(
        env.vars
            .iter()
            .any(|(n, v, s)| n == "typed_env_num" && v == "300" && !s)
    );
    assert!(
        env.vars
            .iter()
            .any(|(n, v, s)| n == "typed_env_bool" && v == "true" && !s)
    );
    assert!(
        env.vars
            .iter()
            .any(|(n, v, s)| n == "typed_env_obj" && v == "{\"scope\":\"env\"}" && !s),
        "multiline '''-quoted object value: {:?}",
        env.vars
    );
    // Dotted and dashed var names survive.
    assert!(env.vars.iter().any(|(n, _, _)| n == "env.var1"));
    assert!(env.vars.iter().any(|(n, _, _)| n == "env-var2"));

    // `vars:secret [ ... ]` yields name-only vars flagged secret, with an
    // empty value — the value is never in the file.
    let local = parse_environment(&corpus_src("collection/environments/Local.bru")).unwrap();
    let secrets: Vec<&str> = local
        .vars
        .iter()
        .filter(|(_, _, s)| *s)
        .map(|(n, v, _)| {
            assert_eq!(v, "", "a secret var must have no value in the file");
            n.as_str()
        })
        .collect();
    assert!(secrets.contains(&"github_client_secret"), "{secrets:?}");
    assert!(secrets.contains(&"google_client_id"), "{secrets:?}");
    // Comma-separated entries inside the bracket list are split, and the
    // trailing comma does not produce an empty name.
    assert!(secrets.iter().all(|n| !n.contains(',') && !n.is_empty()));

    // A single-entry, comma-free list works too.
    let kc = parse_environment(&corpus_src(
        "keycloak-client-credentials/environments/oauth2.bru",
    ))
    .unwrap();
    assert_eq!(
        kc.vars
            .iter()
            .filter(|(_, _, s)| *s)
            .map(|(n, _, _)| n.as_str())
            .collect::<Vec<_>>(),
        vec!["client_secret"]
    );

    // ponytail: not asserted — `color:` and `vars:externalsecrets:*` blocks are
    // documented as dropped by `parse_environment`, and there is no round-trip
    // for environments to make the loss observable. See
    // `environment_blocks_without_a_home_are_dropped` below for the pin.
}

#[test]
fn environment_blocks_without_a_home_are_dropped() {
    // Pins the documented lossy path: anything that is not `vars` or
    // `vars:secret` vanishes. `Environment` has no `extra`, so this cannot be
    // round-tripped — a future change here is a deliberate model change.
    let env = parse_environment(
        "color: red\n\nvars {\n  a: 1\n}\n\nvars:externalsecrets:aws {\n  b: 2\n}\n",
    )
    .unwrap();
    assert_eq!(env.vars, vec![("a".to_string(), "1".to_string(), false)]);
}

#[test]
fn disabled_entries_keep_their_marker_across_the_corpus() {
    // The `~` convention: the marker stays glued to the key for headers/query/
    // form pairs, and becomes `enabled: false` for asserts. Anywhere in the
    // corpus that uses it must survive a round-trip with the marker intact.
    let mut checked = 0;
    let mut failures = Vec::new();
    for path in corpus_files() {
        if is_environment(&path) {
            continue;
        }
        let Ok(a) = parse(&read_text(&path)) else {
            continue;
        };
        let disabled = |r: &rp_core::model::Request| {
            let pairs = r
                .headers
                .iter()
                .chain(r.query.iter())
                .chain(match &r.body {
                    Body::Form(f) => f.iter(),
                    _ => [].iter(),
                })
                .filter(|(k, _)| k.starts_with('~'))
                .count();
            let asserts = r.asserts.iter().filter(|x| !x.enabled).count();
            (pairs, asserts)
        };
        let (disabled_headers, disabled_asserts) = disabled(&a);
        if disabled_headers == 0 && disabled_asserts == 0 {
            continue;
        }
        checked += 1;
        let text = serialize(&a);
        let b = match parse(&text) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!("{}: {e}", rel(&path)));
                continue;
            }
        };
        if disabled(&b) != (disabled_headers, disabled_asserts) {
            failures.push(format!(
                "{}: disabled markers were lost ({:?} -> {:?})",
                rel(&path),
                (disabled_headers, disabled_asserts),
                disabled(&b)
            ));
        }
    }
    assert!(
        checked >= 2,
        "only {checked} corpus files exercise the `~` marker"
    );
    report("disabled-entry handling", failures);
}

#[test]
fn multiline_values_with_awkward_indentation() {
    // `'''` values are outdented by up to 4 spaces and then trimmed. These pin
    // exactly what that means, including the lossy cases.

    // Standard 4-space indentation.
    let r = parse("get {\n  url: u\n}\n\nheaders {\n  m: '''\n    a\n    b\n  '''\n}\n").unwrap();
    assert_eq!(r.headers[0].1, "a\nb");

    // Under-indented: nothing to strip, content kept as-is.
    let r = parse("get {\n  url: u\n}\n\nheaders {\n  m: '''\na\nb\n'''\n}\n").unwrap();
    assert_eq!(r.headers[0].1, "a\nb");

    // Over-indented: only the first 4 spaces come off... except that the whole
    // joined value is then `trim()`ed, so the *first* line loses its remaining
    // indentation while every later line keeps it. Asymmetric and lossy; pinned
    // rather than endorsed.
    let r = parse("get {\n  url: u\n}\n\nheaders {\n  m: '''\n        a\n        b\n  '''\n}\n")
        .unwrap();
    assert_eq!(r.headers[0].1, "a\n    b");

    // Ragged indentation is preserved relative to the 4-space cut, except that
    // the leading whitespace of the *first* line is trimmed away. Lossy, pinned.
    let r = parse("get {\n  url: u\n}\n\nheaders {\n  m: '''\n      a\n    b\n  '''\n}\n").unwrap();
    assert_eq!(
        r.headers[0].1, "a\nb",
        "leading indent of the first line is trimmed"
    );

    // Blank lines inside the value survive; leading/trailing ones do not.
    let r =
        parse("get {\n  url: u\n}\n\nheaders {\n  m: '''\n\n    a\n\n    b\n\n  '''\n}\n").unwrap();
    assert_eq!(r.headers[0].1, "a\n\nb");

    // Opened and closed on the same line.
    let r = parse("get {\n  url: u\n}\n\nheaders {\n  m: '''one line'''\n}\n").unwrap();
    assert_eq!(r.headers[0].1, "one line");

    // Trailing text after the closing ''' is appended with a single space.
    let r = parse("get {\n  url: u\n}\n\nheaders {\n  m: '''\n    a\n  ''' tail\n}\n").unwrap();
    assert_eq!(r.headers[0].1, "a tail");

    // Tab-indented multiline content: tabs are not spaces, so nothing is
    // stripped and the tabs stay in the value.
    let r = parse("get {\n  url: u\n}\n\nheaders {\n  m: '''\n\t\ta\n\t\tb\n  '''\n}\n").unwrap();
    assert_eq!(
        r.headers[0].1, "a\n\t\tb",
        "first line trimmed, later tabs kept"
    );

    // Real corpus file with a multiline multipart value.
    let echo = parse(&corpus_src("collection/echo/echo multipart.bru")).unwrap();
    match &echo.body {
        Body::MultipartForm(f) => {
            let ml = f
                .iter()
                .find(|f| f.name == "multiline")
                .expect("multiline field");
            assert_eq!(ml.value, "\"multiline-test\"");
            assert_eq!(
                ml.content_type.as_deref(),
                Some("application/json--multiline--test")
            );
        }
        other => panic!("expected multipart, got {other:?}"),
    }
}

#[test]
fn content_type_extraction() {
    // ` @contentType(x)` is pulled off the end of a value.
    let f = parse(&corpus_src("collection/file-binary/binary-upload-json.bru")).unwrap();
    assert_eq!(
        f.body,
        Body::File {
            path: "file.json".into(),
            content_type: "application/json".into()
        }
    );

    let m = parse(&corpus_src(
        "collection/multipart/multipart-mixed-form-data-file.bru",
    ))
    .unwrap();
    match &m.body {
        Body::MultipartForm(fields) => {
            let s = fields
                .iter()
                .find(|f| f.name == "sample")
                .expect("sample field");
            assert_eq!(s.file_path.as_deref(), Some("bruno.png"));
            assert_eq!(s.content_type.as_deref(), Some("image/png"));
            assert_eq!(s.value, "", "a file field carries no inline value");
        }
        other => panic!("expected multipart, got {other:?}"),
    }

    // Only a *trailing* @contentType is extracted...
    let r = parse("post {\n  url: u\n  body: multipartForm\n}\n\nbody:multipart-form {\n  a: v @contentType(text/plain)\n}\n").unwrap();
    match &r.body {
        Body::MultipartForm(f) => {
            assert_eq!(f[0].value, "v");
            assert_eq!(f[0].content_type.as_deref(), Some("text/plain"));
        }
        other => panic!("{other:?}"),
    }
    // ...one in the middle is left alone, because the value must end in `)`.
    let r = parse("post {\n  url: u\n  body: multipartForm\n}\n\nbody:multipart-form {\n  a: @contentType(text/plain) v\n}\n").unwrap();
    match &r.body {
        Body::MultipartForm(f) => {
            assert_eq!(f[0].value, "@contentType(text/plain) v");
            assert_eq!(f[0].content_type, None);
        }
        other => panic!("{other:?}"),
    }
    // A nested `)` inside the parens disables extraction rather than guessing.
    let r = parse("post {\n  url: u\n  body: multipartForm\n}\n\nbody:multipart-form {\n  a: v @contentType(a(b))\n}\n").unwrap();
    match &r.body {
        Body::MultipartForm(f) => assert_eq!(f[0].content_type, None),
        other => panic!("{other:?}"),
    }
    // An empty content type extracts as an empty string, and serialize then
    // drops the annotation entirely.
    let r = parse("post {\n  url: u\n  body: multipartForm\n}\n\nbody:multipart-form {\n  a: v @contentType()\n}\n").unwrap();
    match &r.body {
        Body::MultipartForm(f) => assert_eq!(f[0].content_type.as_deref(), Some("")),
        other => panic!("{other:?}"),
    }
    assert!(!serialize(&r).contains("@contentType"));
}

#[test]
fn per_pair_description_annotations_are_dropped() {
    // Documented lossy path: `@description(...)` above a dictionary entry has
    // nowhere to live in the model, so it is discarded — both the single-line
    // and the multiline `'''` form. Pinned so restoring it is a visible choice.

    // The plain multiline form drops just the annotation.
    let r = parse("get {\n  url: u\n}\n\nheaders {\n  @description('''\n    text\n  ''')\n  a: 1\n  b: 2\n}\n").unwrap();
    assert_eq!(
        r.headers,
        vec![
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "2".to_string())
        ]
    );
    let text = serialize(&r);
    assert!(!text.contains("@description"), "and it is not re-emitted");
    assert_eq!(parse(&text).unwrap(), r);

    // The real corpus file uses the *single-line* `@description('''asd''')`
    // form. It used to eat every entry up to the next `''')`; now only the
    // annotation itself is dropped. Updated when
    // `bug_single_line_triple_quoted_annotation_swallows_following_entries`
    // was fixed.
    let src = corpus_src("collection/collection.bru");
    assert!(src.contains("@description('''asd''')"));
    let coll = parse(&src).unwrap();
    assert_eq!(
        coll.headers,
        vec![
            ("check".to_string(), "again".to_string()),
            (
                "token".to_string(),
                "{{collection_pre_var_token}}".to_string()
            ),
            (
                "collection-header".to_string(),
                "collection-header-value".to_string()
            ),
        ]
    );
    let r = coll;

    // `@number` / `@boolean` / `@object` type annotations inside
    // `vars:pre-request` are the exception: that block is stored as raw text,
    // so they survive verbatim.
    assert!(r.extra["vars:pre-request"].contains("@number\ncoll_num: 100"));
    assert!(r.extra["vars:pre-request"].contains("@object\ncoll_obj: '''"));
}

#[test]
fn unmodeled_auth_kinds_land_in_extra() {
    // The model covers basic/bearer/apikey/oauth2/inherit. Everything else is
    // kept verbatim under `extra["auth:<kind>"]` with `auth == Auth::None`, and
    // must round-trip.
    for kind in ["digest", "ntlm", "awsv4", "wsse", "nonsense"] {
        let src = format!(
            "get {{\n  url: u\n  auth: {kind}\n}}\n\nauth:{kind} {{\n  username: u\n  password: p\n}}\n"
        );
        let r = parse(&src).unwrap();
        assert_eq!(r.auth, Auth::None, "{kind}");
        assert_eq!(
            r.extra[&format!("auth:{kind}")],
            "username: u\npassword: p",
            "{kind}"
        );
        assert_eq!(
            parse(&serialize(&r)).unwrap(),
            r,
            "{kind} did not round-trip"
        );
    }

    // An auth block whose kind is *not* the declared mode is also preserved.
    let r = parse(
        "get {\n  url: u\n  auth: bearer\n}\n\nauth:bearer {\n  token: t\n}\n\nauth:digest {\n  username: u\n}\n",
    )
    .unwrap();
    assert!(matches!(r.auth, Auth::Bearer { .. }));
    assert_eq!(r.extra["auth:digest"], "username: u");
    assert_eq!(parse(&serialize(&r)).unwrap(), r);

    // oauth2 fields outside the seven modelled ones ride along in extra under
    // a dotted key, and come back out on serialize.
    let name = "keycloak-client-credentials/user_info_request-auth.bru";
    let o = parse(&corpus_src(name)).unwrap();
    assert!(
        matches!(o.auth, Auth::OAuth2(_)),
        "{name}: expected oauth2, got {:?}",
        o.auth
    );
    let extras: Vec<&String> = o
        .extra
        .keys()
        .filter(|k| k.starts_with("auth:oauth2."))
        .collect();
    assert!(
        !extras.is_empty(),
        "{name}: expected unmodelled oauth2 fields in extra"
    );
    assert_eq!(parse(&serialize(&o)).unwrap(), o);
}

#[test]
fn unknown_blocks_and_extra_keys_survive_a_round_trip() {
    // The `extra` map is the whole reason parse/serialize is not lossy. Assert
    // that for every corpus file, no `extra` key is lost by a round-trip —
    // this is what stops a future refactor from quietly deleting user data.
    let mut failures = Vec::new();
    for path in corpus_files() {
        if is_environment(&path) {
            continue;
        }
        let Ok(a) = parse(&read_text(&path)) else {
            continue;
        };
        let Ok(b) = parse(&serialize(&a)) else {
            continue;
        };
        for k in a.extra.keys() {
            if !b.extra.contains_key(k) {
                failures.push(format!(
                    "{}: extra key {k:?} lost on round-trip",
                    rel(&path)
                ));
            }
        }
    }
    report("extra keys lost on round-trip", failures);
}

// ---------------------------------------------------------------------------
// Known bugs
//
// These tests assert the behaviour the parser *should* have. They are
// `#[ignore]`d so the suite stays green, and each one is a standing invitation
// to fix `crates/core/src/bru.rs`. Run them with:
//     cargo test -p rp-core --test bru_corpus -- --ignored
// ---------------------------------------------------------------------------

#[test]
fn bug_single_line_triple_quoted_annotation_swallows_following_entries() {
    // `parse_dict` sees `('''` in the line and assumes a multiline annotation,
    // so it skips forward from the *next* line looking for a line ending in
    // `''')` — never noticing that the same line already closed it. Everything
    // in between is silently deleted.
    //
    // This is live data loss on real upstream files: two of the three headers in
    // `corpus/bru/collection/collection.bru` disappear.
    let r = parse(
        "get {\n  url: u\n}\n\nheaders {\n  @description('''one line''')\n  a: 1\n  b: 2\n}\n",
    )
    .unwrap();
    assert_eq!(
        r.headers,
        vec![
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "2".to_string())
        ],
        "the annotation should be dropped, not the entries after it"
    );

    let coll = parse(&corpus_src("collection/collection.bru")).unwrap();
    assert_eq!(
        coll.headers,
        vec![
            ("check".to_string(), "again".to_string()),
            (
                "token".to_string(),
                "{{collection_pre_var_token}}".to_string()
            ),
            (
                "collection-header".to_string(),
                "collection-header-value".to_string()
            ),
        ]
    );
}

#[test]
fn bug_multiline_assert_value_does_not_round_trip() {
    // `parse_dict` only starts a multiline read when the value *begins* with
    // `'''`. In an assert the value begins with the operator (`eq '''`), so the
    // `'''` is taken literally and the content lines — having no `:` — are
    // skipped one by one. Two lines of the user's file are deleted.
    //
    // The serialize side has the mirror-image gap: `push_pairs` runs values
    // through `value_str` (which re-wraps a multiline value in `'''`) but the
    // `assert` arm formats `"{expr}: {op} {value}"` by hand, so even a
    // correctly-parsed multiline value could not be written back out.
    let a = parse("get {\n  url: u\n}\n\nassert {\n  res.body: eq '''\n    x\n    y\n  '''\n}\n")
        .unwrap();
    assert_eq!(a.asserts.len(), 1);
    assert_eq!(a.asserts[0].value, "x\ny");
    let b = parse(&serialize(&a)).unwrap();
    assert_eq!(b, a, "serialized:\n{}", serialize(&a));
}

#[test]
fn bug_assert_expr_containing_a_colon_does_not_round_trip() {
    // Every other pair writer goes through `key_str`, which quotes a key
    // containing `:`, `\"`, `{`, `}` or a space. The `assert` arm interpolates
    // `a.expr` directly, so the colon in the expression is re-read as the
    // key/value separator.
    let a = parse("get {\n  url: u\n}\n\nassert {\n  \"res.headers['x:y']\": eq 1\n}\n").unwrap();
    assert_eq!(a.asserts[0].expr, "res.headers['x:y']");
    assert_eq!(a.asserts[0].value, "1");
    let b = parse(&serialize(&a)).unwrap();
    assert_eq!(b, a, "serialized:\n{}", serialize(&a));
}

#[test]
fn bug_empty_text_blocks_are_dropped_by_serialize() {
    // `push_text` returns early on an empty string, so `Body::Json(\"\")` and
    // `extra[\"docs\"] == \"\"` vanish. The block existed in the user's file and
    // does not come back.
    let a = parse("post {\n  url: u\n  body: json\n}\n\nbody:json {\n}\n\ndocs {\n}\n").unwrap();
    assert_eq!(a.body, Body::Json(String::new()));
    assert_eq!(a.extra["docs"], "");
    let b = parse(&serialize(&a)).unwrap();
    assert_eq!(b, a, "serialized:\n{}", serialize(&a));
}
