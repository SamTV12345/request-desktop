//! `string-obfuscator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::Result;

pub(crate) fn obfuscate(
    input: &str,
    replacement: char,
    keep_first: usize,
    keep_last: usize,
    keep_space: bool,
) -> String {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            if keep_space && c == ' ' {
                return c;
            }
            if i < keep_first || i + keep_last >= len {
                c
            } else {
                replacement
            }
        })
        .collect()
}

pub(crate) fn run_obfuscator(a: &Args) -> Result<String> {
    // it-tools has no replacement-character input: the model hard-codes '*'.
    Ok(obfuscate(
        s(a, "text", ""),
        '*',
        n(a, "keep_first", 4).max(0) as usize,
        n(a, "keep_last", 4).max(0) as usize,
        b(a, "keep_space", true),
    ))
}

pub static TOOL: Tool = Tool {
    id: "string-obfuscator",
    name: "String obfuscator",
    description: "Obfuscate a string by keeping only its first and last characters.",
    category: Category::Crypto,
    keywords: &["obfuscate", "mask", "hide", "secret", "redact"],
    fields: &[
        Field::area("text", "String to obfuscate"),
        Field::number("keep_first", "Keep first", "4"),
        Field::number("keep_last", "Keep last", "4"),
        Field::bool("keep_space", "Keep spaces", "true"),
    ],
    kind: Kind::Pure,
    run: run_obfuscator,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn obfuscate_vectors() {
        assert_eq!(obfuscate("1234567890", '*', 4, 0, true), "1234******");
        assert_eq!(obfuscate("1234567890", 'x', 4, 0, true), "1234xxxxxx");
        assert_eq!(obfuscate("1234567890", '*', 5, 0, true), "12345*****");
        assert_eq!(obfuscate("1234567890", '*', 0, 5, true), "*****67890");
        assert_eq!(obfuscate("1234567890", '*', 5, 5, true), "1234567890");
        assert_eq!(obfuscate("1234567890", 'x', 2, 2, true), "12xxxxxx90");
        assert_eq!(obfuscate("12345 67890", '*', 4, 0, true), "1234* *****");
        assert_eq!(obfuscate("12345 67890", '*', 4, 0, false), "1234*******");
    }

    #[test]
    fn obfuscator_tool() {
        let out = run_obfuscator(&args(&[("text", "1234567890"), ("keep_last", "0")])).unwrap();
        assert_eq!(out, "1234******");
        // No replacement-character field upstream: always '*'.
        assert!(
            !TOOLS
                .iter()
                .find(|t| t.id == "string-obfuscator")
                .unwrap()
                .fields
                .iter()
                .any(|f| f.key == "replacement")
        );
        assert_eq!(run_obfuscator(&args(&[])).unwrap(), "");
    }
}
