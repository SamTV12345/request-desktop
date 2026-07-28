//! `nanoid-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Result, bail};
use std::fmt::Write as _;

// it-tools PR #1138. 21 chars of a 64-symbol alphabet is nanoid's default.

pub(crate) const NANOID_ALPHABET: &str =
    "_-0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

pub(crate) fn nanoid(a: &Args) -> Result<String> {
    let size = n(a, "size", 21);
    if !(1..=256).contains(&size) {
        bail!("size must be between 1 and 256");
    }
    let count = n(a, "count", 5);
    if !(1..=100).contains(&count) {
        bail!("quantity must be between 1 and 100");
    }
    let alphabet: Vec<char> = s(a, "alphabet", NANOID_ALPHABET).chars().collect();
    if alphabet.len() < 2 {
        bail!("the alphabet needs at least two distinct characters");
    }
    let mut out = String::new();
    for _ in 0..count {
        let id: String = (0..size).map(|_| *pick(&alphabet)).collect();
        let _ = writeln!(out, "{id}");
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "nanoid-generator",
    name: "Nano ID generator",
    description: "URL-safe, collision-resistant ids — shorter than a UUID, same safety.",
    category: Category::Crypto,
    keywords: &["nanoid", "id", "identifier", "url", "short", "random"],
    fields: &[
        Field::number("size", "Length", "21"),
        Field::number("count", "How many", "5"),
        Field::text("alphabet", "Alphabet"),
    ],
    kind: Kind::Pure,
    run: nanoid,
};
