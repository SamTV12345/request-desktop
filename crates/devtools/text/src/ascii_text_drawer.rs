//! `ascii-text-drawer`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

/// A 4x5 bitmap font: five rows, `#` on, `.` off. ponytail: one built-in font
/// instead of a vendored FIGlet pack (~4 MB of `.flf`). Upgrade path: parse real
/// `.flf` files from disk and keep this as the offline fallback.
pub(crate) const BLOCK_FONT: &[(char, [&str; 5])] = &[
    ('A', [".##.", "#..#", "####", "#..#", "#..#"]),
    ('B', ["###.", "#..#", "###.", "#..#", "###."]),
    ('C', [".###", "#...", "#...", "#...", ".###"]),
    ('D', ["###.", "#..#", "#..#", "#..#", "###."]),
    ('E', ["####", "#...", "###.", "#...", "####"]),
    ('F', ["####", "#...", "###.", "#...", "#..."]),
    ('G', [".###", "#...", "#.##", "#..#", ".###"]),
    ('H', ["#..#", "#..#", "####", "#..#", "#..#"]),
    ('I', ["####", ".##.", ".##.", ".##.", "####"]),
    ('J', ["####", "...#", "...#", "#..#", ".##."]),
    ('K', ["#..#", "#.#.", "##..", "#.#.", "#..#"]),
    ('L', ["#...", "#...", "#...", "#...", "####"]),
    ('M', ["#..#", "####", "####", "#..#", "#..#"]),
    ('N', ["#..#", "##.#", "####", "#.##", "#..#"]),
    ('O', [".##.", "#..#", "#..#", "#..#", ".##."]),
    ('P', ["###.", "#..#", "###.", "#...", "#..."]),
    ('Q', [".##.", "#..#", "#..#", "#.##", ".###"]),
    ('R', ["###.", "#..#", "###.", "#.#.", "#..#"]),
    ('S', [".###", "#...", ".##.", "...#", "###."]),
    ('T', ["####", ".##.", ".##.", ".##.", ".##."]),
    ('U', ["#..#", "#..#", "#..#", "#..#", ".##."]),
    ('V', ["#..#", "#..#", "#..#", ".##.", ".##."]),
    ('W', ["#..#", "#..#", "####", "####", "#..#"]),
    ('X', ["#..#", "#..#", ".##.", "#..#", "#..#"]),
    ('Y', ["#..#", "#..#", ".##.", ".##.", ".##."]),
    ('Z', ["####", "..#.", ".##.", "#...", "####"]),
    ('0', [".##.", "#.##", "####", "##.#", ".##."]),
    ('1', [".##.", "###.", ".##.", ".##.", "####"]),
    ('2', ["###.", "...#", ".##.", "#...", "####"]),
    ('3', ["###.", "...#", ".##.", "...#", "###."]),
    ('4', ["#..#", "#..#", "####", "...#", "...#"]),
    ('5', ["####", "#...", "###.", "...#", "###."]),
    ('6', [".###", "#...", "###.", "#..#", ".##."]),
    ('7', ["####", "...#", "..#.", ".#..", ".#.."]),
    ('8', [".##.", "#..#", ".##.", "#..#", ".##."]),
    ('9', [".##.", "#..#", ".###", "...#", "###."]),
    (' ', ["....", "....", "....", "....", "...."]),
    ('!', [".#..", ".#..", ".#..", "....", ".#.."]),
    ('?', ["###.", "...#", ".##.", "....", ".#.."]),
    ('.', ["....", "....", "....", "....", ".#.."]),
    (',', ["....", "....", "....", ".#..", "#..."]),
    ('-', ["....", "....", "####", "....", "...."]),
    ('+', ["....", ".#..", "####", ".#..", "...."]),
    ('=', ["....", "####", "....", "####", "...."]),
    (':', ["....", ".#..", "....", ".#..", "...."]),
    ('\'', [".#..", ".#..", "....", "....", "...."]),
    ('/', ["...#", "..#.", ".#..", "#...", "#..."]),
];

pub(crate) const GLYPH_WIDTH: usize = 5; // 4 bitmap columns + 1 space

pub(crate) fn glyph(c: char) -> [&'static str; 5] {
    let upper = c.to_ascii_uppercase();
    BLOCK_FONT
        .iter()
        .find(|(g, _)| *g == upper)
        .map(|(_, bits)| *bits)
        .unwrap_or([".#.#", "..#.", ".#.#", "..#.", ".#.#"]) // unknown char
}

pub(crate) fn render_block_font(text: &str, width: usize) -> String {
    let per_line = (width / GLYPH_WIDTH).max(1);
    let mut out = String::new();
    for chunk in text.chars().collect::<Vec<_>>().chunks(per_line) {
        for row in 0..5 {
            let line: String = chunk
                .iter()
                .map(|c| format!("{} ", glyph(*c)[row].replace('.', " ")))
                .collect::<String>();
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }
    out
}

pub(crate) fn ascii_text_drawer(args: &Args) -> anyhow::Result<String> {
    let id = "ascii-text-drawer";
    let text = nonempty("text", a(args, id, "text"))?;
    let width = uint("width", a(args, id, "width"), 10_000)?;
    if width < GLYPH_WIDTH {
        anyhow::bail!("width must be at least {GLYPH_WIDTH}");
    }
    let font = a(args, id, "font");
    if font != "block" {
        anyhow::bail!("only the built-in `block` font is bundled (got `{font}`)");
    }
    let mut out = String::new();
    for (i, line) in text.lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&render_block_font(line, width));
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "ascii-text-drawer",
    name: "ASCII text drawer",
    description: "Create ASCII art text with the built-in block font.",
    category: Category::Text,
    keywords: &["ascii", "art", "figlet", "text", "drawer", "banner"],
    fields: &[
        Field::area("text", "Your text"),
        Field::select("font", "Font", &["block"], "block"),
        Field::number("width", "Width", "80"),
    ],
    kind: Kind::Pure,
    run: ascii_text_drawer,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    #[test]
    fn ascii_art_renders() {
        let out = run("ascii-text-drawer", &[("text", "HI")]).unwrap();
        assert_eq!(out.lines().count(), 5);
        assert!(out.contains('#'));
        // Wrapping: width 5 fits exactly one glyph per line.
        let out = run("ascii-text-drawer", &[("text", "AB"), ("width", "5")]).unwrap();
        assert_eq!(out.lines().count(), 10);

        assert!(run("ascii-text-drawer", &[]).is_err());
        assert!(run("ascii-text-drawer", &[("text", "hi"), ("width", "1")]).is_err());
        assert!(run("ascii-text-drawer", &[("text", "hi"), ("width", "nope")]).is_err());
        assert!(run("ascii-text-drawer", &[("text", "hi"), ("font", "Doom")]).is_err());
    }
}
