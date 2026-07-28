//! `snowflake-id-parser`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Context, Result};

// it-tools PR #1211.

/// `(name, epoch in milliseconds)`.
#[rustfmt::skip]
pub(crate) const SNOWFLAKE_EPOCHS: &[(&str, i64)] = &[
    ("Twitter/X", 1_288_834_974_657),
    ("Discord",   1_420_070_400_000),
    ("Instagram", 1_314_220_021_721),
    ("Unix",      0),
];

pub(crate) fn snowflake(a: &Args) -> Result<String> {
    let raw = s(a, "id", "1541815603606036480").trim();
    let id: u64 = raw
        .parse()
        .with_context(|| format!("a snowflake is an unsigned 64-bit integer, got {raw:?}"))?;
    let name = s(a, "epoch", "Twitter/X");
    let epoch = SNOWFLAKE_EPOCHS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, e)| *e)
        .with_context(|| format!("unknown epoch: {name}"))?;

    let millis = (id >> 22) as i64 + epoch;
    let when = chrono::DateTime::from_timestamp_millis(millis)
        .map(|d| d.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string())
        .unwrap_or_else(|| "out of range".into());
    Ok(format!(
        "timestamp: {when}\nunix milliseconds: {millis}\n\
         worker id: {}\nprocess id: {}\nsequence: {}\nbinary: {id:064b}\n",
        (id >> 17) & 0x1F,
        (id >> 12) & 0x1F,
        id & 0xFFF,
    ))
}

pub static TOOL: Tool = Tool {
    id: "snowflake-id-parser",
    name: "Snowflake ID parser",
    description: "Split a Twitter, Discord or Instagram snowflake into time, worker and sequence.",
    category: Category::Development,
    keywords: &[
        "snowflake",
        "discord",
        "twitter",
        "instagram",
        "id",
        "timestamp",
    ],
    fields: &[
        Field::text("id", "Snowflake").with_default("1541815603606036480"),
        Field::select(
            "epoch",
            "Epoch",
            &["Twitter/X", "Discord", "Instagram", "Unix"],
            "Twitter/X",
        ),
    ],
    kind: Kind::Pure,
    run: snowflake,
};
