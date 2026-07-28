//! `htpasswd-generator` — it-tools PR #902.
//!
//! Produces the `user:hash` line Apache, nginx and Traefik read out of a
//! `.htpasswd` file, and checks an existing line against a password — the half
//! you actually need at 3am when a basic-auth login stopped working.
//!
//! Only bcrypt and SHA-1 are offered. MD5-crypt/APR1 is deliberately absent:
//! it needs a bespoke 1000-round MD5 ladder plus its own base64 alphabet, no
//! dependency here provides it, and Apache has defaulted to bcrypt since 2.4.
//! SHA-1 is here because `{SHA}` lines are still all over old files and need
//! checking — it is unsalted and must not be used for new entries.

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, section};
use anyhow::{Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

pub(crate) const HTPASSWD_ALGOS: &[&str] = &["bcrypt", "SHA-1"];

/// `htpasswd -B` writes the `$2y$` prefix. `$2y$` and `$2b$` are the same hash
/// with the same salt layout — the tag only records which implementation wrote
/// it — so the crate's `$2b$` output is relabelled on the way out and back
/// again on the way in, and every reader accepts the result.
fn to_2y(hash: &str) -> String {
    match hash.strip_prefix("$2b$") {
        Some(rest) => format!("$2y${rest}"),
        None => hash.to_string(),
    }
}

fn sha1_entry(password: &str) -> Result<String> {
    Ok(format!(
        "{{SHA}}{}",
        STANDARD.encode(hash_bytes("SHA1", password.as_bytes())?)
    ))
}

pub(crate) fn htpasswd_line(
    username: &str,
    password: &str,
    algo: &str,
    cost: i64,
) -> Result<String> {
    if username.is_empty() {
        bail!("set a username: a .htpasswd line is `username:hash` and needs both halves");
    }
    // The file format is line-based and colon-separated, so a username carrying
    // either would silently produce a broken or forged second entry.
    if username.contains([':', '\n', '\r']) {
        bail!("a username cannot contain a colon or a line break");
    }
    let hash = match algo {
        // ponytail: clamped to 4..=12 rather than bcrypt's full 4..=31. Cost 12
        // is already ~0.5s per hash and this runs on every keystroke.
        "bcrypt" => to_2y(&bcrypt_hash_with_rounds(password, cost.clamp(4, 12))?),
        "SHA-1" => sha1_entry(password)?,
        _ => bail!("algorithm must be one of {}", HTPASSWD_ALGOS.join(", ")),
    };
    Ok(format!("{username}:{hash}"))
}

/// Check a whole `.htpasswd` line, or a bare hash, against `password`.
pub(crate) fn htpasswd_check(line: &str, password: &str) -> Result<String> {
    let line = line.trim();
    if line.is_empty() {
        bail!("paste a .htpasswd line (or just its hash) to check a password against it");
    }
    // `user:hash` or a bare hash. A bcrypt hash has no colon in it, and neither
    // does `{SHA}...`, so the first colon is unambiguously the separator.
    let (user, hash) = match line.split_once(':') {
        Some((u, h)) => (Some(u), h.trim()),
        None => (None, line),
    };
    let matches = match hash {
        h if h.starts_with("{SHA}") => sha1_entry(password)? == h,
        h if h.starts_with("$2") => {
            // `bcrypt` only parses the $2b$/$2a$/$2x$ tags it writes.
            let h = h.replacen("$2y$", "$2b$", 1);
            bcrypt_verify(password, &h)?
        }
        h if h.starts_with("$apr1$") => {
            bail!("this is an MD5-crypt (APR1) hash, which this tool does not implement")
        }
        _ => bail!("unrecognised hash format: expected a bcrypt (`$2y$...`) or `{{SHA}}...` hash"),
    };
    Ok(match (matches, user) {
        (true, Some(u)) => format!("the password matches the entry for {u}"),
        (true, None) => "the password matches this hash".into(),
        (false, _) => "no: the password does not match".into(),
    })
}

pub(crate) fn run_htpasswd(a: &Args) -> Result<String> {
    let username = s(a, "username", "").trim();
    let check_line = s(a, "check_line", "");
    if username.is_empty() && check_line.trim().is_empty() {
        bail!(
            "enter a username and password to build a .htpasswd line, \
             or paste an existing line to check a password against it"
        );
    }
    Ok([
        section(
            "Entry",
            htpasswd_line(
                username,
                s(a, "password", ""),
                s(a, "algorithm", "bcrypt").trim(),
                n(a, "cost", 10),
            )
            .map(|l| format!("  {l}")),
        ),
        section(
            "Check an existing line",
            htpasswd_check(check_line, s(a, "check_password", "")).map(|r| format!("  {r}")),
        ),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "htpasswd-generator",
    name: "Htpasswd generator",
    description: "Build a .htpasswd entry for basic auth, or check a password against one.",
    category: Category::Crypto,
    keywords: &[
        "htpasswd", "apache", "nginx", "basic", "auth", "bcrypt", "password",
    ],
    fields: &[
        Field::text("username", "Username"),
        Field::text("password", "Password"),
        Field::select("algorithm", "Algorithm", HTPASSWD_ALGOS, "bcrypt"),
        Field::number("cost", "Bcrypt cost (4-12)", "10"),
        Field::text("check_line", "Existing .htpasswd line"),
        Field::text("check_password", "Password to check against it"),
    ],
    kind: Kind::Pure,
    run: run_htpasswd,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    // `htpasswd -bs foo bar` writes exactly this line.
    const SHA_LINE: &str = "foo:{SHA}Ys23Ag/5IOWqZCw9QGaVDdHwH00=";
    // A cost-4 bcrypt entry for the same password, in the $2y$ form `htpasswd
    // -bB -C 4` writes. Pinned so the check side is tested against a fixed
    // salt and not only against a hash this run produced.
    const BCRYPT_LINE: &str = "foo:$2y$04$5l5hEDQdTZ1/0FigYbnD2.oxQF.4CQJYtcDkTHUd6QyaQNUYEvfrq";

    #[test]
    fn sha1_entry_matches_htpasswd() {
        assert_eq!(
            htpasswd_line("foo", "bar", "SHA-1", 10).unwrap(),
            SHA_LINE,
            "the {{SHA}} entry is base64 of the raw SHA-1 digest"
        );
    }

    #[test]
    fn bcrypt_entry_round_trips() {
        let line = htpasswd_line("foo", "bar", "bcrypt", 4).unwrap();
        assert!(line.starts_with("foo:$2y$04$"), "{line}");
        assert_eq!(
            htpasswd_check(&line, "bar").unwrap(),
            "the password matches the entry for foo"
        );
        assert_eq!(
            htpasswd_check(&line, "nope").unwrap(),
            "no: the password does not match"
        );
    }

    #[test]
    fn checks_lines_written_by_htpasswd_itself() {
        for line in [SHA_LINE, BCRYPT_LINE] {
            assert!(
                htpasswd_check(line, "bar")
                    .unwrap()
                    .starts_with("the password matches")
            );
            assert!(htpasswd_check(line, "baz").unwrap().starts_with("no:"));
            // A bare hash, without the username, is accepted too.
            let (_, hash) = line.split_once(':').unwrap();
            assert_eq!(
                htpasswd_check(hash, "bar").unwrap(),
                "the password matches this hash"
            );
        }
    }

    #[test]
    fn rejects_what_it_cannot_answer() {
        assert!(run_htpasswd(&args(&[])).is_err());
        assert!(htpasswd_line("a:b", "x", "bcrypt", 4).is_err());
        assert!(htpasswd_line("", "x", "bcrypt", 4).is_err());
        assert!(htpasswd_line("a", "x", "MD5", 4).is_err());
        assert!(htpasswd_check("foo:$apr1$abc$def", "bar").is_err());
        assert!(htpasswd_check("foo:garbage", "bar").is_err());
        // One dead panel does not take the other down.
        let out = run_htpasswd(&args(&[("username", "foo"), ("check_line", "junk")])).unwrap();
        assert!(out.contains("Entry:\n  foo:$2y$"), "{out}");
        assert!(
            out.contains("Check an existing line:\n  (unrecognised"),
            "{out}"
        );
    }

    #[test]
    fn cost_is_clamped_so_the_ui_stays_responsive() {
        for cost in ["-1", "99", "one", ""] {
            let out = run_htpasswd(&args(&[
                ("username", "foo"),
                ("password", "bar"),
                ("cost", cost),
                ("algorithm", "SHA-1"),
            ]))
            .unwrap();
            assert!(out.contains(SHA_LINE), "{cost}: {out}");
        }
        // The clamp itself, on the one algorithm where it costs time.
        assert!(
            htpasswd_line("foo", "bar", "bcrypt", 9_999)
                .unwrap()
                .contains("$2y$12$")
        );
        assert!(
            htpasswd_line("foo", "bar", "bcrypt", -9_999)
                .unwrap()
                .contains("$2y$04$")
        );
    }

    #[test]
    fn hostile_input() {
        let big = "x".repeat(1 << 20);
        // bcrypt truncates at 72 bytes; a megabyte password must not panic.
        assert!(htpasswd_line("foo", &big, "bcrypt", 4).is_ok());
        assert!(htpasswd_line("foo", &big, "SHA-1", 4).is_ok());
        assert!(htpasswd_check(&big, "bar").is_err());
        assert!(htpasswd_line("foo", "\u{0}\u{fffd}\n", "SHA-1", 4).is_ok());
    }
}
