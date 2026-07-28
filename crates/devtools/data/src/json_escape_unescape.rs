//! `json-escape-unescape` — it-tools PR #966.
//!
//! Turning a log line into a JSON string literal and reading one back are the
//! two halves of the same chore, and which half you want changes mid-task. So
//! both are always on screen, each with its own input, the way the base64 tool
//! does it — a `mode` select would only ever be in the wrong position.
//!
//! `serde_json` owns both directions: it already escapes exactly what the
//! grammar requires (`"`, `\`, and control characters as `\u00XX`) and already
//! rejects a half-written literal. Hand-rolling the escape table is how you get
//! a lone surrogate wrong.

use crate::*;
use anyhow::{Context, Result, bail};

/// Both fields ship pre-filled so the tool shows its two panels the moment it
/// is opened. `empty_args_behave_exactly_like_the_declared_defaults` requires
/// these to be the same constants the `Field`s declare.
const PLAIN: &str = "He said \"hello\".\nThen left\ta note.";
const ESCAPED: &str = r#""Line 1\nLine 2\t❤ \"quoted\"""#;

/// Read a JSON string literal. The surrounding quotes are optional, because
/// what people paste is usually the *inside* of one, copied out of a log.
fn unescape(raw: &str) -> Result<String> {
    let t = raw.trim();
    let quoted = if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t.to_string()
    } else {
        format!("\"{t}\"")
    };
    serde_json::from_str::<String>(&quoted).context("not a valid JSON string literal")
}

fn escape_unescape(a: &Args) -> Result<String> {
    let plain = s(a, "input", PLAIN);
    let escaped = s(a, "escaped", ESCAPED);
    let mut out = Vec::new();
    if !plain.is_empty() {
        out.push(section(
            "Escaped JSON string",
            serde_json::to_string(plain).context("cannot represent this text as JSON"),
        ));
    }
    if !escaped.trim().is_empty() {
        out.push(section("Unescaped text", unescape(escaped)));
    }
    if out.is_empty() {
        bail!(
            "type some text on the left to escape it, or paste a JSON string literal on the right to read it back"
        );
    }
    Ok(out.join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "json-escape-unescape",
    name: "JSON escape / unescape",
    description: "Turn text into a JSON string literal, and read an escaped literal back as text.",
    category: Category::Converter,
    keywords: &[
        "json",
        "escape",
        "unescape",
        "string",
        "literal",
        "quote",
        "backslash",
    ],
    fields: &[
        Field::area("input", "Text to escape").with_default(PLAIN),
        Field::area("escaped", "JSON string to unescape").with_default(ESCAPED),
    ],
    kind: Kind::Pure,
    run: escape_unescape,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn both_panels_are_shown_at_once() {
        let out = ok("json-escape-unescape", &[]);
        assert!(out.contains("Escaped JSON string:\n\"He said"), "{out}");
        assert!(out.contains("Unescaped text:\nLine 1\nLine 2"), "{out}");
    }

    #[test]
    fn escaping_covers_the_whole_grammar() {
        assert_eq!(
            ok(
                "json-escape-unescape",
                &[("input", "a\"b\\c\nd\te\u{1}f"), ("escaped", "")]
            ),
            "Escaped JSON string:\n\"a\\\"b\\\\c\\nd\\te\\u0001f\""
        );
        // Printable non-ASCII stays as itself; only control characters need \u.
        assert_eq!(
            ok(
                "json-escape-unescape",
                &[("input", "héllo ❤"), ("escaped", "")]
            ),
            "Escaped JSON string:\n\"héllo ❤\""
        );
    }

    #[test]
    fn unescaping_takes_a_literal_with_or_without_quotes() {
        for input in [r#""a\nb❤""#, r#"a\nb❤"#] {
            assert_eq!(
                ok("json-escape-unescape", &[("input", ""), ("escaped", input)]),
                "Unescaped text:\na\nb❤"
            );
        }
        // Round trip, both ways.
        let text = "quote \" backslash \\ tab \t";
        let esc = ok("json-escape-unescape", &[("input", text), ("escaped", "")]);
        let esc = esc.trim_start_matches("Escaped JSON string:\n");
        assert_eq!(
            ok("json-escape-unescape", &[("input", ""), ("escaped", esc)]),
            format!("Unescaped text:\n{text}")
        );
    }

    #[test]
    fn one_bad_panel_keeps_the_other() {
        let out = ok(
            "json-escape-unescape",
            &[("input", "fine"), ("escaped", r#"a\qb"#)],
        );
        assert!(out.contains("Escaped JSON string:\n\"fine\""), "{out}");
        assert!(out.contains("not a valid JSON string literal"), "{out}");
        // A dangling escape and a raw newline are both refused, not guessed at.
        for bad in [r#"a\"#, "a\nb", r#""unterminated"#] {
            let out = ok("json-escape-unescape", &[("input", ""), ("escaped", bad)]);
            assert!(out.contains("not a valid JSON"), "{bad:?}: {out}");
        }
        // Nothing typed anywhere is the only hard error.
        assert!(run("json-escape-unescape", &[("input", ""), ("escaped", "")]).is_err());
    }

    #[test]
    fn survives_hostile_input() {
        let mb = "\"".repeat(1_000_000);
        let out = ok("json-escape-unescape", &[("input", &mb), ("escaped", "")]);
        assert_eq!(out.len(), "Escaped JSON string:\n".len() + 2_000_002);
        let braces = "{".repeat(10_000);
        assert!(
            ok(
                "json-escape-unescape",
                &[("input", &braces), ("escaped", "")]
            )
            .contains('{')
        );
        // An escaped megabyte comes back as itself.
        let long = format!("\\n{}", "\\t".repeat(500_000));
        let back = ok("json-escape-unescape", &[("input", ""), ("escaped", &long)]);
        assert_eq!(back.len(), "Unescaped text:\n".len() + 500_001);
    }
}
