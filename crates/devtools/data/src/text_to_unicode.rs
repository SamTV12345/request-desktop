//! `text-to-unicode`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Result, bail};
use std::fmt::Write as _;

/// The `\uXXXX` code units of one character.
///
/// it-tools PRs #1534/#1094: the JS port iterated over UTF-16 code units, so an
/// emoji came out as two `\uD83D`/`\uDE00` halves *and* as two broken `&#…;`
/// entities. Rust iterates over code points, which fixes the entity panels for
/// free; the `\u` panels still need real surrogate pairs, because that is the
/// only way a four-hex-digit escape can spell a non-BMP character.
fn utf16_units(c: char) -> Vec<u16> {
    let mut buf = [0u16; 2];
    c.encode_utf16(&mut buf).to_vec()
}

/// Every escape notation this tool reads, in one pass.
///
/// The `\u{…}` branch has to precede the `\uXXXX` branch: the regex crate
/// matches alternations leftmost-first, and `\u{1F600}` would otherwise be
/// misread as an incomplete `\u` escape.
fn escape_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?i)&#(\d+);|&#x([0-9a-f]+);|\\u\{([0-9a-f]{1,6})\}|\\u([0-9a-f]{4})")
            .expect("static regex")
    })
}

/// Decode all four notations, re-joining surrogate pairs as they come.
///
/// A value that is not a character (a lone surrogate, or a number past
/// `0x10FFFF`) is dropped rather than reported, which is what the original
/// `&#…;`-only decoder did and what keeps a half-typed escape from blanking the
/// panel next to it.
fn decode_unicode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut cursor = 0usize;
    // A high surrogate waits here for its partner; anything else flushes it.
    let mut high: Option<u16> = None;
    for m in escape_re().captures_iter(input) {
        let whole = m.get(0).expect("group 0");
        out.push_str(&input[cursor..whole.start()]);
        cursor = whole.end();
        let value = if let Some(d) = m.get(1) {
            d.as_str().parse::<u32>().ok()
        } else {
            let hex = m.get(2).or_else(|| m.get(3)).or_else(|| m.get(4));
            hex.and_then(|h| u32::from_str_radix(h.as_str(), 16).ok())
        };
        let Some(value) = value else {
            high = None;
            continue;
        };
        match u16::try_from(value) {
            Ok(u @ 0xD800..=0xDBFF) => {
                high = Some(u);
                continue;
            }
            Ok(low @ 0xDC00..=0xDFFF) => {
                if let Some(h) = high.take()
                    && let Some(c) = char::decode_utf16([h, low]).next().and_then(Result::ok)
                {
                    out.push(c);
                }
                continue;
            }
            _ => high = None,
        }
        if let Some(c) = char::from_u32(value) {
            out.push(c);
        }
    }
    out.push_str(&input[cursor..]);
    out
}

/// Panels, as in it-tools: the text in every escape notation at once, plus the
/// one decoder that reads them all back.
pub(crate) fn text_to_unicode(args: &Args) -> Result<String> {
    let t = &TOOLS[21];
    let text = arg(args, t, "input");
    let unicode = arg(args, t, "unicode");
    let mut out = Vec::new();
    if !text.trim().is_empty() {
        // Decimal HTML entities: the original panel, one entity per code point.
        out.push(crate::section(
            "Unicode from your text",
            Ok(text.chars().map(|c| format!("&#{};", c as u32)).collect()),
        ));
        out.push(crate::section(
            "Hex entities from your text",
            Ok(text.chars().fold(String::new(), |mut s, c| {
                let _ = write!(s, "&#x{:X};", c as u32);
                s
            })),
        ));
        out.push(crate::section(
            "\\uXXXX escapes from your text",
            Ok(text.chars().fold(String::new(), |mut s, c| {
                for u in utf16_units(c) {
                    let _ = write!(s, "\\u{u:04x}");
                }
                s
            })),
        ));
        out.push(crate::section(
            "\\u{...} escapes from your text",
            Ok(text.chars().fold(String::new(), |mut s, c| {
                let _ = write!(s, "\\u{{{:X}}}", c as u32);
                s
            })),
        ));
        // Java's `native2ascii` form: printable ASCII is left alone and only
        // what a Latin-1 properties file cannot hold is escaped, in uppercase
        // hex. That is a different output from the panel above, not a restyling
        // of it, which is why it is its own panel.
        out.push(crate::section(
            "Java entities from your text",
            Ok(text.chars().fold(String::new(), |mut s, c| {
                if c.is_ascii_graphic() || c == ' ' {
                    s.push(c);
                } else {
                    for u in utf16_units(c) {
                        let _ = write!(s, "\\u{u:04X}");
                    }
                }
                s
            })),
        ));
    }
    if !unicode.trim().is_empty() {
        out.push(crate::section(
            "Text from your Unicode",
            Ok(decode_unicode(unicode)),
        ));
    }
    if out.is_empty() {
        bail!("no input");
    }
    Ok(out.join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "text-to-unicode",
    name: "Text to Unicode",
    description: "Convert text to Unicode entities and escapes, and back.",
    category: Category::Converter,
    keywords: &["text", "unicode", "entity", "escape", "emoji", "convert"],
    fields: &[
        Field::area("input", "Text to convert to unicode"),
        Field::area("unicode", "Unicode to convert to text"),
    ],
    kind: Kind::Pure,
    run: text_to_unicode,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    fn panel(pairs: &[(&str, &str)], label: &str) -> String {
        let out = ok("text-to-unicode", pairs);
        crate::split_sections(&out)
            .into_iter()
            .find(|(l, _)| *l == label)
            .unwrap_or_else(|| panic!("no {label:?} panel in:\n{out}"))
            .1
            .trim_end()
            .to_string()
    }

    #[test]
    fn text_to_unicode_vectors() {
        assert_eq!(panel(&[("input", "A")], "Unicode from your text"), "&#65;");
        assert_eq!(
            panel(
                &[("input", "linke the string convert to unicode")],
                "Unicode from your text"
            ),
            UNI
        );
        assert_eq!(
            ok("text-to-unicode", &[("unicode", "&#65;")]),
            "Text from your Unicode:\nA"
        );
        assert_eq!(
            ok("text-to-unicode", &[("unicode", UNI)]),
            "Text from your Unicode:\nlinke the string convert to unicode"
        );
        // both directions render together
        let out = ok("text-to-unicode", &[("input", "A"), ("unicode", "&#66;")]);
        assert!(out.starts_with("Unicode from your text:\n&#65;"), "{out}");
        assert!(out.ends_with("Text from your Unicode:\nB"), "{out}");
        assert!(run("text-to-unicode", &[("input", "")]).is_err());
        assert!(run("text-to-unicode", &[]).is_err());
    }

    /// PRs #1087/#1739: the same character in every notation the tool offers.
    #[test]
    fn text_to_unicode_extra_formats() {
        let p = |label: &str| panel(&[("input", "A€")], label);
        assert_eq!(p("Unicode from your text"), "&#65;&#8364;");
        assert_eq!(p("Hex entities from your text"), "&#x41;&#x20AC;");
        assert_eq!(p("\\uXXXX escapes from your text"), "\\u0041\\u20ac");
        assert_eq!(p("\\u{...} escapes from your text"), "\\u{41}\\u{20AC}");
        // native2ascii leaves printable ASCII alone.
        assert_eq!(p("Java entities from your text"), "A\\u20AC");
    }

    /// PRs #1534/#1094: an emoji is one code point, and only the four-hex-digit
    /// notations may split it — into a real surrogate pair, not into mojibake.
    #[test]
    fn text_to_unicode_handles_non_bmp() {
        let p = |label: &str| panel(&[("input", "😀")], label);
        assert_eq!(p("Unicode from your text"), "&#128512;");
        assert_eq!(p("Hex entities from your text"), "&#x1F600;");
        assert_eq!(p("\\uXXXX escapes from your text"), "\\ud83d\\ude00");
        assert_eq!(p("\\u{...} escapes from your text"), "\\u{1F600}");
        assert_eq!(p("Java entities from your text"), "\\uD83D\\uDE00");
        // and every one of them reads back
        for form in [
            "&#128512;",
            "&#x1F600;",
            "\\ud83d\\ude00",
            "\\uD83D\\uDE00",
            "\\u{1F600}",
        ] {
            assert_eq!(
                ok("text-to-unicode", &[("unicode", form)]),
                "Text from your Unicode:\n😀",
                "{form}"
            );
        }
    }

    #[test]
    fn text_to_unicode_decoder_is_forgiving() {
        // Text around the escapes survives.
        assert_eq!(
            ok("text-to-unicode", &[("unicode", "a\\u0062c&#100;")]),
            "Text from your Unicode:\nabcd"
        );
        // A lone high surrogate has no character to stand for, so it drops out
        // rather than turning the panel into an error.
        assert_eq!(
            ok("text-to-unicode", &[("unicode", "x\\ud83dy")]),
            "Text from your Unicode:\nxy"
        );
        // Out of range, same treatment as the original `&#…;`-only decoder.
        assert_eq!(
            ok("text-to-unicode", &[("unicode", "[&#99999999;]")]),
            "Text from your Unicode:\n[]"
        );
    }

    // ── integer-base-converter (vectors from the model test) ────────────
}
