//! `svg-placeholder-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

use base64::Engine as _;

pub(crate) fn svg_placeholder(args: &Args) -> anyhow::Result<String> {
    let id = "svg-placeholder-generator";
    let w = uint("width", a(args, id, "width"), 100_000)?;
    let h = uint("height", a(args, id, "height"), 100_000)?;
    let font_size = uint("font size", a(args, id, "font_size"), 10_000)?;
    if w == 0 || h == 0 || font_size == 0 {
        anyhow::bail!("width, height and font size must all be greater than 0");
    }
    let bg = a(args, id, "bg_color");
    let fg = a(args, id, "fg_color");
    for c in [bg, fg] {
        csscolorparser::parse(c).map_err(|e| anyhow::anyhow!("invalid color `{c}`: {e}"))?;
    }
    let custom = a(args, id, "text").trim().to_string();
    let text = if custom.is_empty() {
        format!("{w}x{h}")
    } else {
        custom
    };
    let size = if truthy(a(args, id, "exact_size")) {
        format!(r#" width="{w}" height="{h}""#)
    } else {
        String::new()
    };

    let svg = format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}"{size}>"#,
            "\n",
            r#"  <rect width="{w}" height="{h}" fill="{bg}"></rect>"#,
            "\n",
            r#"  <text x="50%" y="50%" dominant-baseline="middle" text-anchor="middle" font-family="monospace" font-size="{fs}px" fill="{fg}">{text}</text>"#,
            "\n</svg>",
        ),
        w = w,
        h = h,
        size = size,
        bg = bg,
        fg = fg,
        fs = font_size,
        text = html_escape::encode_text(&text),
    );
    // it-tools shows both the SVG element and its base64 data URI, side by side.
    let b64 = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
    Ok(format!(
        "{}\n\n{}",
        crate::section("SVG HTML element", Ok(svg)),
        crate::section(
            "SVG in Base64",
            Ok(format!("data:image/svg+xml;base64,{b64}")),
        ),
    ))
}

pub static TOOL: Tool = Tool {
    id: "svg-placeholder-generator",
    name: "SVG placeholder generator",
    description: "Generate SVG images to use as placeholders in your applications.",
    category: Category::Images,
    keywords: &["svg", "placeholder", "generator", "image", "mockup"],
    fields: &[
        Field::number("width", "Width (px)", "600"),
        Field::number("height", "Height (px)", "350"),
        Field::number("font_size", "Font size (px)", "26"),
        text_default("bg_color", "Background", "#cccccc"),
        text_default("fg_color", "Text color", "#333333"),
        Field::text("text", "Custom text"),
        Field::bool("exact_size", "Use exact size", "true"),
    ],
    kind: Kind::Pure,
    run: svg_placeholder,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    #[test]
    fn svg_placeholder_output() {
        let out = run(
            "svg-placeholder-generator",
            &[("bg_color", "#cccccc"), ("fg_color", "#333333")],
        )
        .unwrap();
        assert!(out.starts_with("SVG HTML element:\n<svg xmlns="), "{out}");
        assert!(out.contains(r#"viewBox="0 0 600 350""#));
        assert!(out.contains(r#"width="600" height="350""#));
        assert!(out.contains(">600x350</text>"));
        // Both panels, like upstream: the element and its base64 data URI.
        assert!(
            out.contains("SVG in Base64:\ndata:image/svg+xml;base64,PHN2ZyB4bWxucz0"),
            "{out}"
        );

        let out = run(
            "svg-placeholder-generator",
            &[
                ("bg_color", "#000"),
                ("fg_color", "#fff"),
                ("text", "a<b"),
                ("exact_size", "false"),
            ],
        )
        .unwrap();
        assert!(out.contains("a&lt;b"));
        assert!(
            !out.contains(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 350" width"#)
        );

        // The colour pickers upstream always hold a value, so the field defaults
        // do too and the tool renders with no arguments at all.
        assert!(
            run("svg-placeholder-generator", &[])
                .unwrap()
                .contains(r##"fill="#cccccc""##)
        );
        assert!(
            run(
                "svg-placeholder-generator",
                &[("bg_color", "#000"), ("fg_color", "#fff"), ("width", "0")]
            )
            .is_err()
        );
        assert!(
            run(
                "svg-placeholder-generator",
                &[("bg_color", "nope"), ("fg_color", "#fff")]
            )
            .is_err()
        );
    }
}
