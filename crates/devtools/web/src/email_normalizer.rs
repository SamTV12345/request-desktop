//! `email-normalizer`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Context, Result, bail};

/// ponytail: provider rules only for the handful of hosts where the aliasing
/// behaviour is documented; everything else is just lower-cased and trimmed.
pub(crate) fn normalize_email(email: &str) -> Result<String> {
    let e = email.trim().to_lowercase();
    let (local, domain) = e.split_once('@').context("missing @")?;
    if local.is_empty() || domain.is_empty() || domain.contains('@') || !domain.contains('.') {
        bail!("not an email address");
    }
    let (local, domain) = match domain {
        "gmail.com" | "googlemail.com" => (
            local.split('+').next().unwrap_or(local).replace('.', ""),
            "gmail.com",
        ),
        "yahoo.com" | "yahoo.co.uk" | "ymail.com" | "rocketmail.com" => {
            (local.split('-').next().unwrap_or(local).to_string(), domain)
        }
        "outlook.com" | "hotmail.com" | "live.com" | "msn.com" | "icloud.com" | "me.com"
        | "protonmail.com" | "proton.me" | "pm.me" | "fastmail.com" | "zoho.com" | "gmx.com"
        | "gmx.de" | "yandex.com" | "yandex.ru" => {
            (local.split('+').next().unwrap_or(local).to_string(), domain)
        }
        _ => (local.to_string(), domain),
    };
    if local.is_empty() {
        bail!("empty local part after normalization");
    }
    Ok(format!("{local}@{domain}"))
}

pub(crate) fn email_normalizer(args: &Args) -> Result<String> {
    let input = arg(args, tool("email-normalizer"), "emails");
    if input.trim().is_empty() {
        bail!("no emails provided");
    }
    Ok(input
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            normalize_email(l).unwrap_or_else(|_| format!("Unable to parse email: {}", l.trim()))
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

pub static TOOL: Tool = Tool {
    id: "email-normalizer",
    name: "Email normalizer",
    description: "Normalize email addresses to a standard format for easier comparison and deduplication.",
    category: Category::Data,
    keywords: &["email", "normalizer"],
    fields: &[Field::area(
        "emails",
        "Raw emails to normalize (one per line)",
    )],
    kind: Kind::Pure,
    run: email_normalizer,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    #[test]
    fn email_normalizer_lines() {
        let input =
            "John.Doe+spam@Gmail.com\nfoo.bar@googlemail.com\nA.B+x@example.com\nnot-an-email";
        let out = run("email-normalizer", &[("emails", input)]).unwrap();
        let lines: Vec<_> = out.lines().collect();
        assert_eq!(lines[0], "johndoe@gmail.com");
        assert_eq!(lines[1], "foobar@gmail.com");
        assert_eq!(lines[2], "a.b+x@example.com");
        assert_eq!(lines[3], "Unable to parse email: not-an-email");
        assert!(run("email-normalizer", &[("emails", "   ")]).is_err());
    }
}
