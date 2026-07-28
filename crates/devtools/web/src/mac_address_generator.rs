//! `mac-address-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Result, bail};
use std::fmt::Write as _;

/// Port of `splitPrefix`: split on non-hex separators, or group by two when the
/// prefix is bare hex. Odd trailing nibbles get zero-padded on the left.
pub(crate) fn split_prefix(prefix: &str) -> Vec<String> {
    let pad = |s: &str| format!("{s:0>2}");
    if prefix.chars().all(|c| c.is_ascii_hexdigit()) {
        prefix
            .chars()
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|c| pad(&c.iter().collect::<String>()))
            .collect()
    } else {
        prefix
            .split(|c: char| !c.is_ascii_hexdigit())
            .filter(|s| !s.is_empty())
            .map(pad)
            .collect()
    }
}

pub(crate) fn mac_address_generator(args: &Args) -> Result<String> {
    let t = tool("mac-address-generator");
    let prefix = split_prefix(arg(args, t, "prefix").trim());
    if prefix.len() > 6 {
        bail!("prefix is longer than a MAC address");
    }
    let separator = arg(args, t, "separator");
    let uppercase = arg(args, t, "uppercase") == "true";
    let count: usize = arg(args, t, "quantity").trim().parse().unwrap_or(1);
    // it-tools caps the quantity input at 100.
    if !(1..=100).contains(&count) {
        bail!("quantity must be between 1 and 100");
    }
    let mut out = String::new();
    for _ in 0..count {
        let mut bytes = prefix.clone();
        while bytes.len() < 6 {
            bytes.push(format!("{:02x}", rand::random_range(0..=255u8)));
        }
        let mac = bytes.join(separator);
        let _ = writeln!(out, "{}", if uppercase { mac.to_uppercase() } else { mac });
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "mac-address-generator",
    name: "MAC address generator",
    description: "Generate random MAC addresses, optionally with a fixed vendor prefix.",
    category: Category::Network,
    keywords: &["mac", "address", "generator", "random", "prefix"],
    fields: &[
        Field::number("quantity", "Quantity", "1"),
        Field {
            key: "prefix",
            label: "MAC address prefix",
            kind: crate::FieldKind::Text,
            default: "64:16:7F",
            options: &[],
        },
        // it-tools' fourth separator option is "None", i.e. no separator.
        Field::select("separator", "Separator", &[":", "-", ".", ""], ":"),
        Field::bool("uppercase", "Uppercase", "true"),
    ],
    kind: Kind::Pure,
    run: mac_address_generator,
};
