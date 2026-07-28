//! `luhn-checker`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Result, bail};
use std::fmt::Write as _;

// it-tools PR #1320.

/// Luhn over the digits of `s`, plus the digit that would make it check out.
pub(crate) fn luhn(digits: &[u8]) -> (bool, u8) {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, d)| {
            let mut v = *d as u32;
            if i % 2 == 1 {
                v *= 2;
                if v > 9 {
                    v -= 9;
                }
            }
            v
        })
        .sum();
    // The check digit for `digits` used as a payload: shift once more, because
    // appending a digit flips which positions get doubled.
    let payload_sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, d)| {
            let mut v = *d as u32;
            if i % 2 == 0 {
                v *= 2;
                if v > 9 {
                    v -= 9;
                }
            }
            v
        })
        .sum();
    (
        sum.is_multiple_of(10),
        ((10 - (payload_sum % 10)) % 10) as u8,
    )
}

pub(crate) fn luhn_checker(a: &Args) -> Result<String> {
    let input = s(a, "numbers", "");
    let entries = lines(input);
    if entries.is_empty() {
        bail!("enter one number per line (card number, IMEI, …)");
    }
    let mut out = String::new();
    for entry in entries {
        let cleaned: String = entry
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .collect();
        if cleaned.is_empty() || !cleaned.chars().all(|c| c.is_ascii_digit()) {
            let _ = writeln!(
                out,
                "{entry}: not a number (digits, spaces and dashes only)"
            );
            continue;
        }
        let digits: Vec<u8> = cleaned.bytes().map(|b| b - b'0').collect();
        let (valid, _) = luhn(&digits);
        let (_, next) = luhn(&digits);
        let _ = writeln!(
            out,
            "{entry}: {} — {} digits, append {next} to make {cleaned} check out",
            if valid { "valid" } else { "INVALID" },
            digits.len()
        );
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "luhn-checker",
    name: "Luhn checker",
    description: "Validate card numbers, IMEIs and anything else with a Luhn check digit.",
    category: Category::Development,
    keywords: &["luhn", "checksum", "credit", "card", "imei", "mod10"],
    fields: &[Field::area("numbers", "One number per line")],
    kind: Kind::Pure,
    run: luhn_checker,
};
