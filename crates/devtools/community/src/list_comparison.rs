//! `list-comparison`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, section};
use anyhow::Result;

// it-tools PR #1303.

pub(crate) fn list_comparison(a: &Args) -> Result<String> {
    let case_sensitive = b(a, "caseSensitive", true);
    let key = |v: &str| {
        if case_sensitive {
            v.to_string()
        } else {
            v.to_lowercase()
        }
    };
    let a_lines = lines(s(a, "listA", ""));
    let b_lines = lines(s(a, "listB", ""));
    let a_keys: Vec<String> = a_lines.iter().map(|v| key(v)).collect();
    let b_keys: Vec<String> = b_lines.iter().map(|v| key(v)).collect();

    // Order-preserving, first occurrence wins — a diff you can paste back.
    let unique = |src: &[&str], keys: &[String], other: Option<&[String]>| {
        let mut seen: Vec<&String> = Vec::new();
        let mut out = Vec::new();
        for (v, k) in src.iter().zip(keys) {
            let wanted = match other {
                Some(o) => !o.contains(k),
                None => true,
            };
            if wanted && !seen.contains(&k) {
                seen.push(k);
                out.push(*v);
            }
        }
        out.join("\n")
    };
    let both = {
        let mut seen: Vec<&String> = Vec::new();
        let mut out = Vec::new();
        for (v, k) in a_lines.iter().zip(&a_keys) {
            if b_keys.contains(k) && !seen.contains(&k) {
                seen.push(k);
                out.push(*v);
            }
        }
        out.join("\n")
    };
    let duplicates = |src: &[&str], keys: &[String]| {
        let mut out = Vec::new();
        for (i, (v, k)) in src.iter().zip(keys).enumerate() {
            if keys[..i].contains(k) && !out.contains(v) {
                out.push(*v);
            }
        }
        out.join("\n")
    };

    Ok([
        section(
            "Counts",
            Ok(format!(
                "A: {} line(s)\nB: {} line(s)\n",
                a_lines.len(),
                b_lines.len()
            )),
        ),
        section("Only in A", Ok(unique(&a_lines, &a_keys, Some(&b_keys)))),
        section("Only in B", Ok(unique(&b_lines, &b_keys, Some(&a_keys)))),
        section("In both", Ok(both)),
        section(
            "Union",
            Ok({
                let mut all: Vec<&str> = a_lines.clone();
                all.extend(&b_lines);
                let mut all_keys = a_keys.clone();
                all_keys.extend(b_keys.clone());
                unique(&all, &all_keys, None)
            }),
        ),
        section("Duplicates in A", Ok(duplicates(&a_lines, &a_keys))),
        section("Duplicates in B", Ok(duplicates(&b_lines, &b_keys))),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "list-comparison",
    name: "List comparison",
    description: "Two lists in, set difference, intersection, union and duplicates out.",
    category: Category::Data,
    keywords: &[
        "list",
        "compare",
        "diff",
        "set",
        "intersection",
        "union",
        "duplicates",
    ],
    fields: &[
        Field::area("listA", "List A"),
        Field::area("listB", "List B"),
        Field::bool("caseSensitive", "Case sensitive", "true"),
    ],
    kind: Kind::Pure,
    run: list_comparison,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;
    #[test]
    fn list_comparison_is_order_preserving_and_case_aware() {
        let args = [("listA", "b\na\nB\na"), ("listB", "a\nc")];
        let out = run("list-comparison", &args);
        assert!(out.contains("Only in A:\nb\nB\n"), "{out}");
        assert!(out.contains("Only in B:\nc"), "{out}");
        assert!(out.contains("Duplicates in A:\na"), "{out}");
        let insensitive = run(
            "list-comparison",
            &[
                ("listA", "b\na\nB\na"),
                ("listB", "a\nc"),
                ("caseSensitive", "false"),
            ],
        );
        // `B` folds onto `b`, so it is a duplicate rather than a second entry.
        assert!(insensitive.contains("Only in A:\nb\n\n"), "{insensitive}");
        assert!(
            insensitive.contains("Duplicates in A:\nB\na"),
            "{insensitive}"
        );
    }
}
