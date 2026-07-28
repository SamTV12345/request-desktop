//! `url-encoder`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg, section};
use anyhow::{Context, Result, bail};

/// JS `encodeURIComponent` leaves `-_.!~*'()` alone on top of alphanumerics.
pub(crate) const URI_COMPONENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

/// JS `encodeURI`: `encodeURIComponent`'s set plus the reserved characters that
/// make up a URL's structure (`;,/?:@&=+$` and `#`), left alone so an already
/// assembled URL survives a pass through it.
pub(crate) const URI_FULL: &percent_encoding::AsciiSet = &URI_COMPONENT
    .remove(b';')
    .remove(b',')
    .remove(b'/')
    .remove(b'?')
    .remove(b':')
    .remove(b'@')
    .remove(b'&')
    .remove(b'=')
    .remove(b'+')
    .remove(b'$')
    .remove(b'#');

/// RFC 3986 unreserved: `ALPHA / DIGIT / "-" / "." / "_" / "~"`. Stricter than
/// `encodeURIComponent`, which also spares `!*'()` — those are sub-delims and a
/// few servers do treat them as syntax, which is the reason this option exists.
pub(crate) const RFC3986: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// The three sets, by the name of the JavaScript function they match.
pub(crate) const CHARSETS: &[&str] = &["encodeURIComponent", "encodeURI", "RFC 3986"];

/// `decodeURIComponent` semantics: `percent_decode_str` is lenient about `%zz`,
/// JS throws.
pub(crate) fn decode_uri_component(input: &str) -> Result<String> {
    let bytes = input.as_bytes();
    for (i, _) in input.char_indices().filter(|(_, c)| *c == '%') {
        let ok = bytes
            .get(i + 1..i + 3)
            .is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit));
        if !ok {
            bail!("Impossible to parse this string");
        }
    }
    percent_encoding::percent_decode_str(input)
        .decode_utf8()
        .map(|s| s.into_owned())
        .context("Impossible to parse this string")
}

/// it-tools shows an "Encode" and a "Decode" card side by side — no mode toggle.
///
/// it-tools PR #1474 adds the character-set choice. It is a select over an
/// *encoding rule*, not a direction: both cards are still computed on every call
/// and both honour the same rule, so a round trip through the tool stays a round
/// trip.
pub(crate) fn url_encoder(args: &Args) -> Result<String> {
    let t = tool("url-encoder");
    let set = match arg(args, t, "charset") {
        "encodeURI" => URI_FULL,
        "RFC 3986" => RFC3986,
        _ => URI_COMPONENT,
    };
    let plus = b(args, "plus_for_space", false);
    let mut encoded =
        percent_encoding::utf8_percent_encode(arg(args, t, "encode_input"), set).to_string();
    if plus {
        // application/x-www-form-urlencoded. `%20` is the only spelling of a
        // space any of the three sets produces, so this cannot hit a literal
        // percent triple that meant something else.
        encoded = encoded.replace("%20", "+");
    }
    let raw = arg(args, t, "decode_input");
    let decoded = if plus {
        // A literal plus in form encoding is `%2B`, so a bare `+` is always a
        // space; rewriting it before decoding keeps `%2B` intact.
        decode_uri_component(&raw.replace('+', "%20"))
    } else {
        decode_uri_component(raw)
    };
    Ok(format!(
        "{}\n{}\n",
        section("Encoded", Ok(encoded)),
        section("Decoded", decoded),
    ))
}

pub static TOOL: Tool = Tool {
    id: "url-encoder",
    name: "Encode/decode url formatted strings",
    description: "Encode to url-encoded format (also known as percent-encoded) or decode from it.",
    category: Category::Web,
    keywords: &["url", "encode", "decode", "percent", "%20", "format"],
    fields: &[
        Field {
            key: "encode_input",
            label: "Your string",
            kind: crate::FieldKind::TextArea,
            default: "Hello world :)",
            options: &[],
        },
        Field {
            key: "decode_input",
            label: "Your encoded string",
            kind: crate::FieldKind::TextArea,
            default: "Hello%20world%20%3A)",
            options: &[],
        },
        Field::select("charset", "Character set", CHARSETS, CHARSETS[0]),
        Field::bool("plus_for_space", "`+` for space (form encoding)", "false"),
    ],
    kind: Kind::Pure,
    run: url_encoder,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    #[test]
    fn charset_and_form_encoding() {
        let enc = |s: &str, charset: &str, plus: &str| {
            let out = ok(
                "url-encoder",
                &[
                    ("encode_input", s),
                    ("decode_input", ""),
                    ("charset", charset),
                    ("plus_for_space", plus),
                ],
            );
            out.lines().nth(1).unwrap_or_default().to_string()
        };
        // Default is the old behaviour, byte for byte.
        assert_eq!(
            enc("a b/c!*'()~", "encodeURIComponent", "false"),
            "a%20b%2Fc!*'()~"
        );
        assert_eq!(enc("a b/c!*'()~", "encodeURI", "false"), "a%20b/c!*'()~");
        assert_eq!(
            enc("a b/c!*'()~", "RFC 3986", "false"),
            "a%20b%2Fc%21%2A%27%28%29~"
        );
        assert_eq!(enc("a b", "encodeURIComponent", "true"), "a+b");

        // Decoding follows the same rule: `+` is a space only in form mode, and
        // `%2B` survives either way.
        let dec = |s: &str, plus: &str| {
            let out = ok(
                "url-encoder",
                &[
                    ("encode_input", ""),
                    ("decode_input", s),
                    ("charset", "encodeURIComponent"),
                    ("plus_for_space", plus),
                ],
            );
            out.lines().last().unwrap_or_default().to_string()
        };
        assert_eq!(dec("a+b%2Bc", "false"), "a+b+c");
        assert_eq!(dec("a+b%2Bc", "true"), "a b+c");
    }
}
