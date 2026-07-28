//! `text-to-binary`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Result, bail};

/// it-tools PR #1085: the old code turned each `char` into eight bits, which is
/// simply wrong above U+00FF — `ä` became a 9-bit number and anything from a
/// non-Latin script produced nonsense. An octet is a byte, so the unit is the
/// UTF-8 byte, and the way back is `from_utf8` with a lossy fallback: a
/// half-typed octet stream is a normal state in this UI, not an error worth
/// blanking the panel for.
pub(crate) fn binary_to_text(input: &str) -> Result<String> {
    let bits: String = input.chars().filter(|c| *c == '0' || *c == '1').collect();
    if bits.is_empty() || !bits.len().is_multiple_of(8) {
        bail!("Binary should be a valid ASCII binary string with multiples of 8 bits");
    }
    let bytes: Vec<u8> = bits
        .as_bytes()
        .chunks(8)
        .map(|o| o.iter().fold(0u8, |a, b| a * 2 + (b - b'0')))
        .collect();
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Panels, as in it-tools: the text as octets in two radixes, and the way back.
pub(crate) fn text_to_binary(args: &Args) -> Result<String> {
    let t = &TOOLS[20];
    let text = arg(args, t, "input");
    let binary = arg(args, t, "binary");
    let mut out = Vec::new();
    if !text.trim().is_empty() {
        let sep = arg(args, t, "separator");
        let join =
            |f: fn(&u8) -> String| text.as_bytes().iter().map(f).collect::<Vec<_>>().join(sep);
        out.push(crate::section(
            "Binary from your text",
            Ok(join(|b| format!("{b:08b}"))),
        ));
        out.push(crate::section(
            "Hexadecimal from your text",
            Ok(join(|b| format!("{b:02x}"))),
        ));
    }
    if !binary.trim().is_empty() {
        out.push(crate::section(
            "Text from your binary",
            binary_to_text(binary),
        ));
    }
    if out.is_empty() {
        bail!("no input");
    }
    Ok(out.join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "text-to-binary",
    name: "Text to ASCII binary",
    description: "Convert text to its binary and hexadecimal UTF-8 octets, and back.",
    category: Category::Converter,
    keywords: &["text", "binary", "ascii", "hex", "utf8", "convert"],
    fields: &[
        Field::area("input", "Text to convert to binary"),
        Field {
            key: "separator",
            label: "Octet separator",
            kind: crate::FieldKind::Text,
            default: " ",
            options: &[],
        },
        Field::area("binary", "Binary to convert to text"),
    ],
    kind: Kind::Pure,
    run: text_to_binary,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    fn panel(pairs: &[(&str, &str)], label: &str) -> String {
        let out = ok("text-to-binary", pairs);
        crate::split_sections(&out)
            .into_iter()
            .find(|(l, _)| *l == label)
            .unwrap_or_else(|| panic!("no {label:?} panel in:\n{out}"))
            .1
            .trim_end()
            .to_string()
    }

    /// Both directions at once, as in it-tools — the `mode` select is gone.
    #[test]
    fn text_to_binary_vectors() {
        let enc = |pairs: &[(&str, &str)]| panel(pairs, "Binary from your text");
        let dec = |pairs: &[(&str, &str)]| panel(pairs, "Text from your binary");
        assert_eq!(enc(&[("input", "A"), ("separator", " ")]), "01000001");
        assert_eq!(
            enc(&[("input", "hello"), ("separator", " ")]),
            "01101000 01100101 01101100 01101100 01101111"
        );
        assert_eq!(
            enc(&[("input", "hello"), ("separator", "")]),
            "0110100001100101011011000110110001101111"
        );
        assert_eq!(
            dec(&[("binary", "01101000 01100101 01101100 01101100 01101111")]),
            "hello"
        );
        assert_eq!(dec(&[("binary", "01000001")]), "A");
        // input is cleaned before decoding
        assert_eq!(dec(&[("binary", "  01000 001garbage")]), "A");
        // a bad panel degrades to a note in that panel only
        let out = ok("text-to-binary", &[("input", "A"), ("binary", "010000011")]);
        assert!(out.contains("Binary from your text:\n01000001"), "{out}");
        assert!(out.contains("multiples of 8 bits"), "{out}");
        assert!(
            run("text-to-binary", &[("binary", "1")])
                .unwrap()
                .contains("multiples of 8 bits")
        );
        assert!(run("text-to-binary", &[("input", "")]).is_err());
        assert!(run("text-to-binary", &[]).is_err());
    }

    /// PR #1085: octets are UTF-8 bytes, so non-ASCII survives the round trip
    /// instead of being mangled into >8-bit "octets".
    #[test]
    fn text_to_binary_is_utf8_clean() {
        // `ä` is C3 A4, `😀` is F0 9F 98 80.
        assert_eq!(
            panel(&[("input", "ä")], "Binary from your text"),
            "11000011 10100100"
        );
        assert_eq!(
            panel(&[("input", "ä")], "Hexadecimal from your text"),
            "c3 a4"
        );
        assert_eq!(
            panel(&[("input", "😀")], "Hexadecimal from your text"),
            "f0 9f 98 80"
        );
        for text in ["ä", "😀", "Grüße, 世界"] {
            let bits = panel(&[("input", text)], "Binary from your text");
            assert_eq!(panel(&[("binary", &bits)], "Text from your binary"), text);
        }
        // Bytes that are not valid UTF-8 degrade to U+FFFD rather than erroring:
        // a paste is usually half-finished, not wrong.
        assert_eq!(
            panel(&[("binary", "11000011")], "Text from your binary"),
            "\u{fffd}"
        );
    }

    // ── text-to-unicode (vectors from text-to-unicode.service.test.ts) ──
}
