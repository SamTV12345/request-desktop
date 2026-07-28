//! `smart-text-replacer` — it-tools PRs #1290 / #976.
//!
//! Find and replace with a regex, plus the line-break handling that is the
//! other half of the upstream tool: a file that came from the wrong platform.

use crate::*;
use anyhow::{Context, Result, bail};

/// Normalise to one line-ending flavour. `Keep` leaves mixed endings mixed,
/// which is what you want while you are still hunting for the odd one out.
fn line_breaks(text: &str, style: &str) -> Result<String> {
    // Fold to LF first, so a CRLF file does not become CRCRLF.
    let lf = text.replace("\r\n", "\n").replace('\r', "\n");
    Ok(match style {
        "Keep" => text.to_string(),
        "LF (Unix)" => lf,
        "CRLF (Windows)" => lf.replace('\n', "\r\n"),
        "CR (classic Mac)" => lf.replace('\n', "\r"),
        "Join lines" => lf.replace('\n', " "),
        "Strip blank lines" => lf
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        other => bail!("unknown line break handling: {other}"),
    })
}

fn replace(a: &Args) -> Result<String> {
    let input = s(a, "input", "");
    if input.is_empty() {
        bail!("paste the text to work on");
    }
    let find = s(a, "find", "");
    let with = s(a, "replace", "");
    let mut replacements = 0usize;

    let replaced = if find.is_empty() {
        input.to_string()
    } else if b(a, "regex", false) {
        let pattern = if b(a, "caseSensitive", true) {
            find.to_string()
        } else {
            format!("(?i){find}")
        };
        let re = regex::Regex::new(&pattern).context("that is not a valid regular expression")?;
        replacements = re.find_iter(input).count();
        // `$1` in the replacement is the usual capture reference.
        re.replace_all(input, with).into_owned()
    } else if b(a, "caseSensitive", true) {
        replacements = input.matches(find).count();
        input.replace(find, with)
    } else {
        // Case-insensitive plain text: walk the lowered haystack, cut the real
        // one, so the untouched parts keep their original casing.
        let (hay, needle) = (input.to_lowercase(), find.to_lowercase());
        let mut out = String::with_capacity(input.len());
        let mut at = 0;
        while let Some(hit) = hay[at..].find(&needle) {
            let start = at + hit;
            out.push_str(&input[at..start]);
            out.push_str(with);
            at = start + needle.len();
            replacements += 1;
        }
        out.push_str(&input[at..]);
        out
    };

    let text = line_breaks(&replaced, s(a, "lineBreaks", "Keep"))?;
    Ok([
        section("Result", Ok(text.clone())),
        section(
            "Changed",
            Ok(format!(
                "{replacements} replacement(s)\n{} line(s), {} character(s)\n",
                text.lines().count(),
                text.chars().count()
            )),
        ),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "smart-text-replacer",
    name: "Smart text replacer",
    description: "Find and replace with or without a regex, and fix the line endings while you are there.",
    category: Category::Text,
    keywords: &[
        "replace",
        "find",
        "regex",
        "substitute",
        "crlf",
        "line",
        "endings",
        "sed",
    ],
    fields: &[
        Field::area("input", "Text"),
        Field::text("find", "Find"),
        Field::text("replace", "Replace with"),
        Field::bool("regex", "Treat as a regular expression", "false"),
        Field::bool("caseSensitive", "Case sensitive", "true"),
        Field::select(
            "lineBreaks",
            "Line breaks",
            &[
                "Keep",
                "LF (Unix)",
                "CRLF (Windows)",
                "CR (classic Mac)",
                "Join lines",
                "Strip blank lines",
            ],
            "Keep",
        ),
    ],
    kind: Kind::Pure,
    run: replace,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn plain_regex_and_case_folding() {
        let out = ok(
            "smart-text-replacer",
            &[
                ("input", "Cat cat CAT"),
                ("find", "cat"),
                ("replace", "dog"),
            ],
        );
        assert!(out.contains("Cat dog CAT"), "{out}");
        assert!(out.contains("1 replacement(s)"), "{out}");
        // Case-insensitive replaces all three but keeps the rest as typed.
        let ci = ok(
            "smart-text-replacer",
            &[
                ("input", "Cat cat CAT!"),
                ("find", "cat"),
                ("replace", "dog"),
                ("caseSensitive", "false"),
            ],
        );
        assert!(ci.contains("dog dog dog!"), "{ci}");
        assert!(ci.contains("3 replacement(s)"), "{ci}");
        // A regex with a capture group.
        let re = ok(
            "smart-text-replacer",
            &[
                ("input", "id=17 id=23"),
                ("find", r"id=(\d+)"),
                ("replace", "#$1"),
                ("regex", "true"),
            ],
        );
        assert!(re.contains("#17 #23"), "{re}");
        assert!(
            run(
                "smart-text-replacer",
                &[("input", "x"), ("find", "("), ("regex", "true")]
            )
            .is_err()
        );
    }

    #[test]
    fn line_endings_are_normalised_not_doubled() {
        let crlf = ok(
            "smart-text-replacer",
            &[("input", "a\r\nb\nc"), ("lineBreaks", "CRLF (Windows)")],
        );
        assert!(crlf.contains("a\r\nb\r\nc"), "{crlf:?}");
        assert!(
            !crlf.contains("\r\r"),
            "no doubled carriage returns: {crlf:?}"
        );
        let lf = ok(
            "smart-text-replacer",
            &[("input", "a\r\nb"), ("lineBreaks", "LF (Unix)")],
        );
        assert!(lf.contains("a\nb") && !lf.contains('\r'), "{lf:?}");
        let joined = ok(
            "smart-text-replacer",
            &[("input", "a\nb\nc"), ("lineBreaks", "Join lines")],
        );
        assert!(joined.contains("a b c"), "{joined}");
        let stripped = ok(
            "smart-text-replacer",
            &[("input", "a\n\n\nb"), ("lineBreaks", "Strip blank lines")],
        );
        assert!(stripped.contains("a\nb"), "{stripped:?}");
        assert!(run("smart-text-replacer", &[]).is_err());
    }
}
