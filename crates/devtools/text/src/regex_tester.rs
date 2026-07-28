//! `regex-tester`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

use regex::Regex;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Capture {
    pub(crate) name: String,
    /// `(value, start, end)`, or `None` for a group that did not take part in
    /// this match. `(a)?(b)` against "b" has no group 1, and that is *not* the
    /// same as a group 1 that matched the empty string — the port used to drop
    /// the row entirely, so a pattern with an optional group showed a numbering
    /// with holes in it and named groups that silently disappeared.
    pub(crate) at: Option<(String, usize, usize)>,
}

impl Capture {
    pub(crate) fn hit(name: impl Into<String>, m: regex::Match<'_>) -> Self {
        Self {
            name: name.into(),
            at: Some((m.as_str().to_string(), m.start(), m.end())),
        }
    }
    pub(crate) fn absent(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            at: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RegexMatch {
    pub(crate) index: usize,
    pub(crate) value: String,
    pub(crate) captures: Vec<Capture>,
    pub(crate) groups: Vec<Capture>,
}

/// ponytail: offsets are UTF-8 byte offsets, it-tools reports UTF-16 units.
/// Identical for ASCII; revisit if the UI highlights non-ASCII matches.
pub(crate) fn regex_matches(
    pattern: &str,
    text: &str,
    flags: &str,
) -> anyhow::Result<Vec<RegexMatch>> {
    // `g`/`d`/`y`/`u` are JS-only concerns: the Rust engine is always global,
    // always reports indices and is always Unicode-aware.
    let inline: String = flags.chars().filter(|c| "imsxU".contains(*c)).collect();
    let full = if inline.is_empty() {
        pattern.to_string()
    } else {
        format!("(?{inline}){pattern}")
    };
    let re = Regex::new(&full).map_err(|e| anyhow::anyhow!("invalid regex: {e}"))?;

    let names: Vec<Option<String>> = re.capture_names().map(|n| n.map(str::to_string)).collect();
    // Without JS's `g` flag `exec` never advances, so it-tools reports one match.
    let global = flags.contains('g');
    let mut out = Vec::new();
    for caps in re.captures_iter(text) {
        let whole = caps.get(0).expect("group 0 always matches");
        if whole.as_str().is_empty() {
            break; // it-tools bails on zero-length matches to avoid looping
        }
        let mut captures = Vec::new();
        let mut groups = Vec::new();
        // Every declared group gets a row, participating or not: the row is the
        // only place the user can read off which group number they are looking
        // at, so skipping the absent ones is what shifted the output.
        for (i, name) in names.iter().enumerate().skip(1) {
            let m = caps.get(i);
            captures.push(match m {
                Some(m) => Capture::hit(i.to_string(), m),
                None => Capture::absent(i.to_string()),
            });
            if let Some(n) = name {
                groups.push(match m {
                    Some(m) => Capture::hit(n.clone(), m),
                    None => Capture::absent(n.clone()),
                });
            }
        }
        out.push(RegexMatch {
            index: whole.start(),
            value: whole.as_str().to_string(),
            captures,
            groups,
        });
        if !global {
            break;
        }
    }
    Ok(out)
}

pub(crate) fn format_matches(matches: Vec<RegexMatch>) -> String {
    if matches.is_empty() {
        return "No match.".into();
    }
    let show = |kind: &str, c: &Capture| match &c.at {
        Some((value, start, end)) => {
            format!("  {kind} {}: {value:?} [{start}..{end}]\n", c.name)
        }
        None => format!("  {kind} {}: (did not participate)\n", c.name),
    };
    let mut out = format!("{} match(es)\n", matches.len());
    for m in &matches {
        out.push_str(&format!("\n@{} {:?}\n", m.index, m.value));
        for c in &m.captures {
            out.push_str(&show("group", c));
        }
        for g in &m.groups {
            out.push_str(&show("named", g));
        }
    }
    out
}

/// it-tools exposes the flags as six checkboxes — `g`/`i`/`m`/`s`/`u`/`v`, with
/// `g`, `s` and `u` on by default — and swallows a compile error into an empty
/// match list, leaving the pattern field's own validation to report it. So the
/// invalid pattern is reported inside the Matches panel, not as a dead output.
///
/// `u` and `v` are not offered: the Rust engine is always Unicode-aware and has
/// no `v` mode. Upstream also renders a RandExp "sample matching text" and a
/// railroad diagram of the pattern; neither has a dependency-free equivalent.
pub(crate) fn regex_tester(args: &Args) -> anyhow::Result<String> {
    let id = "regex-tester";
    let pattern = nonempty("regex", a(args, id, "regex"))?;
    let mut flags = String::new();
    for (key, ch) in [
        ("global", 'g'),
        ("ignore_case", 'i'),
        ("multiline", 'm'),
        ("dot_all", 's'),
    ] {
        if truthy(a(args, id, key)) {
            flags.push(ch);
        }
    }
    Ok(crate::section(
        "Matches",
        regex_matches(pattern, a(args, id, "text"), &flags).map(format_matches),
    ))
}

pub static TOOL: Tool = Tool {
    id: "regex-tester",
    name: "Regex tester",
    description: "Test your regular expressions with sample text.",
    category: Category::Development,
    keywords: &["regex", "regexp", "test", "match", "pattern", "expression"],
    // One checkbox per flag, as upstream, with its defaults (g and s on).
    fields: &[
        Field::text("regex", "Regular expression"),
        Field::area("text", "Text to match against"),
        Field::bool("global", "Global search (g)", "true"),
        Field::bool("ignore_case", "Case-insensitive search (i)", "false"),
        Field::bool("multiline", "Multiline (m)", "false"),
        Field::bool("dot_all", "Singleline: . matches newlines (s)", "true"),
    ],
    kind: Kind::Pure,
    run: regex_tester,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    /// it-tools PR #1391: an optional group that does not participate used to be
    /// dropped from the list, so `(a)?(b)` reported a lone "group 2" — a hole in
    /// the numbering that reads as "group 1 matched nothing at all", and a named
    /// group that simply vanished.
    #[test]
    fn optional_groups_keep_their_place() {
        let m = regex_matches("(a)?(b)", "b", "").unwrap();
        assert_eq!(m.len(), 1);
        let names: Vec<&str> = m[0].captures.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["1", "2"], "the absent group keeps its row");
        assert_eq!(m[0].captures[0], Capture::absent("1"));
        assert_eq!(m[0].captures[1].at, Some(("b".to_string(), 0, 1)));

        // A group that matched the empty string is a different thing from one
        // that did not run: `(x?)` participates and captures "".
        let m = regex_matches("(x?)b", "b", "").unwrap();
        assert_eq!(m[0].captures[0].at, Some((String::new(), 0, 0)));

        // Named and absent at once: the name still has to show up.
        let m = regex_matches("(?<first>a)?(?<second>b)", "b", "").unwrap();
        assert_eq!(m[0].groups.len(), 2);
        assert_eq!(m[0].groups[0], Capture::absent("first"));

        let out = format_matches(m);
        assert!(out.contains("group 1: (did not participate)"), "{out}");
        assert!(out.contains("group 2: \"b\" [0..1]"), "{out}");
        assert!(out.contains("named first: (did not participate)"), "{out}");
        assert!(out.contains("named second: \"b\" [0..1]"), "{out}");
    }

    #[test]
    fn optional_groups_through_the_tool() {
        let out = ok("regex-tester", &[("regex", "(a)?(b)"), ("text", "ab b")]);
        assert!(out.contains("2 match(es)"), "{out}");
        // First match takes the optional group, the second does not — and both
        // still say "group 1" and "group 2".
        assert!(out.contains("group 1: \"a\" [0..1]"), "{out}");
        assert!(out.contains("group 1: (did not participate)"), "{out}");
    }
}
