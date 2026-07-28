//! Helpers shared by the tools in this crate.

use crate::*;
use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};

/// it-tools `convertHexToBin`: every hex digit becomes 4 bits.
pub(crate) fn convert_hex_to_bin(hex: &str) -> Result<String> {
    hex.trim()
        .chars()
        .map(|c| {
            let v = c
                .to_digit(16)
                .with_context(|| format!("not hexadecimal: {c:?}"))?;
            Ok(format!("{v:04b}"))
        })
        .collect()
}

pub(crate) fn encode_digest(bytes: &[u8], encoding: &str) -> Result<String> {
    let hex = hex_encode(bytes);
    Ok(match encoding {
        "Hex" => hex,
        "Bin" => convert_hex_to_bin(&hex)?,
        "Base64" => STANDARD.encode(bytes),
        "Base64url" => URL_SAFE_NO_PAD.encode(bytes),
        other => bail!("unknown encoding: {other}"),
    })
}

/// it-tools lists Bin first in the select, Hex is the default.
pub(crate) const ENCODINGS: &[&str] = &["Bin", "Hex", "Base64", "Base64url"];

/// How a *secret* is spelled in the form. "Text" is the historical behaviour
/// (the UTF-8 bytes of what was typed) and stays the default everywhere; the
/// other two exist because a binary key cannot be typed at all, and running it
/// through UTF-8 silently mangles every byte above 0x7f.
pub(crate) const KEY_ENCODINGS: &[&str] = &["Text", "Hexadecimal", "Base64"];

/// `what` names the field in the error, since one tool decodes both a key and
/// an IV with the same function.
pub(crate) fn decode_key(what: &str, text: &str, encoding: &str) -> Result<Vec<u8>> {
    // Whitespace is what you get from pasting a hexdump or a wrapped base64
    // blob, so drop it rather than reject the paste.
    let compact = || -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
    Ok(match encoding {
        "Text" => text.as_bytes().to_vec(),
        "Hexadecimal" => {
            hex_to_bytes(&compact()).with_context(|| format!("{what} is not hexadecimal"))?
        }
        "Base64" => STANDARD
            .decode(compact())
            .with_context(|| format!("{what} is not valid base64"))?,
        other => bail!("unknown key encoding: {other}"),
    })
}
/// Order of the rows in it-tools' hash-text card (`Object.keys(algos)`), with
/// the four FIPS SHA3 variants from PR #986 appended rather than interleaved —
/// a user comparing against the upstream card should find the old rows where
/// they were.
pub(crate) const HASH_ALGOS: &[&str] = &[
    "MD5",
    "SHA1",
    "SHA256",
    "SHA224",
    "SHA512",
    "SHA384",
    "SHA3",
    "RIPEMD160",
    "SHA3-224",
    "SHA3-256",
    "SHA3-384",
    "SHA3-512",
];
/// hmac-generator declares its own `algos` object, in a different order. The
/// FIPS variants follow the same appended-not-interleaved rule as `HASH_ALGOS`.
pub(crate) const HMAC_ALGOS: &[&str] = &[
    "MD5",
    "RIPEMD160",
    "SHA1",
    "SHA3",
    "SHA224",
    "SHA256",
    "SHA384",
    "SHA512",
    "SHA3-224",
    "SHA3-256",
    "SHA3-384",
    "SHA3-512",
];
