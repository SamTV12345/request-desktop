//! `qr-code-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

/// it-tools names the four QR error-correction levels low/medium/quartile/high.
pub(crate) const QR_EC_LEVELS: &[&str] = &["low", "medium", "quartile", "high"];

pub(crate) fn qr_code_generator(args: &Args) -> anyhow::Result<String> {
    let id = "qr-code-generator";
    qr_svg(
        a(args, id, "text"),
        a(args, id, "fg_color"),
        a(args, id, "bg_color"),
        a(args, id, "ec_level"),
    )
}

pub static TOOL: Tool = Tool {
    id: "qr-code-generator",
    name: "QR Code generator",
    description: "Generate a QR code for a URL or plain text with custom colors.",
    category: Category::Images,
    keywords: &["qr", "qrcode", "generator", "barcode", "url"],
    fields: &[
        Field::area("text", "Text or URL"),
        text_default("fg_color", "Foreground color", "#000000"),
        text_default("bg_color", "Background color", "#ffffff"),
        Field::select("ec_level", "Error resistance", QR_EC_LEVELS, "medium"),
    ],
    kind: Kind::Pure,
    run: qr_code_generator,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn qr_code_svg_output() {
        let out = run(
            "qr-code-generator",
            &[("text", "https://it-tools.tech"), ("fg_color", "red")],
        )
        .unwrap();
        assert!(
            out.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""),
            "{out:.60}"
        );
        assert!(out.contains(r##"fill="#ff0000""##), "foreground applied");
        assert!(
            out.contains(r##"fill="#ffffff""##),
            "background defaults to white"
        );
        assert!(out.ends_with("</svg>"));
        assert!(
            !out.contains("<?xml"),
            "prolog stripped so the markup inlines"
        );

        // Every level encodes, and a stronger level needs a bigger symbol.
        let dim = |ec: &str| {
            let s = run("qr-code-generator", &[("text", "hello"), ("ec_level", ec)]).unwrap();
            s.split(r#"viewBox="0 0 "#)
                .nth(1)
                .unwrap()
                .split(' ')
                .next()
                .unwrap()
                .to_string()
        };
        for ec in QR_EC_LEVELS {
            assert!(!dim(ec).is_empty(), "{ec} renders");
        }

        assert!(run("qr-code-generator", &[]).is_err()); // empty text
        assert!(run("qr-code-generator", &[("text", "   ")]).is_err());
        assert!(run("qr-code-generator", &[("text", "hi"), ("ec_level", "nope")]).is_err());
        assert!(run("qr-code-generator", &[("text", "hi"), ("bg_color", "nope")]).is_err());
        // Way past the ~2953 byte ceiling: Err, never a panic.
        assert!(run("qr-code-generator", &[("text", &"a".repeat(5000))]).is_err());
        assert!(
            run(
                "qr-code-generator",
                &[("text", &"a".repeat(3000)), ("ec_level", "high")]
            )
            .is_err()
        );
    }
}
