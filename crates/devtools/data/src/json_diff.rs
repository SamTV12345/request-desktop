//! `json-diff`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::Result;

pub(crate) fn json_diff(args: &Args) -> Result<String> {
    let t = &TOOLS[7];
    let left = serde_json::to_string_pretty(&json(args, t, "left")?)?;
    let right = serde_json::to_string_pretty(&json(args, t, "right")?)?;
    if left == right {
        return Ok("the two documents are equal".into());
    }
    Ok(similar::TextDiff::from_lines(&left, &right)
        .unified_diff()
        .header("left", "right")
        .to_string())
}

pub static TOOL: Tool = Tool {
    id: "json-diff",
    name: "JSON diff",
    description: "Compare two JSON documents as a unified diff.",
    category: Category::Data,
    keywords: &["json", "diff", "compare"],
    fields: &[
        Field::area("left", "Left JSON"),
        Field::area("right", "Right JSON"),
    ],
    kind: Kind::Pure,
    run: json_diff,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;
    #[test]
    fn json_diff_reports_changes_and_equality() {
        assert_eq!(
            ok(
                "json-diff",
                &[("left", r#"{"a":1}"#), ("right", r#"{"a":1}"#)]
            ),
            "the two documents are equal"
        );
        let d = ok(
            "json-diff",
            &[("left", r#"{"a":1}"#), ("right", r#"{"a":2}"#)],
        );
        assert!(d.contains("-  \"a\": 1"), "{d}");
        assert!(d.contains("+  \"a\": 2"), "{d}");
        assert!(run("json-diff", &[("left", "{"), ("right", "{}")]).is_err());
        assert!(run("json-diff", &[("left", "{}")]).is_err());
    }
}
