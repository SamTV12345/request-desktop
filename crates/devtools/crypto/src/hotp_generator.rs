//! `hotp-generator` — it-tools PR #1515.
//!
//! The counter-based half of RFC 4226. The ported `otp-code-generator-and-
//! validator` is its time-based cousin and hard-wires SHA1, six digits and a
//! counter derived from the clock; hardware tokens, `oathtool -c` and Yubikey
//! OATH slots need the counter typed in and often other digest sizes.
//!
//! Base32 decoding is not re-implemented: `base32_to_hex` from the TOTP tool is
//! reused through the crate prelude, quirks and all, so both tools accept
//! exactly the same secrets.

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, section};
use anyhow::{Result, bail};

/// RFC 4226 appendix D's secret, base32-encoded — so the tool shows a code that
/// can be checked against the RFC the moment it is opened.
pub(crate) const HOTP_DEMO_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

/// How many counters ahead of the typed one to show.
///
/// RFC 4226 section 7.4 recommends a server look-ahead window of s = 10 for
/// resynchronising a token whose counter has run away. Showing that window is
/// what lets you find out how far a token has drifted.
pub(crate) const HOTP_LOOK_AHEAD: u64 = 10;

pub(crate) const HOTP_DIGITS: &[&str] = &["6", "7", "8"];
pub(crate) const HOTP_ALGOS: &[&str] = &["SHA1", "SHA256", "SHA512"];

/// RFC 4226 section 5.3: HMAC, dynamic truncation, modulo 10^digits.
pub(crate) fn hotp_code(secret: &[u8], counter: u64, digits: u32, algo: &str) -> Result<String> {
    let digest = hmac_bytes(algo, secret, &counter.to_be_bytes())?;
    // The low nibble of the last byte picks the 4-byte window. Every digest
    // here is at least 20 bytes, so offset+3 <= 18 is always in range.
    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let v = u32::from_be_bytes([
        digest[offset] & 0x7f,
        digest[offset + 1],
        digest[offset + 2],
        digest[offset + 3],
    ]);
    let width = digits as usize;
    Ok(format!("{:0width$}", v % 10u32.pow(digits), width = width))
}

fn hotp_secret(a: &Args) -> Result<Vec<u8>> {
    let secret = s(a, "secret", HOTP_DEMO_SECRET).trim().to_string();
    if secret.is_empty() {
        bail!("please set a secret (the base32 string your token or provider gave you)");
    }
    hex_to_bytes(&base32_to_hex(&secret)?)
}

pub(crate) fn run_hotp(a: &Args) -> Result<String> {
    let secret = hotp_secret(a)?;
    let algo = s(a, "algorithm", "SHA1").trim();
    if !HOTP_ALGOS.contains(&algo) {
        bail!("algorithm must be one of {}", HOTP_ALGOS.join(", "));
    }
    // A counter is a 64 bit unsigned quantity; anything unparseable or negative
    // is a typo mid-edit, so fall back to 0 rather than refusing to render.
    let counter = n(a, "counter", 0).max(0) as u64;
    let digits: u32 = match s(a, "digits", "6").trim() {
        "6" => 6,
        "7" => 7,
        "8" => 8,
        other => bail!("digits must be 6, 7 or 8, got {other:?}"),
    };
    let code = |c: u64| hotp_code(&secret, c, digits, algo);
    let window = || -> Result<String> {
        // saturating_add: at u64::MAX the window is a single repeated counter
        // rather than a wrap back to 0, which would print a wrong code.
        (1..=HOTP_LOOK_AHEAD)
            .map(|i| {
                let c = counter.saturating_add(i);
                Ok(format!("  {c:>20}  {}", code(c)?))
            })
            .collect::<Result<Vec<_>>>()
            .map(|rows| rows.join("\n"))
    };
    Ok([
        section("Code", code(counter).map(|c| format!("  {c}"))),
        section("Counter", Ok(format!("  {counter}  ({counter:016x})"))),
        section("Look-ahead window (counter, code)", window()),
        section(
            "Secret in hexadecimal",
            Ok(format!("  {}", hex_encode(&secret))),
        ),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "hotp-generator",
    name: "HOTP generator",
    description: "Generate counter-based one-time passwords (RFC 4226) from a base32 secret.",
    category: Category::Crypto,
    keywords: &["hotp", "otp", "rfc4226", "2fa", "mfa", "counter", "token"],
    fields: &[
        Field::text("secret", "Secret (base32)").with_default(HOTP_DEMO_SECRET),
        Field::number("counter", "Counter", "0"),
        Field::select("digits", "Digits", HOTP_DIGITS, "6"),
        Field::select("algorithm", "Algorithm", HOTP_ALGOS, "SHA1"),
    ],
    kind: Kind::Pure,
    run: run_hotp,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    /// RFC 4226 appendix D: secret "12345678901234567890", counters 0..9.
    const RFC4226: [&str; 10] = [
        "755224", "287082", "359152", "969429", "338314", "254676", "287922", "162583", "399871",
        "520489",
    ];

    #[test]
    fn rfc4226_appendix_d() {
        assert_eq!(
            base32_to_hex(HOTP_DEMO_SECRET).unwrap(),
            hex_encode(b"12345678901234567890")
        );
        for (counter, expected) in RFC4226.iter().enumerate() {
            assert_eq!(
                &hotp_code(b"12345678901234567890", counter as u64, 6, "SHA1").unwrap(),
                expected
            );
        }
    }

    #[test]
    fn tool_shows_the_code_and_the_window() {
        let out = run_hotp(&args(&[("counter", "3")])).unwrap();
        assert!(out.contains(&format!("Code:\n  {}", RFC4226[3])), "{out}");
        assert!(out.contains("Counter:\n  3  (0000000000000003)"), "{out}");
        // The window starts at counter+1 and covers the RFC's resync range.
        assert!(out.contains(&format!("4  {}", RFC4226[4])), "{out}");
        assert!(out.contains(&format!("9  {}", RFC4226[9])), "{out}");
        // ...and ends ten counters on, at 13.
        let window = out.split("(counter, code):\n").nth(1).unwrap();
        let rows: Vec<&str> = window.lines().take_while(|l| !l.is_empty()).collect();
        assert_eq!(rows.len(), 10);
        assert_eq!(rows[9].split_whitespace().next(), Some("13"));
        assert!(
            out.contains("Secret in hexadecimal:\n  3132333435363738393031323334353637383930"),
            "{out}"
        );
    }

    #[test]
    fn digits_and_algorithm() {
        // Same HMAC, wider truncation: the 6-digit code is the tail of the
        // 8-digit one, which is what RFC 4226's table shows.
        let eight = hotp_code(b"12345678901234567890", 0, 8, "SHA1").unwrap();
        assert_eq!(eight, "84755224");
        assert_eq!(
            hotp_code(b"12345678901234567890", 0, 7, "SHA1").unwrap(),
            "4755224"
        );
        // RFC 6238 appendix B shares the truncation step; these are its digests
        // applied at counter 1, i.e. plain HOTP with a longer hash.
        assert_ne!(
            hotp_code(b"12345678901234567890", 0, 6, "SHA256").unwrap(),
            RFC4226[0]
        );
        assert!(run_hotp(&args(&[("digits", "9")])).is_err());
        assert!(run_hotp(&args(&[("algorithm", "MD5")])).is_err());
    }

    #[test]
    fn hostile_input() {
        // No secret at all is the one case where nothing is computable.
        assert!(run_hotp(&args(&[("secret", "  ")])).is_err());
        assert!(run_hotp(&args(&[("secret", "not!base32")])).is_err());
        // Garbage and out-of-range counters fall back to 0 instead of panicking.
        for c in ["-5", "99999999999999999999999", "", "one"] {
            let out = run_hotp(&args(&[("counter", c)])).unwrap();
            assert!(
                out.contains(&format!("Code:\n  {}", RFC4226[0])),
                "{c}: {out}"
            );
        }
        // The largest counter that parses still renders a full window.
        let out = run_hotp(&args(&[("counter", &i64::MAX.to_string())])).unwrap();
        assert!(out.contains("  9223372036854775808  "), "{out}");
        // A megabyte of base32 is decoded, not choked on.
        let big = "A".repeat(1 << 20);
        assert!(run_hotp(&args(&[("secret", &big)])).is_ok());
    }
}
