//! `json-minify`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::Result;

pub(crate) fn json_minify(args: &Args) -> Result<String> {
    Ok(json(args, TOOLS[6], "input")?.to_string())
}

pub static TOOL: Tool = Tool {
    id: "json-minify",
    name: "JSON minify",
    description: "Strip all insignificant whitespace from a JSON document.",
    category: Category::Data,
    keywords: &["json", "minify", "compress"],
    fields: &[Field::area("input", "JSON")],
    kind: Kind::Pure,
    run: json_minify,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;
    #[test]
    fn json_minify_and_viewer() {
        assert_eq!(
            ok(
                "json-minify",
                &[("input", "{\n\t\"hello\": [\n\t\t\"world\"\n\t]\n}")]
            ),
            r#"{"hello":["world"]}"#
        );
        // it-tools' defaults are a 3-space indent with keys sorted.
        assert_eq!(
            ok("json-viewer", &[("input", r#"{"b":1,"a":2}"#)]),
            "{\n   \"a\": 2,\n   \"b\": 1\n}"
        );
        assert_eq!(
            ok(
                "json-viewer",
                &[("input", r#"{"b":1,"a":2}"#), ("sortKeys", "false")]
            ),
            "{\n   \"b\": 1,\n   \"a\": 2\n}"
        );
        assert_eq!(
            ok(
                "json-viewer",
                &[("input", r#"{"b":{"d":1,"c":2}}"#), ("indent", "2")]
            ),
            "{\n  \"b\": {\n    \"c\": 2,\n    \"d\": 1\n  }\n}"
        );
        assert_eq!(
            ok("json-viewer", &[("input", r#"{"a":1}"#), ("indent", "0")]),
            r#"{"a":1}"#
        );
        assert_eq!(
            ok("json-viewer", &[("input", r#"{"a":1}"#), ("indent", "4")]),
            "{\n    \"a\": 1\n}"
        );
    }
}
