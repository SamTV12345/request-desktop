//! `hex-base64-converter` — it-tools PRs #1530, #1525.
//!
//! Two PRs asked for hex→base64 and base64→hex. Both are the same thing: one
//! byte string with three ways of writing it down. So there is one field per
//! notation, and whichever you fill shows you the other two — no direction to
//! pick, and no dead end when you paste hex into the base64 box.

use crate::*;
use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose as b64;

const SAMPLE: &str = "Hello, World!";

/// Read hex the way it is actually pasted: `48 65 6c`, `48:65:6C`,
/// `0x48, 0x65`, or one unbroken run. Separators are dropped, a `0x` prefix on
/// any token is dropped, and what remains must be whole bytes.
fn hex_bytes(raw: &str) -> Result<Vec<u8>> {
    let cleaned: String = raw
        .split(|c: char| c.is_whitespace() || matches!(c, ':' | ',' | '-' | '_'))
        .filter(|t| !t.is_empty())
        .map(|t| {
            t.strip_prefix("0x")
                .or_else(|| t.strip_prefix("0X"))
                .unwrap_or(t)
        })
        .collect();
    if cleaned.is_empty() {
        bail!("no hexadecimal digits");
    }
    if !cleaned.len().is_multiple_of(2) {
        bail!(
            "{} hex digits is half a byte short — add a leading zero",
            cleaned.len()
        );
    }
    hex_to_bytes(&cleaned)
}

/// Padding is optional (`INDIFFERENT`), because half the world strips it.
fn b64_bytes(raw: &str) -> Result<Vec<u8>> {
    let cleaned: String = raw.split_whitespace().collect();
    if cleaned.is_empty() {
        bail!("no base64 input");
    }
    b64::STANDARD_NO_PAD_INDIFFERENT
        .decode(&cleaned)
        .context("Invalid base64 string")
}

/// Bytes that are not UTF-8 are shown lossily rather than refused: someone
/// converting a hash between notations still wants the hex and the base64.
fn as_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn convert(a: &Args) -> Result<String> {
    let text = s(a, "text", SAMPLE);
    let hex = s(a, "hex", "");
    let base64 = s(a, "base64", "");
    let mut out = Vec::new();
    if !text.is_empty() {
        let b = text.as_bytes();
        out.push(section(
            "From text",
            Ok(format!(
                "Hex: {}\nBase64: {}\nBytes: {}",
                hex_encode(b),
                b64::STANDARD.encode(b),
                b.len()
            )),
        ));
    }
    if !hex.trim().is_empty() {
        out.push(section(
            "From hex",
            hex_bytes(hex).map(|b| {
                format!(
                    "Text: {}\nBase64: {}\nBytes: {}",
                    as_text(&b),
                    b64::STANDARD.encode(&b),
                    b.len()
                )
            }),
        ));
    }
    if !base64.trim().is_empty() {
        out.push(section(
            "From base64",
            b64_bytes(base64).map(|b| {
                format!(
                    "Text: {}\nHex: {}\nBytes: {}",
                    as_text(&b),
                    hex_encode(&b),
                    b.len()
                )
            }),
        ));
    }
    if out.is_empty() {
        bail!("fill in one of the three fields — text, hex or base64 — and the other two follow");
    }
    Ok(out.join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "hex-base64-converter",
    name: "Text, hex and base64",
    description: "One value in three notations: fill in any of text, hex or base64.",
    category: Category::Converter,
    keywords: &[
        "hex",
        "hexadecimal",
        "base64",
        "bytes",
        "text",
        "encode",
        "decode",
    ],
    fields: &[
        Field::area("text", "Text").with_default(SAMPLE),
        Field::area("hex", "Hexadecimal"),
        Field::area("base64", "Base64"),
    ],
    kind: Kind::Pure,
    run: convert,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn text_is_prefilled_and_shows_the_other_two() {
        assert_eq!(
            ok("hex-base64-converter", &[]),
            "From text:\nHex: 48656c6c6f2c20576f726c6421\nBase64: SGVsbG8sIFdvcmxkIQ==\nBytes: 13"
        );
    }

    #[test]
    fn hex_is_read_however_it_was_pasted() {
        let want = "From hex:\nText: Hi!\nBase64: SGkh\nBytes: 3";
        for form in [
            "486921",
            "48 69 21",
            "48:69:21",
            "0x48 0x69 0x21",
            "0x48,0x69,0x21",
            "48-69-21",
            "  48\n69\n21  ",
        ] {
            assert_eq!(
                ok("hex-base64-converter", &[("text", ""), ("hex", form)]),
                want,
                "{form:?}"
            );
        }
        // Uppercase digits too.
        assert!(
            ok("hex-base64-converter", &[("text", ""), ("hex", "48692A")]).contains("Bytes: 3")
        );
    }

    #[test]
    fn base64_with_and_without_padding() {
        for form in ["SGkh", "SGkh\n", "SGkhAA==", "SGkhAA"] {
            let out = ok("hex-base64-converter", &[("text", ""), ("base64", form)]);
            assert!(
                out.starts_with("From base64:\nText: Hi!"),
                "{form:?}: {out}"
            );
        }
        assert!(
            ok("hex-base64-converter", &[("text", ""), ("base64", "SGkh")]).contains("Hex: 486921")
        );
    }

    #[test]
    fn non_utf8_bytes_are_shown_lossily_rather_than_refused() {
        let out = ok("hex-base64-converter", &[("text", ""), ("hex", "ff fe 00")]);
        assert!(out.contains("Base64: //4A"), "{out}");
        assert!(out.contains("Bytes: 3"), "{out}");
        assert!(out.contains('\u{fffd}'), "{out}");
    }

    #[test]
    fn all_three_at_once_and_a_bad_one_stays_local() {
        let out = ok(
            "hex-base64-converter",
            &[("text", "a"), ("hex", "zz"), ("base64", "SGkh")],
        );
        assert!(out.contains("From text:\nHex: 61"), "{out}");
        assert!(out.contains("not hexadecimal"), "{out}");
        assert!(out.contains("From base64:\nText: Hi!"), "{out}");
        // An odd digit count says what to do about it.
        let odd = ok("hex-base64-converter", &[("text", ""), ("hex", "486")]);
        assert!(odd.contains("half a byte short"), "{odd}");
        // Nothing typed anywhere is the only hard error.
        assert!(
            run(
                "hex-base64-converter",
                &[("text", ""), ("hex", ""), ("base64", "")]
            )
            .is_err()
        );
    }

    #[test]
    fn survives_hostile_input() {
        let mb = "x".repeat(1_000_000);
        assert!(ok("hex-base64-converter", &[("text", &mb)]).contains("Bytes: 1000000"));
        let hex = "41".repeat(500_000);
        assert!(
            ok("hex-base64-converter", &[("text", ""), ("hex", &hex)]).contains("Bytes: 500000")
        );
        // Garbage of every shape errors in its panel instead of panicking.
        for junk in ["{".repeat(10_000), "9".repeat(1_000_000), "0x".to_string()] {
            let out = ok(
                "hex-base64-converter",
                &[("text", ""), ("hex", &junk), ("base64", &junk)],
            );
            assert!(out.contains("From hex:"), "{out}");
        }
    }
}
