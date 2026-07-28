//! `otp-code-generator-and-validator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Context, Result, bail};

pub(crate) const BASE32_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// it-tools `base32toHex`. Quirk faithfully kept: the bit string is chunked in
/// 8s and a trailing partial chunk is parsed as-is, so `ABCDEF` -> `00443205`.
pub(crate) fn base32_to_hex(base32: &str) -> Result<String> {
    let mut bits = String::new();
    for c in base32.to_uppercase().trim_end_matches('=').chars() {
        let idx = BASE32_ALPHABET
            .iter()
            .position(|&b| b == c as u8)
            .with_context(|| format!("not a base32 character: {c:?}"))?;
        bits.push_str(&format!("{idx:05b}"));
    }
    let chars: Vec<char> = bits.chars().collect();
    Ok(chars
        .chunks(8)
        .map(|chunk| {
            let s: String = chunk.iter().collect();
            format!("{:02x}", u8::from_str_radix(&s, 2).unwrap_or(0))
        })
        .collect())
}

pub(crate) fn generate_hotp(key: &str, counter: u64) -> Result<String> {
    let secret = hex_to_bytes(&base32_to_hex(key)?)?;
    let digest = hmac_bytes("SHA1", &secret, &counter.to_be_bytes())?;
    let offset = (digest[19] & 0x0f) as usize;
    let v = ((digest[offset] as u32 & 0x7f) << 24)
        | ((digest[offset + 1] as u32) << 16)
        | ((digest[offset + 2] as u32) << 8)
        | (digest[offset + 3] as u32);
    Ok(format!("{:06}", v % 1_000_000))
}

pub(crate) fn counter_from_time(now_ms: i64, time_step: i64) -> u64 {
    (now_ms / 1000 / time_step).max(0) as u64
}

pub(crate) fn build_key_uri(
    secret: &str,
    app: &str,
    account: &str,
    digits: i64,
    period: i64,
) -> String {
    let e = |v: &str| urlencoding::encode(v).into_owned();
    format!(
        "otpauth://totp/{}:{}?issuer={}&secret={}&algorithm=SHA1&digits={digits}&period={period}",
        e(app),
        e(account),
        e(app),
        e(secret)
    )
}

/// The `.vue` has exactly one input — the secret — and no direction or mode
/// toggle: the time step is fixed at 30s, the digit count at 6, and there is no
/// HOTP counter and no "validate this code" box. Validation is visual: the
/// previous / current / next codes are shown side by side.
///
/// Not reproducible in a one-shot pure function, and deliberately absent:
///  * the progress bar and "Next in NNs" countdown, which re-render on a timer;
///  * the QR code image of the key URI (needs a raster/canvas target — the
///    key URI itself is emitted so it can be pasted into an authenticator).
///
/// `timestamp_ms` is not an it-tools field; it exists so the clock can be
/// pinned in tests. Empty (the default) means now, as in the UI.
pub(crate) const OTP_TIME_STEP: i64 = 30;

pub(crate) fn run_otp(a: &Args) -> Result<String> {
    let secret = s(a, "secret", "").trim().to_string();
    if secret.is_empty() {
        bail!("please set a secret (a base32 string)");
    }
    let now_ms = match s(a, "timestamp_ms", "").trim() {
        "" => chrono::Utc::now().timestamp_millis(),
        v => v
            .parse()
            .context("timestamp must be a number of milliseconds")?,
    };
    let counter = counter_from_time(now_ms, OTP_TIME_STEP);
    let step_ms = OTP_TIME_STEP * 1000;
    let code = |ms: i64| generate_hotp(&secret, counter_from_time(ms, OTP_TIME_STEP));
    let codes = || -> Result<String> {
        Ok(format!(
            "  previous: {}\n  current:  {}\n  next:     {}",
            code(now_ms - step_ms)?,
            code(now_ms)?,
            code(now_ms + step_ms)?
        ))
    };
    Ok([
        crate::section("Codes", codes()),
        crate::section("Secret in hexadecimal", base32_to_hex(&secret)),
        crate::section("Epoch", Ok(format!("  {}", now_ms / 1000))),
        crate::section("Iteration count", Ok(format!("  {counter}"))),
        crate::section(
            "Iteration count, padded hex",
            Ok(format!("  {counter:016x}")),
        ),
        crate::section(
            "Key URI",
            Ok(format!(
                "  {}",
                build_key_uri(&secret, "IT-Tools", "demo-user", 6, OTP_TIME_STEP)
            )),
        ),
    ]
    .join("\n"))
}

pub static TOOL: Tool = Tool {
    id: "otp-code-generator-and-validator",
    name: "OTP code generator and validator",
    description: "Generate and validate time-based OTP (TOTP) codes from a base32 secret.",
    category: Category::Crypto,
    keywords: &["otp", "totp", "hotp", "2fa", "mfa", "authenticator", "code"],
    fields: &[
        Field::text("secret", "Secret"),
        Field::text("timestamp_ms", "Timestamp in ms (empty = now)"),
    ],
    kind: Kind::Pure,
    run: run_otp,
};
