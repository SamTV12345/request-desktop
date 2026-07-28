//! `passphrase-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Result, bail};
use std::fmt::Write as _;

// it-tools PR #1260. The BIP-39 English list is already a dependency and is a
// better diceware list than anything worth hand-rolling.

pub(crate) fn passphrase(a: &Args) -> Result<String> {
    let words = n(a, "words", 4);
    if !(2..=24).contains(&words) {
        bail!("word count must be between 2 and 24");
    }
    let count = n(a, "count", 3);
    if !(1..=50).contains(&count) {
        bail!("quantity must be between 1 and 50");
    }
    let sep = s(a, "separator", "-");
    let capitalize = b(a, "capitalize", true);
    let with_number = b(a, "number", true);
    let list = bip39::Language::English.word_list();

    let mut out = String::new();
    for _ in 0..count {
        let mut parts: Vec<String> = (0..words)
            .map(|_| {
                let w = *pick(list);
                if capitalize {
                    let mut c = w.chars();
                    match c.next() {
                        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                        None => w.to_string(),
                    }
                } else {
                    w.to_string()
                }
            })
            .collect();
        if with_number {
            // Appended to a random word rather than the end: a trailing digit is
            // the first thing a cracking rule tries.
            let at = rand::random_range(0..parts.len());
            let _ = write!(parts[at], "{}", rand::random_range(0..=99));
        }
        let _ = writeln!(out, "{}", parts.join(sep));
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "passphrase-generator",
    name: "Passphrase generator",
    description: "Diceware-style passphrases from the BIP-39 word list — memorable and long.",
    category: Category::Crypto,
    keywords: &[
        "passphrase",
        "password",
        "diceware",
        "words",
        "xkcd",
        "memorable",
    ],
    fields: &[
        Field::number("words", "Words per passphrase", "4"),
        Field::number("count", "How many", "3"),
        Field::text("separator", "Separator"),
        Field::bool("capitalize", "Capitalise each word", "true"),
        Field::bool("number", "Sprinkle in a number", "true"),
    ],
    kind: Kind::Pure,
    run: passphrase,
};
