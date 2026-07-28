//! `crc-calculator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Result, bail};
use std::fmt::Write as _;

// it-tools PR #1140.

/// Reflected CRC, the shape all four catalogue entries below share.
pub(crate) fn crc_reflected(data: &[u8], poly: u64, init: u64, xorout: u64, width: u32) -> u64 {
    let mask = (1u64 << width) - 1;
    let mut crc = init & mask;
    for &byte in data {
        crc ^= byte as u64;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ poly
            } else {
                crc >> 1
            };
        }
    }
    (crc ^ xorout) & mask
}

/// `(name, reflected polynomial, init, xorout, width)`, from the CRC catalogue.
#[rustfmt::skip]
pub(crate) const CRCS: &[(&str, u64, u64, u64, u32)] = &[
    ("CRC-32/ISO-HDLC (zip, png)", 0xEDB8_8320, 0xFFFF_FFFF, 0xFFFF_FFFF, 32),
    ("CRC-32C (iSCSI, ext4)",      0x82F6_3B78, 0xFFFF_FFFF, 0xFFFF_FFFF, 32),
    ("CRC-16/ARC",                 0x0000_A001, 0x0000_0000, 0x0000_0000, 16),
    ("CRC-16/MODBUS",              0x0000_A001, 0x0000_FFFF, 0x0000_0000, 16),
];

pub(crate) fn crc_calculator(a: &Args) -> Result<String> {
    let raw = s(a, "input", "");
    if raw.is_empty() {
        bail!("enter the text to checksum");
    }
    let data = raw.as_bytes();
    let mut out = String::new();
    for (name, poly, init, xorout, width) in CRCS {
        let v = crc_reflected(data, *poly, *init, *xorout, *width);
        let digits = (*width / 4) as usize;
        let _ = writeln!(out, "{name}: {v:0digits$x} ({v})", digits = digits);
    }
    let _ = writeln!(out, "\n{} byte(s) checksummed", data.len());
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "crc-calculator",
    name: "CRC calculator",
    description: "CRC-32, CRC-32C and CRC-16 checksums of a string.",
    category: Category::Crypto,
    keywords: &["crc", "checksum", "crc32", "crc16", "modbus", "integrity"],
    fields: &[Field::area("input", "Text")],
    kind: Kind::Pure,
    run: crc_calculator,
};
