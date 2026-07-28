//! `pin-code-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Result, bail};
use std::fmt::Write as _;

// it-tools PR #1261.

pub(crate) fn pin_code(a: &Args) -> Result<String> {
    let length = n(a, "length", 6);
    if !(3..=32).contains(&length) {
        bail!("PIN length must be between 3 and 32");
    }
    let count = n(a, "count", 5);
    if !(1..=100).contains(&count) {
        bail!("quantity must be between 1 and 100");
    }
    let no_repeat = b(a, "distinct", false);
    if no_repeat && length > 10 {
        bail!("a PIN with no repeated digit cannot be longer than 10");
    }
    let mut out = String::new();
    for _ in 0..count {
        let mut pin = String::new();
        while pin.len() < length as usize {
            let d = rand::random_range(0..=9u8);
            let c = (b'0' + d) as char;
            if no_repeat && pin.contains(c) {
                continue;
            }
            pin.push(c);
        }
        let _ = writeln!(out, "{pin}");
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "pin-code-generator",
    name: "PIN code generator",
    description: "Random numeric PINs, optionally with no digit repeated.",
    category: Category::Crypto,
    keywords: &["pin", "code", "numeric", "otp", "random", "digits"],
    fields: &[
        Field::number("length", "Digits", "6"),
        Field::number("count", "How many", "5"),
        Field::bool("distinct", "No repeated digit", "false"),
    ],
    kind: Kind::Pure,
    run: pin_code,
};
