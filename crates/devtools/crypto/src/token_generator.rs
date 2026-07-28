//! `token-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Result, bail};

pub(crate) const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
pub(crate) const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
pub(crate) const DIGITS: &str = "0123456789";
pub(crate) const SYMBOLS: &str = ".,;:!?./-\"'#{([-|\\@)]=}*+";

/// `length` chars drawn uniformly from `alphabet`.
pub(crate) fn random_token(alphabet: &str, length: usize) -> String {
    let chars: Vec<char> = alphabet.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    (0..length)
        .map(|_| chars[rand::random_range(0..chars.len())])
        .collect()
}

pub(crate) fn run_token(a: &Args) -> Result<String> {
    // it-tools drives the length with a slider, min 1, max 512.
    let length = n(a, "length", 64);
    if !(1..=512).contains(&length) {
        bail!("length must be between 1 and 512");
    }
    // 100 x 512 chars is still an instant paste; the cap only exists so a
    // mistyped count cannot make the UI build a novel on every keystroke.
    let count = n(a, "count", 1);
    if !(1..=100).contains(&count) {
        bail!("quantity must be between 1 and 100");
    }
    let mut alphabet = String::new();
    if b(a, "uppercase", true) {
        alphabet.push_str(UPPER);
    }
    if b(a, "lowercase", true) {
        alphabet.push_str(LOWER);
    }
    if b(a, "numbers", true) {
        alphabet.push_str(DIGITS);
    }
    if b(a, "symbols", false) {
        alphabet.push_str(SYMBOLS);
    }
    // An alphabet that is empty because every charset is switched off keeps
    // returning "" — that is it-tools' behaviour and the form makes the cause
    // obvious. An alphabet emptied by the exclude field does not: the user sees
    // three charsets ticked and nothing coming out, so say what happened.
    let excluded: Vec<char> = s(a, "exclude", "").chars().collect();
    if !alphabet.is_empty() && !excluded.is_empty() {
        alphabet.retain(|c| !excluded.contains(&c));
        if alphabet.is_empty() {
            bail!("the excluded characters leave nothing to draw from");
        }
    }
    let tokens: Vec<String> = (0..count)
        .map(|_| random_token(&alphabet, length as usize))
        .collect();
    Ok(tokens.join("\n"))
}

pub static TOOL: Tool = Tool {
    id: "token-generator",
    name: "Token generator",
    description: "Generate a random token with the character sets you pick.",
    category: Category::Crypto,
    keywords: &["token", "random", "password", "generator", "secret"],
    fields: &[
        Field::number("length", "Length", "64"),
        Field::bool("uppercase", "Uppercase", "true"),
        Field::bool("lowercase", "Lowercase", "true"),
        Field::bool("numbers", "Numbers", "true"),
        Field::bool("symbols", "Symbols", "false"),
        Field::number("count", "Quantity", "1"),
        // The point of this field is dropping look-alikes such as 0/O and l/1
        // from a token someone has to read off a screen.
        Field::text("exclude", "Exclude characters"),
    ],
    kind: Kind::Pure,
    run: run_token,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn token_generator_charsets() {
        let t = |pairs: &[(&str, &str)]| run_token(&args(pairs)).unwrap();
        assert_eq!(
            t(&[
                ("length", "10"),
                ("uppercase", "false"),
                ("lowercase", "false"),
                ("numbers", "false"),
                ("symbols", "false")
            ])
            .len(),
            0
        );
        for len in [5usize, 10, 100] {
            let token = t(&[("length", &len.to_string()), ("symbols", "true")]);
            assert_eq!(token.chars().count(), len);
        }
        let upper = t(&[
            ("length", "256"),
            ("lowercase", "false"),
            ("numbers", "false"),
        ]);
        assert!(upper.chars().all(|c| c.is_ascii_uppercase()), "{upper}");
        let lower = t(&[
            ("length", "256"),
            ("uppercase", "false"),
            ("numbers", "false"),
        ]);
        assert!(lower.chars().all(|c| c.is_ascii_lowercase()), "{lower}");
        let digits = t(&[
            ("length", "256"),
            ("uppercase", "false"),
            ("lowercase", "false"),
        ]);
        assert!(digits.chars().all(|c| c.is_ascii_digit()), "{digits}");
        let symbols = t(&[
            ("length", "256"),
            ("uppercase", "false"),
            ("lowercase", "false"),
            ("numbers", "false"),
            ("symbols", "true"),
        ]);
        assert!(symbols.chars().all(|c| SYMBOLS.contains(c)), "{symbols}");
        assert!(run_token(&args(&[("length", "-1")])).is_err());
    }

    #[test]
    fn token_generator_count_and_exclusions() {
        let out = run_token(&args(&[("length", "8"), ("count", "5")])).unwrap();
        assert_eq!(out.lines().count(), 5);
        assert!(out.lines().all(|l| l.chars().count() == 8), "{out}");
        assert!(run_token(&args(&[("count", "0")])).is_err());
        assert!(run_token(&args(&[("count", "101")])).is_err());

        // The look-alike case the PR is about.
        let out = run_token(&args(&[("length", "256"), ("exclude", "0Ol1")])).unwrap();
        assert!(!out.contains(['0', 'O', 'l', '1']), "{out}");
        // Excluding everything is an explained error, not an empty box...
        assert!(
            run_token(&args(&[
                ("lowercase", "false"),
                ("numbers", "false"),
                ("exclude", UPPER)
            ]))
            .is_err()
        );
        // ...but no charset at all still yields "", as it-tools does.
        assert_eq!(
            run_token(&args(&[
                ("uppercase", "false"),
                ("lowercase", "false"),
                ("numbers", "false"),
                ("exclude", "abc"),
            ]))
            .unwrap(),
            ""
        );
    }
}
