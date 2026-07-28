//! `folder-tree-diagram` — it-tools PR #1020.
//!
//! An indented list, or a list of paths, becomes the `├──` drawing that goes in
//! a README. Both inputs are accepted because both are what you have: the
//! indented one you typed, or the `git ls-files` output you pasted.

use crate::*;
use anyhow::{Result, bail};
use std::fmt::Write as _;

#[derive(Default)]
struct Node {
    /// Insertion order matters: the drawing should follow the input, not an
    /// alphabet the user did not ask for.
    children: Vec<(String, Node)>,
}

impl Node {
    fn child(&mut self, name: &str) -> &mut Node {
        if let Some(i) = self.children.iter().position(|(n, _)| n == name) {
            return &mut self.children[i].1;
        }
        self.children.push((name.to_string(), Node::default()));
        &mut self.children.last_mut().expect("just pushed").1
    }

    fn draw(&self, prefix: &str, out: &mut String) {
        for (i, (name, child)) in self.children.iter().enumerate() {
            let last = i + 1 == self.children.len();
            let _ = writeln!(out, "{prefix}{} {name}", if last { "└──" } else { "├──" });
            child.draw(
                &format!("{prefix}{}", if last { "    " } else { "│   " }),
                out,
            );
        }
    }
}

/// Leading whitespace as a depth, counting a tab as four columns and rounding
/// down to whatever unit the file itself uses.
fn indent_of(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

fn tree(a: &Args) -> Result<String> {
    let input = s(a, "input", DEFAULT);
    let rows = lines(input);
    if rows.is_empty() {
        bail!("paste an indented list, or one path per line");
    }
    let mut root = Node::default();

    if rows.iter().any(|l| l.contains('/') || l.contains('\\')) {
        for row in &rows {
            let mut node = &mut root;
            for part in row.split(['/', '\\']).filter(|p| !p.is_empty()) {
                node = node.child(part);
            }
        }
    } else {
        // Indentation mode: the smallest non-zero indent is one level.
        let raw: Vec<(usize, &str)> = input
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| (indent_of(l), l.trim()))
            .collect();
        let unit = raw
            .iter()
            .map(|(i, _)| *i)
            .filter(|i| *i > 0)
            .min()
            .unwrap_or(1);
        let mut path: Vec<String> = Vec::new();
        for (indent, name) in raw {
            let depth = indent / unit;
            path.truncate(depth);
            path.push(name.to_string());
            let mut node = &mut root;
            for part in &path {
                node = node.child(part);
            }
        }
    }

    let mut out = String::from(".\n");
    root.draw("", &mut out);
    Ok(out)
}

const DEFAULT: &str = "src\n  main.rs\n  ui\n    button.rs\n    theme.rs\nCargo.toml\nREADME.md";

pub static TOOL: Tool = Tool {
    id: "folder-tree-diagram",
    name: "Folder tree diagram",
    description: "An indented list or a list of paths, drawn as the tree you put in a README.",
    category: Category::Text,
    keywords: &[
        "tree",
        "folder",
        "directory",
        "structure",
        "readme",
        "ascii",
    ],
    fields: &[Field::area("input", "Indented list, or one path per line").with_default(DEFAULT)],
    kind: Kind::Pure,
    run: tree,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn indentation_becomes_a_drawing() {
        let out = ok("folder-tree-diagram", &[]);
        assert!(out.starts_with(".\n├── src\n"), "{out}");
        assert!(out.contains("│   ├── main.rs"), "{out}");
        assert!(
            out.contains(
                "│   └── ui
"
            ),
            "{out}"
        );
        assert!(out.contains("│       ├── button.rs"), "{out}");
        assert!(out.contains("└── README.md"), "{out}");
        // The last child of a branch closes it.
        assert!(out.contains("│       └── theme.rs"), "{out}");
    }

    #[test]
    fn paths_merge_into_one_tree() {
        let out = ok(
            "folder-tree-diagram",
            &[("input", "src/a.rs\nsrc/ui/b.rs\nsrc/ui/c.rs\nREADME.md")],
        );
        assert!(out.contains("├── src\n"), "{out}");
        assert!(out.contains("│   └── ui\n"), "{out}");
        assert!(out.contains("│       ├── b.rs"), "{out}");
        assert!(out.contains("│       └── c.rs"), "{out}");
        // Windows separators are paths too.
        assert!(ok("folder-tree-diagram", &[("input", r"a\b\c")]).contains("└── c"));
        assert!(run("folder-tree-diagram", &[("input", "   ")]).is_err());
    }
}
