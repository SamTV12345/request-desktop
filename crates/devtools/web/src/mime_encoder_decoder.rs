//! `mime-encoder-decoder` — it-tools PR #930.
//!
//! The two encodings a mail header or an SMTP transcript is full of:
//! quoted-printable (RFC 2045) and encoded-words (RFC 2047).

use crate::*;
use anyhow::Result;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use std::fmt::Write as _;

/// Quoted-printable, with the 76-column soft line breaks the RFC asks for.
pub(crate) fn qp_encode(text: &str) -> String {
    let mut out = String::new();
    let mut column = 0;
    for byte in text.as_bytes() {
        let piece = match byte {
            // Printable ASCII except `=`, which always escapes.
            b'!'..=b'<' | b'>'..=b'~' | b' ' | b'\t' => (*byte as char).to_string(),
            b'\n' => {
                out.push_str("\r\n");
                column = 0;
                continue;
            }
            b'\r' => continue,
            _ => format!("={byte:02X}"),
        };
        if column + piece.len() > 75 {
            out.push_str("=\r\n");
            column = 0;
        }
        column += piece.len();
        out.push_str(&piece);
    }
    out
}

pub(crate) fn qp_decode(text: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '=' {
            let mut buf = [0u8; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        // `=` at end of line is a soft break: it disappears entirely.
        let hex: String = chars.clone().take(2).collect();
        if hex.starts_with(['\r', '\n']) {
            while matches!(chars.peek(), Some('\r' | '\n')) {
                chars.next();
            }
            continue;
        }
        match u8::from_str_radix(&hex, 16) {
            Ok(byte) => {
                chars.next();
                chars.next();
                bytes.push(byte);
            }
            Err(_) => bytes.push(b'='),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// RFC 2047 `=?UTF-8?B?…?=`, used for a Subject line that is not ASCII.
fn encoded_word(text: &str) -> String {
    if text.is_ascii() {
        return text.to_string();
    }
    format!("=?UTF-8?B?{}?=", STANDARD.encode(text))
}

/// Decode every encoded-word in a header, leaving the rest of the line alone.
pub(crate) fn decode_words(header: &str) -> String {
    let mut out = String::new();
    let mut rest = header;
    while let Some(start) = rest.find("=?") {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start + 2..].find("?=").map(|i| start + 2 + i) else {
            break;
        };
        let word = &rest[start + 2..end];
        // charset ? encoding ? payload
        let decoded = match word.splitn(3, '?').collect::<Vec<_>>()[..] {
            [_charset, enc, payload] if enc.eq_ignore_ascii_case("B") => STANDARD
                .decode(payload)
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned()),
            [_charset, enc, payload] if enc.eq_ignore_ascii_case("Q") => {
                // Q is quoted-printable with `_` standing in for a space.
                Some(qp_decode(&payload.replace('_', " ")))
            }
            _ => None,
        };
        match decoded {
            Some(text) => {
                out.push_str(&text);
                rest = &rest[end + 2..];
            }
            None => {
                out.push_str(&rest[start..end + 2]);
                rest = &rest[end + 2..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn mime(a: &Args) -> Result<String> {
    let text = s(a, "text", "");
    let encoded = s(a, "encoded", "");
    if text.is_empty() && encoded.is_empty() {
        anyhow::bail!("enter text to encode, or a quoted-printable/encoded-word string to decode");
    }
    let mut panels = Vec::new();
    if !text.is_empty() {
        let mut forms = String::new();
        let _ = writeln!(forms, "{}", qp_encode(text));
        panels.push(section("Quoted-printable", Ok(forms)));
        panels.push(section(
            "Encoded-word (header)",
            Ok(format!("{}\n", encoded_word(text))),
        ));
    }
    if !encoded.is_empty() {
        // One decoder handles both: an encoded-word carries its own encoding,
        // and anything left over is plain quoted-printable.
        panels.push(section(
            "Decoded",
            Ok(format!("{}\n", qp_decode(&decode_words(encoded)))),
        ));
    }
    Ok(panels.join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "mime-encoder-decoder",
    name: "MIME encoder/decoder",
    description: "Quoted-printable and RFC 2047 encoded-words — the two things a mail header is full of.",
    category: Category::Converter,
    keywords: &[
        "mime",
        "quoted-printable",
        "rfc2047",
        "email",
        "header",
        "subject",
        "encode",
    ],
    fields: &[
        Field::area("text", "Text"),
        Field::area("encoded", "Quoted-printable or =?UTF-8?B?…?="),
    ],
    kind: Kind::Pure,
    run: mime,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn quoted_printable_round_trips() {
        let out = ok("mime-encoder-decoder", &[("text", "Grüße = Preis 5€")]);
        assert!(
            out.contains("Gr=C3=BC=C3=9Fe =3D Preis 5=E2=82=AC"),
            "{out}"
        );
        assert_eq!(
            qp_decode(&qp_encode("Grüße = Preis 5€")),
            "Grüße = Preis 5€"
        );
        // Soft line breaks disappear on the way back.
        assert_eq!(qp_decode("a=\r\nb"), "ab");
        // A long line gets folded under 76 columns.
        let long = qp_encode(&"ä".repeat(40));
        assert!(long.lines().all(|l| l.len() <= 76), "{long}");
        assert_eq!(qp_decode(&long), "ä".repeat(40));
    }

    #[test]
    fn encoded_words_in_a_real_subject_line() {
        let out = ok("mime-encoder-decoder", &[("text", "Grüße")]);
        assert!(out.contains("=?UTF-8?B?R3LDvMOfZQ==?="), "{out}");
        // Both encodings decode, and the surrounding plain text survives.
        let subject = "Re: =?UTF-8?B?R3LDvMOfZQ==?= and =?utf-8?Q?caf=C3=A9_bar?= today";
        assert_eq!(decode_words(subject), "Re: Grüße and café bar today");
        let decoded = ok("mime-encoder-decoder", &[("encoded", subject)]);
        assert!(
            decoded.contains("Re: Grüße and café bar today"),
            "{decoded}"
        );
        // Something that only looks like an encoded word is left alone.
        assert_eq!(decode_words("a =?nonsense?= b"), "a =?nonsense?= b");
        assert!(run("mime-encoder-decoder", &[]).is_err());
    }
}
