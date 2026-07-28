//! `color-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

pub(crate) fn remove_alpha_channel_when_opaque(hex: &str) -> String {
    re(r"(?i)^(#(?:[0-9a-f]{3}){1,2})ff$")
        .replace(hex, "$1")
        .into_owned()
}

/// The nearest CSS colour name and how far off it is, as a plain RGB distance.
///
/// it-tools' `toName({ closest: true })` always answers; csscolorparser only
/// knows exact matches, but it exports the 148-entry table, so the scan is four
/// lines. `NAMED_COLORS` is ordered, so a tie (aqua/cyan, grey/gray) always
/// resolves the same way.
pub(crate) fn nearest_css_name(rgb: [u8; 3]) -> (&'static str, f64) {
    let mut best = ("black", f64::INFINITY);
    for (name, c) in csscolorparser::NAMED_COLORS.entries() {
        let d: f64 = (0..3)
            .map(|i| (rgb[i] as f64 - c[i] as f64).powi(2))
            .sum::<f64>()
            .sqrt();
        if d < best.1 {
            best = (name.as_str(), d);
        }
    }
    best
}

/// WCAG 2.1 relative luminance of an opaque sRGB colour.
pub(crate) fn relative_luminance(rgb: [u8; 3]) -> f64 {
    let lin = |c: u8| {
        let c = c as f64 / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(rgb[0]) + 0.7152 * lin(rgb[1]) + 0.0722 * lin(rgb[2])
}

/// WCAG contrast ratio, 1:1 to 21:1.
pub(crate) fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The ratio plus the highest WCAG bar it clears, which is the answer people
/// actually open this tool for: "can I put this text on that background".
pub(crate) fn contrast_row(fg: [u8; 3], bg: [u8; 3]) -> String {
    let ratio = contrast_ratio(fg, bg);
    let grade = match ratio {
        r if r >= 7.0 => "AAA",
        r if r >= 4.5 => "AA",
        r if r >= 3.0 => "AA for large text only",
        _ => "fails WCAG",
    };
    format!("{}:1 ({grade})", fmt_f((ratio * 100.0).round() / 100.0))
}

pub(crate) fn color_converter(args: &Args) -> anyhow::Result<String> {
    let input = nonempty("color", a(args, "color-converter", "color"))?.trim();
    let c = csscolorparser::parse(input).map_err(|e| anyhow::anyhow!("invalid color: {e}"))?;
    let [r, g, b, _] = c.to_rgba8();

    // cmyk is not in csscolorparser; the formula is three lines.
    let (rf, gf, bf) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let k = 1.0 - rf.max(gf).max(bf);
    let comp = |v: f64| {
        if k >= 1.0 {
            0.0
        } else {
            (1.0 - v - k) / (1.0 - k) * 100.0
        }
    };

    // Row order is the `.vue`'s `formats` record: hex, rgb, hsl, hwb, lch, cmyk,
    // name (its first entry is the colour-picker widget, which has no text form).
    // `oklch` is ours: csscolorparser gives it for free and it costs a row. The
    // two contrast rows are it-tools PR #1017's, and are the reason the tool
    // gets opened in the first place.
    Ok([
        row("hex", Ok(remove_alpha_channel_when_opaque(&c.to_css_hex()))),
        row("rgb", Ok(c.to_css_rgb())),
        row("hsl", Ok(c.to_css_hsl())),
        row("hwb", Ok(c.to_css_hwb())),
        row("lch", Ok(c.to_css_lch())),
        row("oklch", Ok(c.to_css_oklch())),
        row(
            "cmyk",
            Ok(format!(
                "cmyk({}%, {}%, {}%, {}%)",
                comp(rf).round(),
                comp(gf).round(),
                comp(bf).round(),
                (k * 100.0).round()
            )),
        ),
        row(
            "name",
            Ok(match c.name() {
                Some(exact) => exact.to_string(),
                None => {
                    let (name, d) = nearest_css_name([r, g, b]);
                    format!("{name} (nearest, off by {})", fmt_f(d.round()))
                }
            }),
        ),
        row("contrast on black", Ok(contrast_row([r, g, b], [0, 0, 0]))),
        row(
            "contrast on white",
            Ok(contrast_row([r, g, b], [255, 255, 255])),
        ),
    ]
    .concat())
}

pub static TOOL: Tool = Tool {
    id: "color-converter",
    name: "Color converter",
    description: "Convert a color between hex, rgb, hsl, hwb, lch, cmyk and css name, with WCAG contrast.",
    category: Category::Converter,
    keywords: &[
        "color",
        "converter",
        "hex",
        "rgb",
        "hsl",
        "hwb",
        "lch",
        "cmyk",
        "css",
        "contrast",
        "wcag",
        "accessibility",
    ],
    fields: &[Field::text("color", "Color (e.g. #1ea54c)")],
    kind: Kind::Pure,
    run: color_converter,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn color_alpha_stripping_vectors() {
        assert_eq!(remove_alpha_channel_when_opaque("#000000ff"), "#000000");
        assert_eq!(remove_alpha_channel_when_opaque("#ffffffFF"), "#ffffff");
        assert_eq!(remove_alpha_channel_when_opaque("#000000FE"), "#000000FE");
        assert_eq!(remove_alpha_channel_when_opaque("#00000000"), "#00000000");
    }

    #[test]
    fn color_conversions() {
        let out = run("color-converter", &[("color", "#ff0000")]).unwrap();
        assert!(out.contains("hex: #ff0000"), "{out}");
        // csscolorparser emits modern space-separated CSS syntax.
        assert!(out.contains("rgb: rgb(255 0 0)"), "{out}");
        assert!(out.contains("hsl: hsl(0 100% 50%)"), "{out}");
        assert!(out.contains("cmyk: cmyk(0%, 100%, 100%, 0%)"), "{out}");
        assert!(out.contains("name: red"), "{out}");
        assert!(out.contains("hwb: "), "{out}");

        assert!(
            run("color-converter", &[("color", "red")])
                .unwrap()
                .contains("#ff0000")
        );
        assert!(run("color-converter", &[]).is_err());
        assert!(run("color-converter", &[("color", "#zzz")]).is_err());
    }

    /// it-tools PR #1017: the nearest name and the two contrast ratios.
    #[test]
    fn nearest_name_and_contrast() {
        // An off-palette colour names its neighbour instead of giving up.
        let out = run("color-converter", &[("color", "#8fbc8e")]).unwrap();
        assert!(
            out.contains("name: darkseagreen (nearest, off by 1)"),
            "{out}"
        );
        // An exact match is still reported as itself, with no distance.
        assert!(
            run("color-converter", &[("color", "#8fbc8f")])
                .unwrap()
                .contains("name: darkseagreen\n")
        );

        // The two extremes pin the formula: black and white are 21:1 apart.
        assert!((contrast_ratio([0, 0, 0], [255, 255, 255]) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio([1, 2, 3], [1, 2, 3]) - 1.0).abs() < 1e-9);
        // WCAG's own worked example: #777 on white is 4.48:1, just under AA.
        let out = run("color-converter", &[("color", "#777777")]).unwrap();
        assert!(
            out.contains("contrast on white: 4.48:1 (AA for large text only)"),
            "{out}"
        );
        assert!(out.contains("contrast on black: 4.69:1 (AA)"), "{out}");

        let white = run("color-converter", &[("color", "white")]).unwrap();
        assert!(white.contains("contrast on black: 21:1 (AAA)"), "{white}");
        assert!(
            white.contains("contrast on white: 1:1 (fails WCAG)"),
            "{white}"
        );
    }
}
