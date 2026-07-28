//! `base64-string-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose as b64;

/// Decode one base64 string the way it-tools' `base64ToText` does.
///
/// Two deliberate points of behaviour, both copied from js-base64:
/// * the `data:<mime>;base64,` prefix is stripped first;
/// * the result is *lossy* UTF-8. `TESTtest` is perfectly good base64 over six
///   binary bytes, and it-tools shows those bytes rather than refusing — a hard
///   `from_utf8` error here was the original bug report.
///
/// `urlSafe` is honoured rather than guessed at: on, `-_` are translated and
/// every other stray character dropped (js-base64's `unURI`); off, only
/// whitespace is removed, so a `-` really is a decode error.
pub(crate) fn b64_decode(raw: &str, url_safe: bool) -> Result<String> {
    let s = raw.trim();
    let s = s
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(";base64,"))
        .map_or(s, |(_, body)| body);
    let cleaned: String = if url_safe {
        s.chars()
            .map(|c| match c {
                '-' => '+',
                '_' => '/',
                c => c,
            })
            .filter(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '/')
            .collect()
    } else {
        s.split_whitespace().collect()
    };
    if cleaned.is_empty() {
        bail!("no base64 input");
    }
    let bytes = b64::STANDARD_NO_PAD_INDIFFERENT
        .decode(&cleaned)
        .context("Invalid base64 string")?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// it-tools renders "String to base64" and "Base64 to string" side by side, each
/// with its own URL-safe switch — there is no mode to be in the wrong one of.
pub(crate) fn base64_string(args: &Args) -> Result<String> {
    let t = &TOOLS[0];
    let text = arg(args, t, "input");
    let b64_in = arg(args, t, "base64");
    let mut out = Vec::new();
    if !text.is_empty() {
        let encoded = if flag(args, t, "encodeUrlSafe") {
            b64::URL_SAFE_NO_PAD.encode(text)
        } else {
            b64::STANDARD.encode(text)
        };
        out.push(crate::section("Base64 of string", Ok(encoded)));
    }
    if !b64_in.trim().is_empty() {
        out.push(crate::section(
            "Decoded string",
            b64_decode(b64_in, flag(args, t, "decodeUrlSafe")),
        ));
    }
    if out.is_empty() {
        bail!("no input");
    }
    Ok(out.join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "base64-string-converter",
    name: "Base64 string encoder/decoder",
    description: "Encode text to base64 or decode a base64 string back to text.",
    category: Category::Converter,
    keywords: &["base64", "encode", "decode", "string"],
    fields: &[
        Field::area("input", "String to encode"),
        Field::bool("encodeUrlSafe", "Encode URL safe", "false"),
        Field::area("base64", "Base64 string to decode"),
        Field::bool("decodeUrlSafe", "Decode URL safe", "false"),
    ],
    kind: Kind::Pure,
    run: base64_string,
};
