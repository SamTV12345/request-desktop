//! `markdown-toc-generator` — it-tools PR #1106.

use crate::*;
use anyhow::{Result, bail};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use std::fmt::Write as _;

/// GitHub's anchor rules: lower-case, punctuation dropped, spaces to hyphens,
/// and a `-1`, `-2` … suffix for a heading that repeats.
pub(crate) fn slug(text: &str, seen: &mut Vec<String>) -> String {
    let base: String = text
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            'a'..='z' | '0'..='9' | '_' => Some(c),
            ' ' | '-' => Some('-'),
            c if c.is_alphanumeric() => Some(c), // keeps ä, é, 日
            _ => None,
        })
        .collect();
    let count = seen.iter().filter(|s| **s == base).count();
    seen.push(base.clone());
    if count == 0 {
        base
    } else {
        format!("{base}-{count}")
    }
}

/// `(level, text)` for every ATX/setext heading, in document order. Headings
/// inside fenced code blocks are not headings, which is why this goes through
/// the parser rather than a line scan.
pub(crate) fn headings(markdown: &str) -> Vec<(u8, String)> {
    let mut out = Vec::new();
    let mut current: Option<(u8, String)> = None;
    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let n = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                current = Some((n, String::new()));
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, text)) = current.as_mut() {
                    text.push_str(&t);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, text)) = current.take()
                    && !text.trim().is_empty()
                {
                    out.push((level, text.trim().to_string()));
                }
            }
            _ => {}
        }
    }
    out
}

fn toc(a: &Args) -> Result<String> {
    let input = s(a, "input", "");
    if input.trim().is_empty() {
        bail!("no markdown input");
    }
    let (min, max) = (n(a, "minLevel", 2), n(a, "maxLevel", 4));
    if !(1..=6).contains(&min) || !(1..=6).contains(&max) || min > max {
        bail!("levels must be between 1 and 6, and the minimum not above the maximum");
    }
    let all = headings(input);
    let mut seen = Vec::new();
    let mut out = String::new();
    let mut listed = 0usize;
    for (level, text) in &all {
        // Every heading takes an anchor, including the ones outside the range:
        // GitHub's duplicate counter does not skip them either.
        let anchor = slug(text, &mut seen);
        if i64::from(*level) < min || i64::from(*level) > max {
            continue;
        }
        let indent = "  ".repeat((i64::from(*level) - min) as usize);
        let _ = writeln!(out, "{indent}- [{text}](#{anchor})");
        listed += 1;
    }
    if listed == 0 {
        bail!(
            "no headings between level {min} and {max} ({} heading(s) in the document)",
            all.len()
        );
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "markdown-toc-generator",
    name: "Markdown table of contents",
    description: "A linked table of contents from the headings, with GitHub's anchor rules.",
    category: Category::Converter,
    keywords: &[
        "markdown", "toc", "table", "contents", "anchor", "readme", "headings",
    ],
    fields: &[
        Field::area("input", "Markdown"),
        Field::number("minLevel", "From heading level", "2"),
        Field::number("maxLevel", "To heading level", "4"),
    ],
    kind: Kind::Pure,
    run: toc,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    const DOC: &str = "# Title\n\n## Getting started\n\nx\n\n### Install & run\n\n```\n## not a heading\n```\n\n## Getting started\n\n#### Deep\n\n##### Deeper";

    #[test]
    fn anchors_follow_the_github_rules() {
        let out = ok("markdown-toc-generator", &[("input", DOC)]);
        assert!(
            out.contains("- [Getting started](#getting-started)"),
            "{out}"
        );
        // The `&` is dropped, the spaces become hyphens.
        assert!(out.contains("  - [Install & run](#install--run)"), "{out}");
        // A repeated heading gets GitHub's counter.
        assert!(
            out.contains("- [Getting started](#getting-started-1)"),
            "{out}"
        );
        // A `##` inside a fence is not a heading.
        assert!(!out.contains("not a heading"), "{out}");
        // Default range is 2..=4, so H1 and H5 stay out.
        assert!(
            !out.contains("[Title]") && !out.contains("[Deeper]"),
            "{out}"
        );
        assert!(out.contains("    - [Deep](#deep)"), "{out}");
    }

    #[test]
    fn the_level_range_is_the_users_call() {
        let all = ok(
            "markdown-toc-generator",
            &[("input", DOC), ("minLevel", "1"), ("maxLevel", "6")],
        );
        assert!(all.contains("- [Title](#title)"), "{all}");
        assert!(all.contains("        - [Deeper](#deeper)"), "{all}");
        assert!(
            run(
                "markdown-toc-generator",
                &[("input", "# A"), ("minLevel", "2")]
            )
            .is_err()
        );
        assert!(
            run(
                "markdown-toc-generator",
                &[("input", DOC), ("minLevel", "5"), ("maxLevel", "2")]
            )
            .is_err()
        );
        assert!(run("markdown-toc-generator", &[]).is_err());
        // Unicode headings keep their letters.
        let de = ok("markdown-toc-generator", &[("input", "## Größe & Maß")]);
        assert!(de.contains("(#größe--maß)"), "{de}");
    }
}
