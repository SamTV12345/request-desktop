//! `markdown-table-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Context, Result, bail};
use std::fmt::Write as _;

// it-tools PR #1807.

pub(crate) fn markdown_table(a: &Args) -> Result<String> {
    let input = s(a, "input", "");
    if input.trim().is_empty() {
        bail!("paste CSV or TSV — one row per line");
    }
    let delimiter = match s(a, "delimiter", "Auto") {
        "Auto" => {
            let first = input.lines().next().unwrap_or("");
            *[(b'\t', '\t'), (b';', ';'), (b',', ','), (b'|', '|')]
                .iter()
                .max_by_key(|(_, c)| first.matches(*c).count())
                .map(|(b, _)| b)
                .expect("non-empty candidate list")
        }
        "Comma" => b',',
        "Semicolon" => b';',
        "Tab" => b'\t',
        "Pipe" => b'|',
        other => bail!("unknown delimiter: {other}"),
    };
    let rule = match s(a, "align", "Left") {
        "Left" => ":--",
        "Center" => ":-:",
        "Right" => "--:",
        other => bail!("unknown alignment: {other}"),
    };

    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(input.as_bytes());
    let rows: Vec<Vec<String>> = rdr
        .records()
        .map(|r| r.map(|r| r.iter().map(str::to_string).collect()))
        .collect::<std::result::Result<_, _>>()
        .context("could not read that as delimited text")?;
    let Some((header, body)) = rows.split_first() else {
        bail!("no rows found");
    };
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    // A `|` inside a cell would end the cell.
    let cell = |v: &str| v.replace('|', "\\|").replace('\n', " ");
    let row = |cells: &[String]| {
        let mut c: Vec<String> = cells.iter().map(|v| cell(v)).collect();
        c.resize(width, String::new());
        format!("| {} |", c.join(" | "))
    };

    let mut out = String::new();
    let _ = writeln!(out, "{}", row(header));
    let _ = writeln!(out, "| {} |", vec![rule; width].join(" | "));
    for r in body {
        let _ = writeln!(out, "{}", row(r));
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "markdown-table-generator",
    name: "Markdown table generator",
    description: "CSV or TSV in, a padded Markdown table out.",
    category: Category::Text,
    keywords: &["markdown", "table", "csv", "tsv", "convert", "readme"],
    fields: &[
        Field::area("input", "CSV or TSV"),
        Field::select(
            "delimiter",
            "Delimiter",
            &["Auto", "Comma", "Semicolon", "Tab", "Pipe"],
            "Auto",
        ),
        Field::select("align", "Alignment", &["Left", "Center", "Right"], "Left"),
    ],
    kind: Kind::Pure,
    run: markdown_table,
};
