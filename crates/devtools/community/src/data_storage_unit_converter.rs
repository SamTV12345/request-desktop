//! `data-storage-unit-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Context, Result, bail};
use std::fmt::Write as _;

// it-tools PRs #1289 / #1811.

/// `(name, bytes)` — decimal, binary and the bit units that make network
/// numbers look ten times better than they are.
#[rustfmt::skip]
pub(crate) const STORAGE_UNITS: &[(&str, f64)] = &[
    ("bit",  0.125),          ("B",   1.0),
    ("kbit", 125.0),          ("kB",  1e3),   ("KiB", 1024.0),
    ("Mbit", 125e3),          ("MB",  1e6),   ("MiB", 1024.0 * 1024.0),
    ("Gbit", 125e6),          ("GB",  1e9),   ("GiB", 1024.0 * 1024.0 * 1024.0),
    ("Tbit", 125e9),          ("TB",  1e12),  ("TiB", 1024.0 * 1024.0 * 1024.0 * 1024.0),
    ("PB",   1e15),           ("PiB", 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0),
];

pub(crate) fn storage_units(a: &Args) -> Result<String> {
    let value = f(a, "value", 1.0);
    if !value.is_finite() {
        bail!("enter a finite number");
    }
    let unit = s(a, "unit", "MiB");
    let factor = STORAGE_UNITS
        .iter()
        .find(|(n, _)| *n == unit)
        .map(|(_, f)| *f)
        .with_context(|| format!("unknown unit: {unit}"))?;
    let bytes = value * factor;
    let mut out = String::new();
    for (name, f) in STORAGE_UNITS {
        let _ = writeln!(out, "{name:>5}: {}", trim_float(bytes / f));
    }
    Ok(out)
}

/// `{:.6}` without the trailing zeros, so `1 MiB` reads `1` and not `1.000000`.
pub(crate) fn trim_float(v: f64) -> String {
    if v != 0.0 && (v.abs() >= 1e15 || v.abs() < 1e-6) {
        return format!("{v:e}");
    }
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

pub static TOOL: Tool = Tool {
    id: "data-storage-unit-converter",
    name: "Data storage unit converter",
    description: "Bytes, kB and KiB, bits and bytes — every unit at once, so the factor 8 is visible.",
    category: Category::Measurement,
    keywords: &[
        "storage", "bytes", "kib", "mib", "gigabyte", "bit", "size", "unit",
    ],
    fields: &[
        Field::number("value", "Value", "1"),
        Field::select(
            "unit",
            "Unit",
            &[
                "bit", "B", "kbit", "kB", "KiB", "Mbit", "MB", "MiB", "Gbit", "GB", "GiB", "Tbit",
                "TB", "TiB", "PB", "PiB",
            ],
            "MiB",
        ),
    ],
    kind: Kind::Pure,
    run: storage_units,
};
