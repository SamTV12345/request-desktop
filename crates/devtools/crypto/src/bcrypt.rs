//! `bcrypt`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Result, bail};

// it-tools renders two independent cards, "Hash" and "Compare string with
// hash", both always visible. So both are always emitted, each as its own
// section: a salt count the hasher rejects must not hide the comparison.
/// Hash `text`, applying the salt-round rule in one place so the registry entry
/// and the app's bcrypt panel cannot disagree about it.
///
/// bcryptjs' `genSaltSync` silently clamps rounds into 4..=31, which is why
/// it-tools can offer a 0..=100 number input at all.
///
/// ponytail: the upper end is clamped to 15, not 31 — cost 31 is ~18 hours of
/// CPU for one hash. Say so instead of freezing the UI.
pub fn bcrypt_hash_with_rounds(text: &str, rounds: i64) -> Result<String> {
    let rounds = rounds.clamp(4, 31);
    if rounds > 15 {
        bail!("salt rounds above 15 would take minutes to hours per hash; not run here");
    }
    ::bcrypt::hash(text, rounds as u32).map_err(Into::into)
}

/// Whether `text` is the string behind `hash`.
///
/// `Err` means `hash` is not a bcrypt hash at all, which is a different answer
/// from "does not match" — the text output collapses the two, a UI should not.
pub fn bcrypt_verify(text: &str, hash: &str) -> Result<bool> {
    ::bcrypt::verify(text, hash.trim()).map_err(Into::into)
}

pub(crate) fn bcrypt_hash(a: &Args) -> Result<String> {
    bcrypt_hash_with_rounds(s(a, "text", ""), n(a, "rounds", 10))
}

pub(crate) fn bcrypt_compare(a: &Args) -> Result<String> {
    let matches = bcrypt_verify(s(a, "compare_text", ""), s(a, "compare_hash", ""));
    Ok(match matches.unwrap_or(false) {
        true => "Yes".into(),
        false => "No".into(),
    })
}

pub(crate) fn run_bcrypt(a: &Args) -> Result<String> {
    Ok(format!(
        "{}\n{}\n",
        crate::section("Hash", bcrypt_hash(a)),
        crate::section("Compare string with hash", bcrypt_compare(a))
    ))
}

pub static TOOL: Tool = Tool {
    id: "bcrypt",
    name: "Bcrypt",
    description: "Hash a string with bcrypt, or check a string against a bcrypt hash.",
    category: Category::Crypto,
    keywords: &["bcrypt", "hash", "password", "salt", "compare"],
    // Two always-visible cards upstream: Hash (string + salt count) and
    // Compare string with hash (string + hash -> Yes/No).
    fields: &[
        Field::text("text", "Your string"),
        Field::number("rounds", "Salt count", "10"),
        Field::text("compare_text", "Your string (compare)"),
        Field::text("compare_hash", "Your hash (compare)"),
    ],
    kind: Kind::Pure,
    run: run_bcrypt,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn bcrypt_hash_and_verify() {
        // Fixed vector: this cost-10 hash verifies "password" and nothing else.
        let hash = "$2b$10$TBE7NUU3/Wl0F9..9h8iBe6trDPbRxdGiDbJjgQWgn5hSJ7Ags32W";
        let out = run_bcrypt(&args(&[
            ("text", "whatever"),
            ("rounds", "4"),
            ("compare_text", "password"),
            ("compare_hash", hash),
        ]))
        .unwrap();
        assert!(out.contains("Compare string with hash:\nYes"), "{out}");

        let out = run_bcrypt(&args(&[
            ("rounds", "4"),
            ("compare_text", "wrong"),
            ("compare_hash", hash),
        ]))
        .unwrap();
        assert!(out.contains("Compare string with hash:\nNo"), "{out}");
    }

    #[test]
    fn bcrypt_round_trip_and_bounds() {
        let out = run_bcrypt(&args(&[("text", "secret"), ("rounds", "4")])).unwrap();
        let produced = out.lines().nth(1).unwrap();
        assert!(::bcrypt::verify("secret", produced).unwrap());
        // Below 4 is clamped up to 4, as bcryptjs does; above 15 is refused,
        // but only inside the Hash panel — Compare still answers.
        assert!(
            run_bcrypt(&args(&[("rounds", "1")]))
                .unwrap()
                .contains("$2")
        );
        let out = run_bcrypt(&args(&[("rounds", "99")])).unwrap();
        assert!(out.contains("Hash:\n  (salt rounds above 15"), "{out}");
        assert!(out.contains("Compare string with hash:\nNo"), "{out}");
    }
}
