//! `sensitive-data-masker`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, section};
use anyhow::{Result, bail};
use std::sync::OnceLock;

// it-tools PR #1182: paste a log, get something you can put in a ticket.

/// `(label, pattern, which capture group holds the secret)`. Group 0 is the
/// whole match.
#[rustfmt::skip]
pub(crate) fn maskers() -> &'static [(&'static str, regex::Regex, usize)] {
    static P: OnceLock<Vec<(&'static str, regex::Regex, usize)>> = OnceLock::new();
    P.get_or_init(|| {
        let r = |p: &str| regex::Regex::new(p).expect("static pattern");
        vec![
            // Longest and most specific first: a JWT is also three "words".
            ("JWT",             r(r"\beyJ[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]*"), 0),
            ("Bearer token",    r(r"(?i)\b(bearer|basic)\s+([A-Za-z0-9._~+/=-]{8,})"), 2),
            ("Private key",     r("(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----"), 0),
            ("AWS access key",  r(r"\b(?:AKIA|ASIA|AGPA|AIDA)[0-9A-Z]{12,16}\b"), 0),
            ("GitHub token",    r(r"\bgh[pousr]_[A-Za-z0-9]{20,}\b"), 0),
            ("Credential pair", r(r#"(?i)\b(password|passwd|pwd|secret|api[_-]?key|token|client[_-]?secret)"?\s*[:=]\s*"?([^"'\s,;}&]+)"#), 2),
            ("Card number",     r(r"\b(?:\d[ -]?){12,18}\d\b"), 0),
            ("Email",           r(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b"), 0),
            ("IPv4",            r(r"\b(?:\d{1,3}\.){3}\d{1,3}\b"), 0),
        ]
    })
}

pub(crate) fn mask_value(value: &str, keep: usize, ch: char) -> String {
    let chars: Vec<char> = value.chars().collect();
    let hidden = chars.len().saturating_sub(keep);
    let mut out: String = std::iter::repeat_n(ch, hidden).collect();
    out.extend(chars[hidden..].iter());
    out
}

pub(crate) fn sensitive_masker(a: &Args) -> Result<String> {
    let input = s(a, "input", "");
    if input.trim().is_empty() {
        bail!("paste the log, config or payload to mask");
    }
    let keep = n(a, "reveal", 0).clamp(0, 8) as usize;
    let ch = s(a, "maskChar", "*").chars().next().unwrap_or('*');

    let mut text = input.to_string();
    let mut found: Vec<String> = Vec::new();
    for (label, re, group) in maskers() {
        let mut hits = 0usize;
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                let Some(secret) = caps.get(*group) else {
                    return caps[0].to_string();
                };
                hits += 1;
                let whole = &caps[0];
                let start = secret.start() - caps.get(0).expect("group 0").start();
                format!(
                    "{}{}{}",
                    &whole[..start],
                    mask_value(secret.as_str(), keep, ch),
                    &whole[start + secret.as_str().len()..]
                )
            })
            .into_owned();
        if hits > 0 {
            found.push(format!("{label}: {hits}"));
        }
    }
    if found.is_empty() {
        found.push(
            "nothing matched — the text is already clean, or the secret has an unusual shape"
                .into(),
        );
    }
    Ok([
        section("Masked", Ok(text)),
        section("Found", Ok(found.join("\n"))),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "sensitive-data-masker",
    name: "Sensitive data masker",
    description: "Mask tokens, keys, cards, emails and IPs in a log before pasting it anywhere.",
    category: Category::Text,
    keywords: &[
        "mask",
        "redact",
        "secret",
        "log",
        "anonymise",
        "sanitize",
        "pii",
    ],
    fields: &[
        Field::area("input", "Text"),
        Field::text("maskChar", "Mask character"),
        Field::number("reveal", "Reveal last N characters", "0"),
    ],
    kind: Kind::Pure,
    run: sensitive_masker,
};
