//! `ulid-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Result, bail};

pub(crate) fn run_ulid(a: &Args) -> Result<String> {
    let count = n(a, "count", 1);
    if !(1..=100).contains(&count) {
        bail!("quantity must be between 1 and 100");
    }
    let ids: Vec<String> = (0..count)
        .map(|_| ulid::Ulid::generate().to_string())
        .collect();
    Ok(match s(a, "format", "Raw") {
        "Raw" => ids.join("\n"),
        "JSON" => serde_json::to_string_pretty(&ids)?,
        other => bail!("unknown format: {other}"),
    })
}

pub static TOOL: Tool = Tool {
    id: "ulid-generator",
    name: "ULID generator",
    description: "Generate sortable, lexicographically ordered ULIDs.",
    category: Category::Crypto,
    keywords: &["ulid", "unique", "identifier", "sortable", "generator"],
    fields: &[
        Field::number("count", "Quantity", "1"),
        Field::select("format", "Format", &["Raw", "JSON"], "Raw"),
    ],
    kind: Kind::Pure,
    run: run_ulid,
};
