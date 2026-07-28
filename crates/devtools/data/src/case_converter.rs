//! `case-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Result, bail};

/// Split on any non-alphanumeric run plus camelCase / acronym boundaries.
/// ponytail: `change-case`'s `stripRegexp` also treats digits as separators;
/// keeping them inside words is the less surprising behaviour here.
pub(crate) fn words(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        let prev = if cur.is_empty() {
            None
        } else {
            chars.get(i - 1).copied()
        };
        let next = chars.get(i + 1).copied();
        let boundary = match prev {
            Some(p) if c.is_uppercase() && !p.is_uppercase() => true,
            // HTTPServer -> HTTP | Server
            Some(p) if p.is_uppercase() && c.is_uppercase() => {
                next.is_some_and(|n| n.is_lowercase())
            }
            _ => false,
        };
        if boundary && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub(crate) fn capitalize(w: &str) -> String {
    let mut cs = w.chars();
    match cs.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + &cs.as_str().to_lowercase(),
    }
}

/// Unknown keys are impossible: the only caller iterates [`CASES`], because
/// it-tools shows every case at once instead of offering a picker.
pub(crate) fn convert_case(input: &str, case: &str) -> String {
    let ws = words(input);
    let lower: Vec<String> = ws.iter().map(|w| w.to_lowercase()).collect();
    match case {
        "lower" => input.to_lowercase(),
        "upper" => input.to_uppercase(),
        "mock" => input
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if i % 2 == 0 {
                    c.to_uppercase().to_string()
                } else {
                    c.to_lowercase().to_string()
                }
            })
            .collect(),
        "camel" => lower
            .iter()
            .enumerate()
            .map(|(i, w)| if i == 0 { w.clone() } else { capitalize(w) })
            .collect(),
        "pascal" => lower.iter().map(|w| capitalize(w)).collect(),
        "snake" => lower.join("_"),
        "constant" => lower.join("_").to_uppercase(),
        "kebab" => lower.join("-"),
        // change-case's `noCase`: words, lowercased, space separated.
        "no" => lower.join(" "),
        "header" => lower
            .iter()
            .map(|w| capitalize(w))
            .collect::<Vec<_>>()
            .join("-"),
        "dot" => lower.join("."),
        "path" => lower.join("/"),
        "capital" => lower
            .iter()
            .map(|w| capitalize(w))
            .collect::<Vec<_>>()
            .join(" "),
        "sentence" => lower
            .iter()
            .enumerate()
            .map(|(i, w)| if i == 0 { capitalize(w) } else { w.clone() })
            .collect::<Vec<_>>()
            .join(" "),
        other => unreachable!("unknown case {other:?}"),
    }
}

pub(crate) fn case_converter(args: &Args) -> Result<String> {
    let t = &TOOLS[19];
    let input = arg(args, t, "input");
    if input.is_empty() {
        bail!("no input");
    }
    // One line per case, like `hash-text`: nothing here can fail, so no
    // `section` wrapping is needed.
    Ok(CASES
        .iter()
        .map(|(label, case)| format!("{label}: {}", convert_case(input, case)))
        .collect::<Vec<_>>()
        .join("\n"))
}

pub static TOOL: Tool = Tool {
    id: "case-converter",
    name: "Case converter",
    description: "Show a string in camel, snake, kebab and every other case at once.",
    category: Category::Text,
    keywords: &["case", "camel", "snake", "kebab", "pascal", "convert"],
    fields: &[Field::area("input", "Your string")],
    kind: Kind::Pure,
    run: case_converter,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;
    #[test]
    fn list_converter_sorts_reverses_and_lowercases() {
        let input = "b\nA\nc";
        assert_eq!(
            ok(
                "list-converter",
                &[
                    ("input", input),
                    ("separator", ","),
                    ("sortList", crate::list_converter::SORT_ASC),
                    ("lowerCase", "true")
                ]
            ),
            "a,b,c"
        );
        assert_eq!(
            ok(
                "list-converter",
                &[
                    ("input", input),
                    ("separator", ","),
                    ("sortList", crate::list_converter::SORT_DESC),
                    ("lowerCase", "true")
                ]
            ),
            "c,b,a"
        );
        assert_eq!(
            ok(
                "list-converter",
                &[
                    ("input", input),
                    ("separator", ","),
                    ("reverseList", "true")
                ]
            ),
            "c,A,b"
        );
    }

    // ── markdown / sql ──────────────────────────────────────────────────
    #[test]
    fn case_converter_splits_camel_and_acronyms() {
        let out = ok("case-converter", &[("input", "fooBarBaz")]);
        assert!(out.contains("Snakecase: foo_bar_baz"), "{out}");
        let out = ok("case-converter", &[("input", "HTTPServer")]);
        assert!(out.contains("Paramcase: http-server"), "{out}");
        let out = ok("case-converter", &[("input", "foo-bar_baz qux")]);
        assert!(out.contains("Camelcase: fooBarBazQux"), "{out}");
        assert!(run("case-converter", &[("input", "")]).is_err());
    }

    // ── text-to-binary (vectors from text-to-binary.models.test.ts) ─────
}
