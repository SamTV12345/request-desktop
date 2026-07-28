//! `password-strength-analyser`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::Result;

pub(crate) fn charset_length(password: &str) -> u32 {
    let mut len = 0;
    if password.chars().any(|c| c.is_ascii_lowercase()) {
        len += 26;
    }
    if password.chars().any(|c| c.is_ascii_uppercase()) {
        len += 26;
    }
    if password.chars().any(|c| c.is_ascii_digit()) {
        len += 10;
    }
    // JS `/\W|_/`: anything that is not [A-Za-z0-9], or an underscore.
    if password.chars().any(|c| c == '_' || !c.is_alphanumeric()) {
        len += 32;
    }
    len
}

pub(crate) fn prettify_quantity(q: f64) -> String {
    if q >= 1e21 {
        // JS switches to exponential notation here.
        let exp = q.log10().floor();
        return format!("{:.2}e+{}", q / 10f64.powf(exp), exp as i64);
    }
    // Thousands separators, like JS toLocaleString on an integer.
    let digits: Vec<char> = format!("{}", q as u64).chars().collect();
    let mut out = String::new();
    for (i, c) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c);
    }
    out
}

pub(crate) const TIME_UNITS: &[(&str, &str, f64)] = &[
    ("millennium", "millennia", 31_536_000_000.0),
    ("century", "centuries", 3_153_600_000.0),
    ("decade", "decades", 315_360_000.0),
    ("year", "years", 31_536_000.0),
    ("month", "months", 2_592_000.0),
    ("week", "weeks", 604_800.0),
    ("day", "days", 86_400.0),
    ("hour", "hours", 3_600.0),
    ("minute", "minutes", 60.0),
    ("second", "seconds", 1.0),
];

pub(crate) fn human_duration(mut seconds: f64) -> String {
    if seconds <= 0.001 {
        return "Instantly".into();
    }
    if seconds <= 1.0 {
        return "Less than a second".into();
    }
    let mut parts = Vec::new();
    for (unit, plural, in_unit) in TIME_UNITS {
        let quantity = (seconds / in_unit).floor();
        seconds %= in_unit;
        if quantity <= 0.0 {
            continue;
        }
        let label = if quantity > 1.0 { plural } else { unit };
        parts.push(format!("{} {label}", prettify_quantity(quantity)));
        if parts.len() == 2 {
            break;
        }
    }
    parts.join(", ")
}

/// it-tools hard-codes `guessesPerSecond` (the service parameter is never
/// passed from the `.vue`), so it is not a field here either.
pub(crate) const GUESSES_PER_SECOND: f64 = 1e9;

pub(crate) fn run_password_strength(a: &Args) -> Result<String> {
    let password = s(a, "password", "");
    let charset = charset_length(password);
    let length = password.chars().count();
    let entropy = if password.is_empty() {
        0.0
    } else {
        (charset as f64).log2() * length as f64
    };
    let seconds_to_crack = 2f64.powf(entropy) / GUESSES_PER_SECOND;
    // Same five readonly rows the `.vue` derives, in its order: the crack
    // duration card first, then length / entropy / charset size / score.
    Ok(format!(
        "duration to crack this password with brute force: {}\n\
         password length: {length}\n\
         entropy: {entropy:.2}\n\
         character set size: {charset}\n\
         score: {} / 100\n",
        human_duration(seconds_to_crack),
        ((entropy / 128.0).min(1.0) * 100.0).round(),
    ))
}

pub static TOOL: Tool = Tool {
    id: "password-strength-analyser",
    name: "Password strength analyser",
    description: "Estimate the entropy of a password and how long it takes to crack it.",
    category: Category::Crypto,
    keywords: &["password", "strength", "entropy", "crack", "brute", "force"],
    fields: &[Field::text("password", "Password")],
    kind: Kind::Pure,
    run: run_password_strength,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn charset_length_vectors() {
        assert_eq!(charset_length("abcdefghijklmnopqrstuvwxyz"), 26);
        assert_eq!(charset_length("ABCDEFGHIJKLMNOPQRSTUVWXYZ"), 26);
        assert_eq!(charset_length("0123456789"), 10);
        assert_eq!(charset_length("-_("), 32);
        assert_eq!(charset_length(""), 0);
        assert_eq!(charset_length("abcdefghijklmnopqrstuvwxyz0123456789"), 36);
        assert_eq!(
            charset_length("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_("),
            94
        );
    }

    #[test]
    fn password_strength_output() {
        let out = run_password_strength(&args(&[("password", "0123456789")])).unwrap();
        assert!(out.contains("character set size: 10"), "{out}");
        assert!(out.contains("password length: 10"), "{out}");
        // log2(10) * 10 = 33.22 bits
        assert!(out.contains("entropy: 33.22"), "{out}");
        assert!(out.contains("score: 26 / 100"), "{out}");
        assert!(
            out.contains("duration to crack this password with brute force: "),
            "{out}"
        );
        let empty = run_password_strength(&args(&[("password", "")])).unwrap();
        assert!(empty.contains("entropy: 0.00"), "{empty}");
        assert!(empty.contains("score: 0 / 100"), "{empty}");
    }

    #[test]
    fn human_duration_vectors() {
        assert_eq!(human_duration(0.0), "Instantly");
        assert_eq!(human_duration(0.5), "Less than a second");
        assert_eq!(human_duration(90.0), "1 minute, 30 seconds");
        assert_eq!(human_duration(3661.0), "1 hour, 1 minute");
        assert_eq!(human_duration(31_536_000.0 * 3.0), "3 years");
    }
}
