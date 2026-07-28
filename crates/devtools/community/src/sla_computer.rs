//! `sla-computer`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Result, bail};
use std::fmt::Write as _;

// it-tools PR #1304.

pub(crate) fn duration_words(seconds: f64) -> String {
    let total = seconds.round() as i64;
    let (d, h, m, s) = (
        total / 86400,
        (total % 86400) / 3600,
        (total % 3600) / 60,
        total % 60,
    );
    let mut parts = Vec::new();
    for (v, unit) in [(d, "d"), (h, "h"), (m, "m"), (s, "s")] {
        if v > 0 {
            parts.push(format!("{v}{unit}"));
        }
    }
    if parts.is_empty() {
        parts.push("0s".into());
    }
    parts.join(" ")
}

pub(crate) fn sla_computer(a: &Args) -> Result<String> {
    let uptime = f(a, "uptime", 99.9);
    if !(0.0..=100.0).contains(&uptime) {
        bail!("uptime must be a percentage between 0 and 100");
    }
    let down = (100.0 - uptime) / 100.0;
    let mut out = String::new();
    for (label, seconds) in [
        ("per day", 86_400.0),
        ("per week", 604_800.0),
        ("per month (30d)", 2_592_000.0),
        ("per quarter (91d)", 7_862_400.0),
        ("per year (365d)", 31_536_000.0),
    ] {
        let _ = writeln!(out, "{label:>18}: {}", duration_words(down * seconds));
    }
    let _ = writeln!(
        out,
        "\n{uptime}% uptime allows {:.4}% downtime",
        down * 100.0
    );
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "sla-computer",
    name: "SLA computer",
    description: "How much downtime an uptime percentage actually buys you.",
    category: Category::Measurement,
    keywords: &["sla", "uptime", "downtime", "nines", "availability", "slo"],
    fields: &[Field::number("uptime", "Uptime %", "99.9")],
    kind: Kind::Pure,
    run: sla_computer,
};
