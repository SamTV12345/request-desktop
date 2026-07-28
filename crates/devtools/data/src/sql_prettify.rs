//! `sql-prettify`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Result, bail};

pub(crate) fn sql_prettify(args: &Args) -> Result<String> {
    let t = &TOOLS[18];
    let input = arg(args, t, "input");
    if input.trim().is_empty() {
        bail!("no SQL input");
    }
    let opts = sqlformat::FormatOptions {
        indent: sqlformat::Indent::Spaces(num(args, t, "indent")?.clamp(1, 16) as u8),
        uppercase: match arg(args, t, "keywordCase") {
            "lower" => Some(false),
            "preserve" => None,
            _ => Some(true),
        },
        ..Default::default()
    };
    Ok(sqlformat::format(
        input,
        &sqlformat::QueryParams::None,
        &opts,
    ))
}

pub static TOOL: Tool = Tool {
    id: "sql-prettify",
    name: "SQL prettify and format",
    description: "Indent and normalise the keyword case of a SQL query.",
    category: Category::Development,
    keywords: &["sql", "format", "prettify", "query"],
    fields: &[
        Field::area("input", "SQL query"),
        Field::select(
            "keywordCase",
            "Keyword case",
            &["upper", "lower", "preserve"],
            "upper",
        ),
        Field::number("indent", "Indent size", "2"),
    ],
    kind: Kind::Pure,
    run: sql_prettify,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;
    #[test]
    fn sql_prettify_formats() {
        let out = ok(
            "sql-prettify",
            &[("input", "select field1,field2 from my_table where a=1;")],
        );
        assert!(out.starts_with("SELECT"), "{out}");
        assert!(out.contains("FROM"), "{out}");
        let lower = ok(
            "sql-prettify",
            &[("input", "SELECT a FROM b"), ("keywordCase", "lower")],
        );
        assert!(lower.starts_with("select"), "{lower}");
        assert!(run("sql-prettify", &[("input", "")]).is_err());
        assert!(run("sql-prettify", &[("input", "select 1"), ("indent", "x")]).is_err());
    }

    // ── case-converter ──────────────────────────────────────────────────
}
