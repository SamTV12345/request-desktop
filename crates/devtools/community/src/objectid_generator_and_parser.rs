//! `objectid-generator-and-parser`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, section};
use anyhow::{Context, Result, bail};
use std::fmt::Write as _;

// it-tools PRs #1463 / #1233: generate a Mongo ObjectId, and read the date back
// out of one you paste in.

pub(crate) fn objectid_parse(hex: &str) -> Result<String> {
    let hex = hex.trim();
    if hex.len() != 24 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!(
            "an ObjectId is 24 hexadecimal characters, got {}",
            hex.len()
        );
    }
    let secs = i64::from_str_radix(&hex[..8], 16).context("bad timestamp")?;
    let when = chrono::DateTime::from_timestamp(secs, 0).context("timestamp out of range")?;
    Ok(format!(
        "timestamp: {}\nunix seconds: {secs}\nrandom (machine+process): {}\ncounter: {}\n",
        when.format("%Y-%m-%d %H:%M:%S UTC"),
        &hex[8..18],
        u32::from_str_radix(&hex[18..], 16).context("bad counter")?
    ))
}

pub(crate) fn objectid(a: &Args) -> Result<String> {
    let now = chrono::Utc::now().timestamp() as u32;
    let random: [u8; 5] = std::array::from_fn(|_| rand::random());
    let counter: u32 = rand::random_range(0..0x00FF_FFFF);
    let mut generated = format!("{now:08x}");
    for byte in random {
        let _ = write!(generated, "{byte:02x}");
    }
    let _ = write!(generated, "{counter:06x}");

    let mut panels = vec![
        section("Generated ObjectId", Ok(generated.clone())),
        section("Generated, decoded", objectid_parse(&generated)),
    ];
    let input = s(a, "objectId", "");
    if !input.trim().is_empty() {
        panels.push(section("Pasted ObjectId, decoded", objectid_parse(input)));
    }
    Ok(panels.join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "objectid-generator-and-parser",
    name: "Mongo ObjectId generator/parser",
    description: "Generate an ObjectId, or read the timestamp back out of one.",
    category: Category::Development,
    keywords: &["objectid", "mongo", "mongodb", "bson", "id", "timestamp"],
    fields: &[Field::text("objectId", "ObjectId to decode")],
    kind: Kind::Pure,
    run: objectid,
};
