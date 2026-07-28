//! Property tests for the tool registry.
//!
//! The UI calls `tool.run(args)` on every keystroke, so the args it passes are
//! always half-typed garbage. The invariant under test is therefore not
//! "produces the right answer" (unit tests in `src/tools/*` cover that) but
//! **no tool may ever panic or hang, for any input**.
//!
//! No new dependencies: the PRNG is a 12-line LCG with a fixed seed, so a
//! failure here reproduces exactly. Where a case does fail, the test prints the
//! seed, the tool id, the field and the input.
//!
//! ## Hangs
//! Every tool's whole input battery runs on its own worker thread, collected
//! with `mpsc::recv_timeout`. A tool that loops forever fails its own case with
//! its id named, instead of wedging CI until the job timeout. The stuck thread
//! is deliberately leaked — the test process is about to fail and exit anyway,
//! and killing a thread is not something std offers.
//!
//! ## Breadth traded for speed (budget: < 60s total, runs on every push)
//! - The 1 MB and 10k-nesting inputs run once per tool, not once per field.
//! - `SLOW` tools (KDF / RSA keygen — seconds per call by design) get a
//!   three-input smoke battery instead of the full one.
//! - The random sweep is 12 iterations per tool, not hundreds. A fuzz target
//!   (`fuzz/fuzz_targets/tools_run.rs`) does the deep search continuously; this
//!   suite is the fast regression gate.

use rp_devtools::{Args, Field, FieldKind, Kind, Tool, all, find, search};
use std::sync::mpsc;
use std::time::Duration;

/// Deliberately expensive by design: a work-factor KDF and RSA key generation.
/// Running the full battery against them would blow the whole time budget.
const SLOW: &[&str] = &["bcrypt", "rsa-key-pair-generator"];

/// Panics this suite FOUND and that are not fixed yet, as `(tool id, battery
/// case name)`. This is a quarantine list, not an approval list: each entry is a
/// real crash in the UI (the tool panics on a keystroke) and must be deleted
/// once the tool is fixed. `known_panics_are_still_broken` fails if an entry
/// starts passing, so the list cannot rot. Currently empty: the last three
/// entries (`eta-calculator` on `huge int` / `i64 min` / `u64 max`) were fixed
/// by the `try_milliseconds` + `checked_add_signed` guard in `eta_calculator`,
/// and `known_panics_are_still_broken` duly went red until they were removed.
const KNOWN_PANICS: &[(&str, &str)] = &[];

/// Tools whose output is *supposed* to differ between two identical calls, so
/// they are exempt from the determinism check. Keyed by id on purpose: a tool
/// that newly becomes nondeterministic is a bug (it would make the UI flicker
/// on every keystroke) and must fail loudly here rather than be tolerated.
const NONDETERMINISTIC: &[&str] = &[
    // Random by definition.
    "token-generator",
    "uuid-generator",
    "ulid-generator",
    "mac-address-generator",
    "random-port-generator",
    "lorem-ipsum-generator",
    "bip39-generator",
    "rsa-key-pair-generator",
    "encryption", // random salt per call
    "ipv6-ula-generator",
    // Clock-dependent.
    "eta-calculator", // prints "It will end at: <now + duration>"
    "otp-code-generator-and-validator",
    "crontab-generator",
    "chronometer",
    "date-time-converter",
    "git-memo",
    // The pull-request batch: five generators and one "blank means today".
    "fake-data-generator",
    "passphrase-generator",
    "pin-code-generator",
    "nanoid-generator",
    "objectid-generator-and-parser",
    "week-numbers-converter",
    // The parallel batch: a random salt, random PKCS#1 padding, a die, and a
    // blank start date meaning "today".
    "htpasswd-generator",
    "rsa-encryption",
    "dice-roller",
    "date-duration-calculator",
];

/// Collects every violation instead of dying on the first one, so one run tells
/// you about all the broken tools rather than making you fix them one at a time.
#[derive(Default)]
struct Failures(Vec<String>);

impl Failures {
    fn push(&mut self, msg: String) {
        self.0.push(msg);
    }
    fn assert_empty(self) {
        if !self.0.is_empty() {
            panic!(
                "{} violation(s):\n  - {}",
                self.0.len(),
                self.0.join("\n  - ")
            );
        }
    }
}

/// One worker thread per call site, so a hang is reported instead of hanging CI.
/// Returns `None` if the tool panicked or hung; the case is recorded in `fail`.
fn run_guarded(
    tool: &'static Tool,
    args: Args,
    what: &str,
    fail: &mut Failures,
) -> Option<Result<String, String>> {
    let (tx, rx) = mpsc::channel();
    // 32 MB stack: several tools wrap recursive-descent parsers (serde_json,
    // quick-xml, toml) and the deep-nesting inputs below need more than the
    // 2 MB a spawned thread gets by default — an unoptimised debug build uses
    // far bigger frames than the release build the app ships. A stack overflow
    // aborts the whole process and cannot be caught, so it must be given room
    // rather than reported.
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            // Any panic inside `run` unwinds this thread; the channel then hangs up
            // and `recv_timeout` reports Disconnected. The default panic hook still
            // prints the panic message, which is the useful half of the report.
            let out = (tool.run)(&args);
            let _ = tx.send(out.map_err(|e| e.to_string()));
        })
        .expect("spawn worker thread");
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(r) => Some(r),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            fail.push(format!("tool `{}` HUNG (>20s) on {what}", tool.id));
            None
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            fail.push(format!("tool `{}` PANICKED on {what}", tool.id));
            None
        }
    }
}

fn defaults(tool: &Tool) -> Args {
    tool.fields
        .iter()
        .map(|f| (f.key.to_string(), f.default.to_string()))
        .collect()
}

/// Small, cheap, nasty. Applied to every field of every tool.
fn battery() -> Vec<(&'static str, String)> {
    let mut v: Vec<(&'static str, String)> = vec![
        ("empty", String::new()),
        ("spaces", "   \t\r\n  ".into()),
        ("nul", "a\0b".into()),
        ("only nul", "\0".into()),
        // A String cannot hold invalid UTF-8, so this is what the UI actually
        // receives: bytes off a multi-byte boundary, lossily converted.
        (
            "lossy surrogate",
            String::from_utf8_lossy(&[0xED, 0xA0, 0x80]).into_owned(),
        ),
        (
            "lossy truncated",
            String::from_utf8_lossy(&[b'a', 0xE2, 0x82]).into_owned(),
        ),
        (
            "lone continuation",
            String::from_utf8_lossy(&[0x80, 0x80, b'x']).into_owned(),
        ),
        ("replacement char", "\u{FFFD}\u{FFFD}".into()),
        ("negative", "-5".into()),
        ("negative float", "-3.5".into()),
        ("minus zero", "-0".into()),
        ("1e400", "1e400".into()),
        ("-1e400", "-1e400".into()),
        ("nan", "NaN".into()),
        ("inf", "Infinity".into()),
        ("huge int", "99999999999999999999999999999999999999".into()),
        ("i64 min", i64::MIN.to_string()),
        ("u64 max", u64::MAX.to_string()),
        ("emoji", "👨‍👩‍👧‍👦🇩🇪💩".into()),
        ("rtl", "مرحبا بالعالم \u{202E}reversed".into()),
        (
            "combining",
            "e\u{0301}\u{0301}\u{0301}a\u{0335}\u{0489}".into(),
        ),
        ("ascii controls", (0u8..0x20).map(|b| b as char).collect()),
        ("del + high", "\u{7F}\u{80}\u{9F}".into()),
        // Valid-looking but wrong: the half-typed shapes the UI sees constantly.
        ("truncated json", "[1,2".into()),
        ("truncated json obj", "{\"a\":".into()),
        ("json null", "null".into()),
        ("bad yaml", "a:\n - b\n\tc: 1".into()),
        ("bad toml", "[a\nb =".into()),
        ("bad xml", "<a><b></a>".into()),
        ("bad base64", "aGVsbG8".into()),
        ("bad hex", "0xZZ".into()),
        ("bad url", "ht!tp://[::".into()),
        ("bad date", "2024-13-45T99:99:99Z".into()),
        ("bad cidr", "999.999.999.999/99".into()),
        ("bad regex", "([a-".into()),
        ("path traversal", "../../etc/passwd".into()),
        ("format specifiers", "%s %d {} {0} ${x} {{x}}".into()),
        ("sql-ish", "'; DROP TABLE t; --".into()),
        ("html-ish", "<script>alert(1)</script>".into()),
        ("single quote", "'".into()),
        ("backslashes", "\\\\\\".into()),
        ("crlf only", "\r\n\r\n".into()),
    ];
    v.push(("long line", "a".repeat(4096)));
    v
}

/// Expensive inputs: once per tool, on one field only (see module header).
fn heavy() -> Vec<(&'static str, String)> {
    vec![
        ("1MB of a", "a".repeat(1024 * 1024)),
        ("10k open braces", "{".repeat(10_000)),
        ("10k open brackets", "[".repeat(10_000)),
        (
            "10k nested pairs",
            format!("{}{}", "[".repeat(10_000), "]".repeat(10_000)),
        ),
        (
            "100k controls",
            (0u8..0x20)
                .map(|b| b as char)
                .cycle()
                .take(100_000)
                .collect(),
        ),
    ]
}

#[test]
fn registry_is_well_formed() {
    let tools = all();
    assert!(
        tools.len() > 50,
        "registry suspiciously small: {}",
        tools.len()
    );
    let mut ids: Vec<&str> = tools.iter().map(|t| t.id).collect();
    ids.sort_unstable();
    let before = ids.len();
    ids.dedup();
    assert_eq!(before, ids.len(), "duplicate tool ids in all()");

    for t in tools {
        assert!(!t.name.trim().is_empty(), "{}: empty name", t.id);
        assert!(
            !t.description.trim().is_empty(),
            "{}: empty description",
            t.id
        );
        assert!(!t.id.trim().is_empty(), "empty id on {}", t.name);
        let mut keys: Vec<&str> = t.fields.iter().map(|f| f.key).collect();
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(n, keys.len(), "{}: duplicate field keys", t.id);
        for f in t.fields {
            assert!(
                !f.label.trim().is_empty(),
                "{}.{}: empty label",
                t.id,
                f.key
            );
            if f.kind == FieldKind::Select {
                assert!(
                    !f.options.is_empty(),
                    "{}.{}: select with no options",
                    t.id,
                    f.key
                );
                assert!(
                    f.options.contains(&f.default),
                    "{}.{}: default {:?} is not one of {:?}",
                    t.id,
                    f.key,
                    f.default,
                    f.options
                );
            }
            if f.kind == FieldKind::Bool {
                assert!(
                    matches!(f.default, "true" | "false"),
                    "{}.{}: bool default {:?}",
                    t.id,
                    f.key,
                    f.default
                );
            }
        }
    }
}

#[test]
fn defaults_never_panic() {
    let mut fail = Failures::default();
    for t in all() {
        run_guarded(t, defaults(t), "its own field defaults", &mut fail);
    }
    fail.assert_empty();
}

#[test]
fn adversarial_battery_per_field() {
    let battery = battery();
    let mut fail = Failures::default();
    for t in all() {
        let cases: &[(&str, String)] = if SLOW.contains(&t.id) {
            &battery[..3]
        } else {
            &battery
        };
        for f in t.fields {
            for (name, value) in cases {
                if KNOWN_PANICS
                    .iter()
                    .any(|(id, case)| *id == t.id && case == name)
                {
                    continue;
                }
                let mut args = defaults(t);
                args.insert(f.key.to_string(), value.clone());
                // Ok or Err are both fine; only a panic or a hang is recorded.
                run_guarded(t, args, &format!("field `{}` = {name}", f.key), &mut fail);
            }
        }
    }
    fail.assert_empty();
}

#[test]
fn heavy_inputs_once_per_tool() {
    let mut fail = Failures::default();
    for t in all() {
        if SLOW.contains(&t.id) {
            continue;
        }
        // Prefer the biggest free-text input; that is what a user pastes into.
        let Some(f) = t
            .fields
            .iter()
            .find(|f| f.kind == FieldKind::TextArea)
            .or_else(|| t.fields.iter().find(|f| f.kind == FieldKind::Text))
        else {
            continue;
        };
        for (name, value) in heavy() {
            let mut args = defaults(t);
            args.insert(f.key.to_string(), value);
            run_guarded(t, args, &format!("field `{}` = {name}", f.key), &mut fail);
        }
    }
    fail.assert_empty();
}

/// LCG (Numerical Recipes constants). Deterministic, dependency-free, and good
/// enough to shuffle a fixed battery — this is not a statistical generator.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 11
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Values a `Number`/`Bool`/`Select` field plausibly gets typed into it, plus a
/// few that it must survive but never expects.
fn random_value(rng: &mut Lcg, f: &Field, battery: &[(&'static str, String)]) -> String {
    match f.kind {
        FieldKind::Bool => ["true", "false", "TRUE", "1", "0", "yes", ""][rng.below(7)].to_string(),
        FieldKind::Select if rng.below(4) > 0 => f.options[rng.below(f.options.len())].to_string(),
        FieldKind::Number if rng.below(3) > 0 => [
            "0",
            "1",
            "-1",
            "8",
            "64",
            "4096",
            "0.5",
            "-0.5",
            "1e9",
            "2147483648",
        ][rng.below(10)]
        .to_string(),
        _ => battery[rng.below(battery.len())].1.clone(),
    }
}

/// All fields hostile at once — field interactions are where the real crashes
/// live (e.g. a length field disagreeing with a mode field).
#[test]
fn random_sweep_all_fields_at_once() {
    const SEED: u64 = 0x5EED_1234_ABCD_0001;
    let battery = battery();
    let mut fail = Failures::default();
    for t in all() {
        // A quarantined tool would just re-report its known panic here.
        if KNOWN_PANICS.iter().any(|(id, _)| *id == t.id) {
            continue;
        }
        let iters = if SLOW.contains(&t.id) { 2 } else { 12 };
        // Per-tool stream, so adding a tool does not reshuffle the others.
        let mut rng = Lcg(SEED
            ^ t.id
                .bytes()
                .fold(0u64, |a, b| a.wrapping_mul(31) + b as u64));
        for i in 0..iters {
            let mut args = defaults(t);
            for f in t.fields {
                args.insert(f.key.to_string(), random_value(&mut rng, f, &battery));
            }
            let shown: Vec<String> = args
                .iter()
                .map(|(k, v)| format!("{k}={:?}", &v[..v.len().min(60)]))
                .collect();
            run_guarded(
                t,
                args,
                &format!("random sweep seed={SEED:#x} iter={i}: {}", shown.join(" ")),
                &mut fail,
            );
        }
    }
    fail.assert_empty();
}

#[test]
fn pure_tools_are_deterministic() {
    const SEED: u64 = 0x5EED_1234_ABCD_0002;
    let battery = battery();
    let mut fail = Failures::default();
    for t in all() {
        if t.kind != Kind::Pure || NONDETERMINISTIC.contains(&t.id) || SLOW.contains(&t.id) {
            continue;
        }
        let mut rng = Lcg(SEED
            ^ t.id
                .bytes()
                .fold(0u64, |a, b| a.wrapping_mul(31) + b as u64));
        for i in 0..4 {
            let mut args = defaults(t);
            if i > 0 {
                for f in t.fields {
                    args.insert(f.key.to_string(), random_value(&mut rng, f, &battery));
                }
            }
            let a = run_guarded(t, args.clone(), "determinism check", &mut fail);
            let b = run_guarded(t, args.clone(), "determinism check (2nd call)", &mut fail);
            if a.is_some() && a != b {
                fail.push(format!(
                    "tool `{}` is not deterministic (iter {i}, args {:?}): {:?} vs {:?}. \
                     If this is intentional, add its id to NONDETERMINISTIC with a reason.",
                    t.id, args, a, b
                ));
            }
        }
    }
    fail.assert_empty();
}

/// The quarantine list must shrink, never quietly outlive the bug. Fails when a
/// listed case no longer panics: delete the entry, the tool is fixed.
#[test]
fn known_panics_are_still_broken() {
    let battery = battery();
    let mut fail = Failures::default();
    for (id, case) in KNOWN_PANICS {
        let t = find(id).unwrap_or_else(|| panic!("KNOWN_PANICS lists unknown tool `{id}`"));
        let (_, value) = battery
            .iter()
            .find(|(n, _)| n == case)
            .unwrap_or_else(|| panic!("KNOWN_PANICS lists unknown battery case `{case}`"));
        let mut still_panics = false;
        for f in t.fields {
            let mut args = defaults(t);
            args.insert(f.key.to_string(), value.clone());
            let mut sink = Failures::default();
            if run_guarded(t, args, "quarantine recheck", &mut sink).is_none() {
                still_panics = true;
            }
        }
        if !still_panics {
            fail.push(format!(
                "`{id}` no longer panics on `{case}` — remove it from KNOWN_PANICS"
            ));
        }
    }
    fail.assert_empty();
}

#[test]
fn nondeterministic_list_has_no_stale_entries() {
    for id in NONDETERMINISTIC {
        assert!(
            find(id).is_some(),
            "NONDETERMINISTIC lists unknown tool id `{id}`"
        );
    }
    for id in SLOW {
        assert!(find(id).is_some(), "SLOW lists unknown tool id `{id}`");
    }
}

#[test]
fn search_invariants() {
    let tools = all();
    assert_eq!(
        search("").len(),
        tools.len(),
        "empty query must return everything"
    );
    assert_eq!(
        search("   \t ").len(),
        tools.len(),
        "blank query must return everything"
    );
    assert!(
        search("zzzqqqxxx-no-such-tool").is_empty(),
        "query matching nothing must return empty"
    );

    let ids: std::collections::HashSet<&str> = tools.iter().map(|t| t.id).collect();
    let mut rng = Lcg(0x5EED_1234_ABCD_0003);
    let queries: Vec<String> = tools
        .iter()
        .map(|t| t.id.to_string())
        .chain(tools.iter().map(|t| t.name.to_string()))
        .chain(battery().into_iter().map(|(_, v)| v))
        .chain((0..20).map(|_| {
            let t = tools[rng.below(tools.len())];
            let n = 1 + rng.below(t.id.len());
            t.id[..n].to_string()
        }))
        .collect();

    for q in &queries {
        let hits = search(q);
        for h in &hits {
            assert!(
                ids.contains(h.id),
                "search({q:?}) returned a tool not in all()"
            );
        }
        // Subset also means no duplicates.
        let mut seen: Vec<&str> = hits.iter().map(|t| t.id).collect();
        let n = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(n, seen.len(), "search({q:?}) returned duplicates");
        // Case-insensitive: upper and lower must agree.
        let upper: Vec<&str> = search(&q.to_uppercase()).iter().map(|t| t.id).collect();
        let lower: Vec<&str> = search(&q.to_lowercase()).iter().map(|t| t.id).collect();
        assert_eq!(upper, lower, "search is case-sensitive for {q:?}");
    }

    for t in tools {
        assert_eq!(
            find(t.id).map(|f| f.id),
            Some(t.id),
            "find({}) did not round-trip",
            t.id
        );
        // A tool must be findable by its own name and id.
        assert!(
            search(t.id).iter().any(|h| h.id == t.id),
            "search({}) does not find the tool itself",
            t.id
        );
        assert!(
            search(t.name).iter().any(|h| h.id == t.id),
            "search({:?}) does not find {}",
            t.name,
            t.id
        );
    }
    assert!(find("no-such-tool-id").is_none());
    assert!(find("").is_none());
}
