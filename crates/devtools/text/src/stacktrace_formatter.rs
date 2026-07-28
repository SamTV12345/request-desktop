//! `stacktrace-formatter` — it-tools PR #1307.
//!
//! A stack trace almost never arrives as a stack trace. It arrives as one line
//! of a JSON log with `\n` spelled out, or wrapped by a terminal, and the job
//! is to get it back to something readable.

use crate::*;
use anyhow::{Result, bail};
use std::fmt::Write as _;

/// Undo the escaping a log pipeline added: the two-character spellings of
/// newline, tab, quote and backslash, plus `\uXXXX`.
pub(crate) fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('u') => {
                let hex: String = chars.clone().take(4).collect();
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(ch) => {
                        for _ in 0..4 {
                            chars.next();
                        }
                        out.push(ch);
                    }
                    None => out.push_str("\\u"),
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Is this line a frame rather than a message? Java/Kotlin `at x.y(F.java:1)`,
/// JS `at fn (file:1:2)`, Rust `  12: core::…`, Python `  File "x", line 1`,
/// Go `\tmain.go:17 +0x1d`.
pub(crate) fn is_frame(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("at ")
        || t.starts_with("File \"")
        || t.starts_with("from ")
        || t.starts_with("#")
        || t.split_once(':')
            .is_some_and(|(n, _)| !n.is_empty() && n.trim().chars().all(|c| c.is_ascii_digit()))
}

/// A new exception starts here: `Caused by:`, `Suppressed:`, Python's chaining
/// sentences, or a bare `com.foo.Bar: message` after a run of frames.
fn is_cause(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("Caused by")
        || t.starts_with("Suppressed")
        || t.starts_with("The above exception")
        || t.starts_with("During handling")
        || t.starts_with("Traceback (")
}

fn stacktrace(a: &Args) -> Result<String> {
    let raw = s(a, "input", "");
    if raw.trim().is_empty() {
        bail!("paste a stack trace — escaped, wrapped or straight from the log");
    }
    let text = unescape(raw);
    // A trace mangled onto one line still has its frames marked by " at ".
    let text = if text.lines().count() == 1 && text.matches(" at ").count() > 1 {
        text.replace(" at ", "\n at ")
    } else {
        text
    };

    let (mut frames, mut causes) = (0usize, 0usize);
    let mut out = String::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        if is_cause(line) {
            causes += 1;
            let _ = writeln!(out, "\n{}", line.trim());
        } else if is_frame(line) {
            frames += 1;
            let _ = writeln!(out, "    {}", line.trim());
        } else {
            let _ = writeln!(out, "{}", line.trim());
        }
    }
    Ok([
        section("Formatted", Ok(out)),
        section(
            "Shape",
            Ok(format!("{frames} frame(s), {causes} chained cause(s)\n")),
        ),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "stacktrace-formatter",
    name: "Stack trace formatter",
    description: "Unescape and re-indent a stack trace that arrived as one line of JSON log.",
    category: Category::Development,
    keywords: &[
        "stacktrace",
        "exception",
        "log",
        "unescape",
        "java",
        "format",
        "trace",
    ],
    fields: &[Field::area("input", "Stack trace")],
    kind: Kind::Pure,
    run: stacktrace,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn a_json_log_line_becomes_a_trace_again() {
        let logged = r#"java.lang.IllegalStateException: boom\n\tat com.foo.Bar.run(Bar.java:42)\n\tat com.foo.Bar.main(Bar.java:11)\nCaused by: java.io.IOException: disk\n\tat java.base/java.io.File.read(File.java:9)"#;
        let out = ok("stacktrace-formatter", &[("input", logged)]);
        assert!(
            out.contains("java.lang.IllegalStateException: boom"),
            "{out}"
        );
        assert!(out.contains("    at com.foo.Bar.run(Bar.java:42)"), "{out}");
        assert!(
            out.contains("Caused by: java.io.IOException: disk"),
            "{out}"
        );
        assert!(out.contains("3 frame(s), 1 chained cause(s)"), "{out}");
        // `\n` really became a newline rather than staying two characters.
        assert!(!out.contains("\\n"), "{out}");
    }

    #[test]
    fn other_runtimes_and_the_one_line_case() {
        // Python, with its own frame syntax and chaining sentence.
        let py = "Traceback (most recent call last):\n  File \"a.py\", line 3, in <module>\n    main()\nValueError: nope";
        let out = ok("stacktrace-formatter", &[("input", py)]);
        assert!(
            out.contains("    File \"a.py\", line 3, in <module>"),
            "{out}"
        );
        assert!(out.contains("ValueError: nope"), "{out}");
        // A trace flattened onto one line by a console still splits.
        let flat = "Error: x at a (f.js:1:2) at b (f.js:3:4) at c (f.js:5:6)";
        let js = ok("stacktrace-formatter", &[("input", flat)]);
        assert!(js.contains("3 frame(s)"), "{js}");
        assert!(js.contains("    at b (f.js:3:4)"), "{js}");
        // The two characters backslash-t are a tab in disguise.
        assert!(ok("stacktrace-formatter", &[("input", "a\\tb")]).contains("a\tb"));
        assert!(run("stacktrace-formatter", &[]).is_err());
    }
}
