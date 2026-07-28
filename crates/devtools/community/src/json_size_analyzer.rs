//! `json-size-analyzer`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, section};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::fmt::Write as _;

// it-tools PR #1323: which key is making the payload big.

pub(crate) fn human_bytes(n: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.2} {}", UNITS[i])
    }
}

pub(crate) fn json_size(a: &Args) -> Result<String> {
    let raw = s(a, "input", "").trim();
    if raw.is_empty() {
        bail!("no JSON input");
    }
    let value: Value = serde_json::from_str(raw).context("invalid JSON")?;
    let compact = serde_json::to_string(&value)?;
    let total = compact.len() as u64;

    let mut rows: Vec<(String, u64)> = match &value {
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    serde_json::to_string(v).map(|s| s.len()).unwrap_or(0) as u64,
                )
            })
            .collect(),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, v)| {
                (
                    format!("[{i}]"),
                    serde_json::to_string(v).map(|s| s.len()).unwrap_or(0) as u64,
                )
            })
            .collect(),
        _ => vec![],
    };
    rows.sort_by_key(|(_, size)| std::cmp::Reverse(*size));

    let mut breakdown = String::new();
    for (key, size) in rows.iter().take(50) {
        let pct = if total == 0 {
            0.0
        } else {
            *size as f64 * 100.0 / total as f64
        };
        let _ = writeln!(
            breakdown,
            "{:>10}  {:>5.1}%  {key}",
            human_bytes(*size),
            pct
        );
    }
    if rows.len() > 50 {
        let _ = writeln!(breakdown, "… and {} more", rows.len() - 50);
    }
    if rows.is_empty() {
        breakdown.push_str("(a scalar document has no members to break down)\n");
    }

    Ok([
        section(
            "Total",
            Ok(format!(
                "minified: {} ({total} bytes)\nas typed: {}\nmembers: {}\n",
                human_bytes(total),
                human_bytes(raw.len() as u64),
                rows.len()
            )),
        ),
        section("Largest members", Ok(breakdown)),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "json-size-analyzer",
    name: "JSON size analyser",
    description: "Which key is making the payload big: size per member, largest first.",
    category: Category::Data,
    keywords: &["json", "size", "bytes", "payload", "analyse", "weight"],
    fields: &[Field::area("input", "JSON")],
    kind: Kind::Pure,
    run: json_size,
};
