//! Registry-wide properties for the tool port, complementing `tool_props.rs`.
//!
//! `tool_props.rs` owns the "no input may panic, hang or flicker" side: the
//! adversarial battery, determinism, the search invariants and the basic
//! well-formedness of a `Field`. Nothing here repeats any of that. What is
//! pinned here is the *shape of the port* — the conclusions of the audit of all
//! 86 tools against it-tools' real UI, so that undoing one of them is a red test
//! rather than a quiet regression:
//!
//! 1. No tool is modal. The audit found not one it-tools tool has a
//!    direction toggle, so a `select` offering `encode`/`decode` is a bug.
//! 2. A tool must produce something the moment it is opened, or say clearly
//!    what it needs. `svg-placeholder-generator` used to error on open; the
//!    three-way partition below is what keeps that from coming back.
//! 3. Which tools are `Kind::Custom` is a deliberate, reviewed list, not
//!    whatever happens to be unfinished.
//! 4. Ids are the it-tools route segment, kebab-case, and pinned one by one.
//!
//! No new dependencies, deterministic, and every check collects all its
//! failures with the tool id before failing.

use rp_devtools::{Args, FieldKind, Kind, Tool, all, find};

/// Collects every violation instead of dying on the first one.
#[derive(Default)]
struct Failures(Vec<String>);

impl Failures {
    fn push(&mut self, msg: String) {
        self.0.push(msg);
    }
    fn check(&mut self, ok: bool, msg: impl FnOnce() -> String) {
        if !ok {
            self.0.push(msg());
        }
    }
    fn assert_empty(self) {
        assert!(
            self.0.is_empty(),
            "{} violation(s):\n  - {}",
            self.0.len(),
            self.0.join("\n  - ")
        );
    }
}

fn defaults(tool: &Tool) -> Args {
    tool.fields
        .iter()
        .map(|f| (f.key.to_string(), f.default.to_string()))
        .collect()
}

/// Runs on a roomy thread for the same reason `tool_props.rs` does: several
/// tools wrap recursive-descent parsers and a stack overflow cannot be caught.
fn run(tool: &'static Tool, args: Args) -> Result<String, String> {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || (tool.run)(&args).map_err(|e| format!("{e:#}")))
        .expect("spawn")
        .join()
        .unwrap_or_else(|_| Err("PANICKED".to_string()))
}

// ------------------------------------------------------------------- selects

#[test]
fn select_options_are_usable() {
    let mut fail = Failures::default();
    for t in all() {
        for f in t.fields.iter().filter(|f| f.kind == FieldKind::Select) {
            let mut seen: Vec<&str> = f.options.to_vec();
            let n = seen.len();
            seen.sort_unstable();
            seen.dedup();
            fail.check(seen.len() == n, || {
                format!("{}.{}: duplicate options in {:?}", t.id, f.key, f.options)
            });
            fail.check(f.options.len() > 1 || t.id == "ascii-text-drawer", || {
                format!(
                    "{}.{}: a select with {} option(s) is not a choice",
                    t.id,
                    f.key,
                    f.options.len()
                )
            });
            for o in f.options {
                fail.check(!o.trim().is_empty() || o.is_empty(), || {
                    format!("{}.{}: blank-but-not-empty option {o:?}", t.id, f.key)
                });
            }
            // Every option must be selectable, i.e. actually change nothing but
            // itself: picking any option must not turn a working tool into an
            // error, given otherwise-default args.
            for o in f.options {
                let mut args = defaults(t);
                args.insert(f.key.to_string(), (*o).to_string());
                if let Err(e) = run(t, args) {
                    fail.check(e != "PANICKED", || {
                        format!("{}.{} = {o:?} panicked", t.id, f.key)
                    });
                }
            }
        }
    }
    fail.assert_empty();
}

/// Labels within one tool must be tellable apart: the generic form renders them
/// as a flat list, so two identical labels are two indistinguishable inputs.
/// `percentage-calculator` is the one offender; see the ignored test below.
const KNOWN_DUPLICATE_LABELS: &[&str] = &["percentage-calculator"];

#[test]
fn field_labels_are_distinct_within_a_tool() {
    let mut fail = Failures::default();
    for t in all() {
        if KNOWN_DUPLICATE_LABELS.contains(&t.id) {
            continue;
        }
        let mut labels: Vec<&str> = t.fields.iter().map(|f| f.label).collect();
        let n = labels.len();
        labels.sort_unstable();
        labels.dedup();
        fail.check(labels.len() == n, || {
            format!(
                "{}: duplicate field labels in {:?}",
                t.id,
                t.fields.iter().map(|f| f.label).collect::<Vec<_>>()
            )
        });
    }
    // The exception list must not rot.
    for id in KNOWN_DUPLICATE_LABELS {
        assert!(find(id).is_some(), "unknown tool id `{id}`");
    }
    fail.assert_empty();
}

#[test]
fn no_tool_has_duplicate_field_labels() {
    let mut fail = Failures::default();
    for t in all() {
        let mut labels: Vec<&str> = t.fields.iter().map(|f| f.label).collect();
        let n = labels.len();
        labels.sort_unstable();
        labels.dedup();
        fail.check(labels.len() == n, || {
            format!(
                "{}: {:?}",
                t.id,
                t.fields.iter().map(|f| f.label).collect::<Vec<_>>()
            )
        });
    }
    fail.assert_empty();
}

/// Word pairs that mean "this select is really a direction toggle". Rule 1 of
/// the module header: an audit of all 86 it-tools tools found *none* of them is
/// modal, so a select offering both halves of one of these pairs is a
/// mistranslation of a multi-panel UI, and the tool should compute both panels.
const DIRECTION_PAIRS: &[(&str, &str)] = &[
    ("encode", "decode"),
    ("escape", "unescape"),
    ("encrypt", "decrypt"),
    ("compress", "decompress"),
    ("minify", "beautify"),
    ("minify", "prettify"),
    ("stringify", "parse"),
    ("serialize", "deserialize"),
    ("obfuscate", "deobfuscate"),
    ("hash", "verify"),
    ("generate", "validate"),
    ("to", "from"),
    ("forward", "reverse"),
    ("in", "out"),
];

/// Field keys that only ever exist to switch a tool between two behaviours.
const MODAL_KEYS: &[&str] = &["mode", "direction", "operation", "action"];

#[test]
fn no_tool_exposes_a_direction_toggle() {
    let mut fail = Failures::default();
    for t in all() {
        for f in t.fields {
            fail.check(!MODAL_KEYS.contains(&f.key), || {
                format!(
                    "{}.{}: `{}` is a modal switch — compute every panel instead \
                     (see lib.rs rule 1)",
                    t.id, f.key, f.key
                )
            });
            if f.kind != FieldKind::Select {
                continue;
            }
            let lower: Vec<String> = f.options.iter().map(|o| o.to_lowercase()).collect();
            for (a, b) in DIRECTION_PAIRS {
                let hit = |w: &str| lower.iter().any(|o| o == w);
                fail.check(!(hit(a) && hit(b)), || {
                    format!(
                        "{}.{}: options {:?} are the direction pair ({a}/{b}) — \
                         it-tools renders both panels at once, so no select",
                        t.id, f.key, f.options
                    )
                });
            }
        }
    }
    fail.assert_empty();
}

// ------------------------------------------------- what a tool does on open

/// `Kind::Pure` tools whose declared defaults are self-sufficient: opening the
/// tool must already show something. Pinned, because "it errors the moment you
/// open it" is exactly the `svg-placeholder-generator` bug.
#[rustfmt::skip]
const SELF_SUFFICIENT: &[&str] = &[
    "basic-auth-generator", "bcrypt", "bip39-generator", "chmod-calculator",
    "crontab-generator", "date-time-converter", "docker-run-to-docker-compose-converter",
    "encryption", "eta-calculator", "git-memo", "hash-text", "hmac-generator",
    "html-entities", "http-status-codes", "integer-base-converter",
    "ipv4-address-converter", "ipv4-range-expander", "ipv4-subnet-calculator",
    "ipv6-ula-generator", "lorem-ipsum-generator", "mac-address-generator",
    "mac-address-lookup", "meta-tag-generator", "mime-types",
    "password-strength-analyser", "percentage-calculator", "random-port-generator",
    "regex-memo", "roman-numeral-converter", "rsa-key-pair-generator",
    "svg-placeholder-generator", "temperature-converter", "text-statistics",
    "token-generator", "ulid-generator", "url-encoder", "url-parser",
    "uuid-generator",
];

/// Self-sufficient in the weaker sense: `Ok`, but with nothing to show until
/// the user types. Both are list/segment transforms of an empty list, so an
/// empty result is the honest answer rather than an error.
const OK_BUT_EMPTY: &[&str] = &["list-converter", "string-obfuscator"];

/// The rest: a tool that legitimately has nothing to do without input. It must
/// say so in a sentence a user can act on, never panic and never return `Ok`
/// with a misleading empty body.
#[rustfmt::skip]
const NEEDS_INPUT: &[&str] = &[
    "ascii-text-drawer", "base64-string-converter", "benchmark-builder",
    "case-converter", "color-converter", "email-normalizer",
    "iban-validator-and-parser", "json-diff", "json-minify", "json-to-csv",
    "json-to-toml", "json-to-xml", "json-to-yaml-converter", "json-viewer",
    "jwt-parser", "markdown-to-html", "math-evaluator", "numeronym-generator",
    "otp-code-generator-and-validator", "phone-parser-and-formatter",
    "qr-code-generator", "regex-tester", "safelink-decoder", "slugify-string",
    "sql-prettify", "text-diff", "text-to-binary", "text-to-nato-alphabet",
    "text-to-unicode", "toml-to-json", "toml-to-yaml", "user-agent-parser",
    "wifi-qr-code-generator", "xml-formatter", "xml-to-json",
    "yaml-to-json-converter", "yaml-to-toml", "yaml-viewer",
];

#[test]
fn every_pure_tool_is_classified_by_what_it_does_on_open() {
    let mut fail = Failures::default();
    let mut all_listed: Vec<&str> = SELF_SUFFICIENT
        .iter()
        .chain(OK_BUT_EMPTY)
        .chain(NEEDS_INPUT)
        .copied()
        .collect();
    let n = all_listed.len();
    all_listed.sort_unstable();
    all_listed.dedup();
    assert_eq!(n, all_listed.len(), "a tool is listed in two buckets");

    let mut pure: Vec<&str> = all()
        .iter()
        .filter(|t| t.kind == Kind::Pure)
        .map(|t| t.id)
        .collect();
    pure.sort_unstable();
    assert_eq!(
        pure, all_listed,
        "every Kind::Pure tool must be in exactly one bucket: decide whether a \
         new tool shows something on open, shows nothing, or asks for input"
    );

    for id in &pure {
        let t = find(id).expect("listed tool");
        // Empty args: the UI's very first call, before a keystroke. `arg()` falls
        // back to the declared defaults.
        match run(t, Args::new()) {
            Ok(out) if SELF_SUFFICIENT.contains(id) => {
                fail.check(!out.trim().is_empty(), || {
                    format!("{id}: listed self-sufficient but produced no output")
                });
            }
            Ok(out) if OK_BUT_EMPTY.contains(id) => {
                fail.check(out.trim().is_empty(), || {
                    format!("{id}: now produces output on open — move it to SELF_SUFFICIENT")
                });
            }
            Ok(_) => fail.push(format!(
                "{id}: listed in NEEDS_INPUT but is Ok on open — move it to SELF_SUFFICIENT"
            )),
            Err(e) if NEEDS_INPUT.contains(id) => {
                fail.check(e != "PANICKED", || format!("{id}: panicked on open"));
                // "no JSON input", "please set a secret (a base32 string)" — a
                // sentence, not a Rust type error.
                fail.check(e.len() >= 6 && !e.contains("unwrap"), || {
                    format!("{id}: unhelpful message on open: {e:?}")
                });
            }
            Err(e) => fail.push(format!(
                "{id}: errors on open ({e:?}) — that is the svg-placeholder-generator \
                 bug again; either fix the defaults or move it to NEEDS_INPUT"
            )),
        }
    }
    fail.assert_empty();
}

/// Nondeterministic by design; same list as `tool_props.rs`, duplicated because
/// two test binaries cannot share a constant without a new crate.
#[rustfmt::skip]
const NONDETERMINISTIC: &[&str] = &[
    "token-generator", "uuid-generator", "ulid-generator", "mac-address-generator",
    "random-port-generator", "lorem-ipsum-generator", "bip39-generator",
    "rsa-key-pair-generator", "encryption", "ipv6-ula-generator", "eta-calculator",
    "otp-code-generator-and-validator", "crontab-generator", "chronometer",
    "date-time-converter", "git-memo", "bcrypt",
];

/// Missing keys must behave exactly like the declared defaults. A tool reading
/// `args.get(..)` directly instead of `arg(args, tool, ..)` silently ignores its
/// own default, which the UI only reveals as a wrong first render.
#[test]
fn empty_args_behave_exactly_like_the_declared_defaults() {
    let mut fail = Failures::default();
    for t in all() {
        if t.kind != Kind::Pure || NONDETERMINISTIC.contains(&t.id) {
            continue;
        }
        let empty = run(t, Args::new());
        let declared = run(t, defaults(t));
        fail.check(empty == declared, || {
            format!(
                "{}: run(no args) != run(declared defaults):\n      {:?}\n      {:?}",
                t.id, empty, declared
            )
        });
    }
    fail.assert_empty();
}

// -------------------------------------------------------------- Kind::Custom

/// Tools that need bespoke UI. Pinned so that promoting one to a real panel is
/// a deliberate, visible change — and so that a half-finished tool cannot be
/// parked here quietly.
#[rustfmt::skip]
const CUSTOM: &[&str] = &[
    "base64-file-converter", "camera-recorder", "chronometer", "device-information",
    "emoji-picker", "html-wysiwyg-editor", "keycode-info", "pdf-signature-checker",
];

#[test]
fn custom_tools_are_pinned_and_refuse_to_run() {
    let mut listed: Vec<&str> = CUSTOM.to_vec();
    listed.sort_unstable();
    let mut actual: Vec<&str> = all()
        .iter()
        .filter(|t| t.kind == Kind::Custom)
        .map(|t| t.id)
        .collect();
    actual.sort_unstable();
    assert_eq!(
        actual, listed,
        "the Kind::Custom set changed: a tool gaining a real panel (or losing one) \
         must be a reviewed edit to CUSTOM"
    );

    let mut fail = Failures::default();
    for id in CUSTOM {
        let t = find(id).expect("custom tool");
        // Whatever the args, a Custom tool must refuse rather than half-work.
        for args in [Args::new(), defaults(t)] {
            match run(t, args) {
                Ok(out) => fail.push(format!("{id}: Kind::Custom must not run: {out:?}")),
                Err(e) => {
                    fail.check(e != "PANICKED", || format!("{id}: panicked instead of Err"));
                    fail.check(!e.trim().is_empty(), || {
                        format!("{id}: empty error message")
                    });
                    // The message is what the generic form shows in place of the
                    // panel, so it has to explain itself to a user.
                    let low = e.to_lowercase();
                    fail.check(
                        low.contains("panel") || low.contains("ui") || low.contains("needs"),
                        || format!("{id}: message does not say what is missing: {e:?}"),
                    );
                }
            }
        }
    }
    fail.assert_empty();
}

#[test]
fn custom_tool_messages_name_the_panel_they_need() {
    let mut fail = Failures::default();
    for id in CUSTOM {
        let t = find(id).expect("custom tool");
        let e = run(t, Args::new()).expect_err("Custom must not run");
        fail.check(e.len() > "needs bespoke UI".len(), || {
            format!("{id}: {e:?} is a placeholder, not an explanation")
        });
    }
    fail.assert_empty();
}

// ------------------------------------------------------ whitespace stability

/// Inputs where trailing whitespace and CRLF vs LF must not change the output:
/// documents in a structured format, plus the two tools that normalise a single
/// value. Deliberately not every tool — for `hash-text` or `text-statistics` a
/// trailing newline is part of the data and *must* change the answer.
#[rustfmt::skip]
const LINE_ENDING_INSENSITIVE: &[(&str, &str, &str)] = &[
    ("json-minify",            "input", "{\n  \"a\": 1,\n  \"b\": [1, 2]\n}"),
    ("json-viewer",            "input", "{\n  \"a\": 1,\n  \"b\": [1, 2]\n}"),
    ("json-to-csv",            "input", "[\n{\"a\":1},\n{\"a\":2}\n]"),
    ("json-to-toml",           "input", "{\n  \"a\": 1\n}"),
    ("json-to-xml",            "input", "{\n  \"a\": 1\n}"),
    ("json-to-yaml-converter", "input", "{\n  \"a\": 1\n}"),
    ("toml-to-json",           "input", "a = 1\nb = 2\n"),
    ("toml-to-yaml",           "input", "a = 1\nb = 2\n"),
    ("yaml-viewer",            "input", "a: 1\nb: 2\n"),
    ("yaml-to-json-converter", "input", "a: 1\nb: 2\n"),
    ("yaml-to-toml",           "input", "a: 1\nb: 2\n"),
    ("xml-formatter",          "input", "<a>\n<b>1</b>\n</a>"),
    ("xml-to-json",            "input", "<a>\n<b>1</b>\n</a>"),
    ("sql-prettify",           "input", "select a\nfrom t\nwhere a = 1"),
    ("markdown-to-html",       "input", "# Title\n\nA para\n"),
    ("slugify-string",         "input", "Hello World"),
    ("email-normalizer",       "emails", "A.B+x@Gmail.com"),
    ("math-evaluator",         "expression", "1 + 2 * 3"),
];

#[test]
fn line_endings_and_trailing_whitespace_do_not_change_the_output() {
    let mut fail = Failures::default();
    for (id, key, input) in LINE_ENDING_INSENSITIVE {
        let t = find(id).unwrap_or_else(|| panic!("unknown tool `{id}`"));
        assert!(
            t.fields.iter().any(|f| f.key == *key),
            "{id} has no field `{key}`"
        );
        let with = |v: String| {
            let mut args = defaults(t);
            args.insert((*key).to_string(), v);
            run(t, args)
        };
        let base = with((*input).to_string());
        fail.check(base.is_ok(), || {
            format!("{id}: the sample input itself failed: {base:?}")
        });
        let crlf = with(input.replace('\n', "\r\n"));
        fail.check(crlf == base, || {
            format!("{id}: CRLF changed the output:\n      {base:?}\n      {crlf:?}")
        });
        let trailing = with(format!("{input}  \t\n \n"));
        fail.check(trailing == base, || {
            format!(
                "{id}: trailing whitespace changed the output:\n      {base:?}\n      {trailing:?}"
            )
        });
    }
    fail.assert_empty();
}

// ------------------------------------------------------------------ ids

/// Every id, one per line, in registry order. Each is the it-tools route
/// segment the tool was ported from (`it-tools.tech/<id>`), which is why the
/// list is pinned by hand: a typo here means a user searching for the tool by
/// its it-tools name does not find it.
#[rustfmt::skip]
const IDS: &[&str] = &[
    "ascii-text-drawer", "base64-file-converter", "base64-string-converter",
    "basic-auth-generator", "bcrypt", "benchmark-builder", "bip39-generator",
    "camera-recorder", "case-converter", "chmod-calculator", "chronometer",
    "color-converter", "crontab-generator", "date-time-converter",
    "device-information", "docker-run-to-docker-compose-converter",
    "email-normalizer", "emoji-picker", "encryption", "eta-calculator",
    "git-memo", "hash-text", "hmac-generator", "html-entities",
    "html-wysiwyg-editor", "http-status-codes", "iban-validator-and-parser",
    "integer-base-converter", "ipv4-address-converter", "ipv4-range-expander",
    "ipv4-subnet-calculator", "ipv6-ula-generator", "json-diff", "json-minify",
    "json-to-csv", "json-to-toml", "json-to-xml", "json-to-yaml-converter",
    "json-viewer", "jwt-parser", "keycode-info", "list-converter",
    "lorem-ipsum-generator", "mac-address-generator", "mac-address-lookup",
    "markdown-to-html", "math-evaluator", "meta-tag-generator", "mime-types",
    "numeronym-generator", "otp-code-generator-and-validator",
    "password-strength-analyser", "pdf-signature-checker", "percentage-calculator",
    "phone-parser-and-formatter", "qr-code-generator", "random-port-generator",
    "regex-memo", "regex-tester", "roman-numeral-converter",
    "rsa-key-pair-generator", "safelink-decoder", "slugify-string",
    "sql-prettify", "string-obfuscator", "svg-placeholder-generator",
    "temperature-converter", "text-diff", "text-statistics", "text-to-binary",
    "text-to-nato-alphabet", "text-to-unicode", "toml-to-json", "toml-to-yaml",
    "token-generator", "ulid-generator", "url-encoder", "url-parser",
    "user-agent-parser", "uuid-generator", "wifi-qr-code-generator",
    "xml-formatter", "xml-to-json", "yaml-to-json-converter", "yaml-to-toml",
    "yaml-viewer",
];

#[test]
fn ids_are_kebab_case_and_pinned() {
    let mut fail = Failures::default();
    for t in all() {
        let id = t.id;
        fail.check(
            id.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            || format!("{id}: not kebab-case (lowercase ascii, digits and `-` only)"),
        );
        fail.check(
            !id.starts_with('-') && !id.ends_with('-') && !id.contains("--"),
            || format!("{id}: stray dashes"),
        );
        // An it-tools path segment is never a single word away from its name.
        fail.check(!id.is_empty(), || "empty id".to_string());
    }
    let mut actual: Vec<&str> = all().iter().map(|t| t.id).collect();
    actual.sort_unstable();
    let mut pinned: Vec<&str> = IDS.to_vec();
    pinned.sort_unstable();
    assert_eq!(pinned.len(), IDS.len(), "IDS itself has duplicates");
    assert_eq!(
        actual, pinned,
        "the tool id set changed: add or remove the it-tools route segment in IDS \
         deliberately (86 tools were audited against it-tools' UI)"
    );
    fail.assert_empty();
}

/// Copying a value must not copy its label. `section` puts the label *in* the
/// text, so the UI splits it back out with `split_sections`.
///
/// The invariant is coverage, not reassembly: every byte of a tool's output must
/// land inside exactly one section, in order. That is what makes a mistaken
/// header harmless — the text is still all there — and it is what caught the
/// first version of the parser silently dropping everything before the first
/// header on `crontab-generator`, `url-parser` and `regex-memo`.
#[test]
fn split_sections_loses_nothing_for_every_tool() {
    let mut broken = Vec::new();
    for tool in rp_devtools::all() {
        if tool.kind == rp_devtools::Kind::Custom {
            continue;
        }
        let Ok(output) = (tool.run)(&rp_devtools::Args::new()) else {
            continue;
        };
        let parts = rp_devtools::split_sections(&output);
        assert!(!parts.is_empty(), "{}: split produced nothing", tool.id);

        // Walk the slices and require them to tile the input exactly: each
        // value must start where the previous section ended, allowing only the
        // header line the parser consumed in between.
        let base = output.as_ptr() as usize;
        let mut cursor = 0usize;
        for (label, value) in &parts {
            let start = value.as_ptr() as usize - base;
            let header = if label.is_empty() { 0 } else { label.len() + 2 };
            if start != cursor + header {
                broken.push(format!(
                    "{}: gap before section {label:?} — expected {} got {start}",
                    tool.id,
                    cursor + header
                ));
            }
            cursor = start + value.len();
        }
        if cursor != output.len() {
            broken.push(format!(
                "{}: {} byte(s) past the last section were dropped",
                tool.id,
                output.len() - cursor
            ));
        }
    }
    assert!(
        broken.is_empty(),
        "{} problem(s):
{}",
        broken.len(),
        broken.join(
            "
"
        )
    );
}

/// The reported bug, pinned: pressing Copy on the base64 tool handed back
/// "Base64 of string:\nSGVsbG8=" instead of the value.
#[test]
fn a_labelled_value_yields_the_value_alone() {
    let out = rp_devtools::section("Base64 of string", Ok("SGVsbG8=".into()));
    let parts = rp_devtools::split_sections(&out);
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].0, "Base64 of string");
    // The parser cuts but never trims, so the caller trims — which is what the
    // copy button does. What matters is that the label is gone from the value.
    assert_eq!(parts[0].1.trim_end(), "SGVsbG8=");

    // Several panels, the shape every multi-panel tool produces.
    let joined = [
        rp_devtools::section("Base64 of string", Ok("SGVsbG8=".into())),
        rp_devtools::section("String from base64", Ok("Hello".into())),
    ]
    .join("\n\n");
    let parts = rp_devtools::split_sections(&joined);
    let seen: Vec<(&str, String)> = parts
        .iter()
        .map(|(l, v)| (*l, v.trim_end().to_string()))
        .collect();
    assert_eq!(
        seen,
        [
            ("Base64 of string", "SGVsbG8=".to_string()),
            ("String from base64", "Hello".to_string())
        ]
    );

    // A multi-line value keeps its interior newlines but not the separator.
    let multi = rp_devtools::section("Rows", Ok("a\nb\nc".into()));
    assert_eq!(rp_devtools::split_sections(&multi), [("Rows", "a\nb\nc")]);

    // Unlabelled output is one section with an empty label, so a single-value
    // tool needs no special case at the call site.
    assert_eq!(
        rp_devtools::split_sections("just a value"),
        [("", "just a value")]
    );
    assert_eq!(rp_devtools::split_sections(""), [("", "")]);

    // A body line ending in ':' that does *not* follow a blank line is content.
    let tricky = rp_devtools::section("Head", Ok("nested:\nvalue".into()));
    assert_eq!(
        rp_devtools::split_sections(&tricky),
        [("Head", "nested:\nvalue")]
    );
}
