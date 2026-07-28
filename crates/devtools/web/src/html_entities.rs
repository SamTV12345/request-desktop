//! `html-entities`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg, section};
use anyhow::Result;

/// it-tools shows an "Escape" and an "Unescape" card at once — no mode toggle.
pub(crate) fn html_entities(args: &Args) -> Result<String> {
    let t = tool("html-entities");
    let escaped = html_escape::encode_quoted_attribute(arg(args, t, "escape_input")).into_owned();
    let unescaped = html_escape::decode_html_entities(arg(args, t, "unescape_input")).into_owned();
    Ok(format!(
        "{}\n{}\n",
        section("Escaped", Ok(escaped)),
        section("Unescaped", Ok(unescaped)),
    ))
}

pub static TOOL: Tool = Tool {
    id: "html-entities",
    name: "Escape html entities",
    description: "Escape or unescape html entities (replace <, >, &, \" and ' to their html version).",
    category: Category::Web,
    keywords: &[
        "html",
        "entities",
        "escape",
        "unescape",
        "special",
        "characters",
        "tags",
    ],
    fields: &[
        Field {
            key: "escape_input",
            label: "Your string",
            kind: crate::FieldKind::TextArea,
            default: "<title>IT Tool</title>",
            options: &[],
        },
        Field {
            key: "unescape_input",
            label: "Your escaped string",
            kind: crate::FieldKind::TextArea,
            default: "&lt;title&gt;IT Tool&lt;/title&gt;",
            options: &[],
        },
    ],
    kind: Kind::Pure,
    run: html_entities,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    #[test]
    fn html_entities_escape_and_unescape() {
        let out = run(
            "html-entities",
            &[
                ("escape_input", "<title>IT Tool</title>"),
                ("unescape_input", "&lt;title&gt;IT Tool&lt;/title&gt;"),
            ],
        )
        .unwrap();
        assert!(
            out.contains("Escaped:\n&lt;title&gt;IT Tool&lt;/title&gt;"),
            "{out}"
        );
        assert!(out.contains("Unescaped:\n<title>IT Tool</title>"), "{out}");
    }
}
