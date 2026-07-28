//! `text-diff`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

pub(crate) fn text_diff(args: &Args) -> anyhow::Result<String> {
    let old = a(args, "text-diff", "old");
    let new = a(args, "text-diff", "new");
    if old.trim().is_empty() && new.trim().is_empty() {
        anyhow::bail!("provide two texts to compare");
    }
    let diff = similar::TextDiff::from_lines(old, new);
    let mut u = diff.unified_diff();
    let out = u.context_radius(3).header("old", "new").to_string();
    Ok(if out.trim().is_empty() {
        "texts are identical".into()
    } else {
        out
    })
}

pub static TOOL: Tool = Tool {
    id: "text-diff",
    name: "Text diff",
    description: "Compare two texts and see the differences between them.",
    category: Category::Text,
    keywords: &["text", "diff", "compare", "difference", "delta", "patch"],
    fields: &[
        Field::area("old", "Original text"),
        Field::area("new", "Modified text"),
    ],
    kind: Kind::Pure,
    run: text_diff,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    #[test]
    fn text_diff_output() {
        let out = run("text-diff", &[("old", "a\nb\n"), ("new", "a\nc\n")]).unwrap();
        assert!(out.contains("-b"), "{out}");
        assert!(out.contains("+c"), "{out}");
        assert_eq!(
            run("text-diff", &[("old", "x"), ("new", "x")]).unwrap(),
            "texts are identical"
        );
        assert!(run("text-diff", &[]).is_err());
    }
}
