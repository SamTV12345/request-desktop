//! `roman-numeral-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Context, Result, anyhow, bail};

pub(crate) fn arabic_to_roman(input: &str) -> Result<String> {
    let f: f64 = input.parse().context("not a number")?;
    // it-tools range-checks the raw value (3999.1 is rejected) then truncates (1.9 -> I).
    if !(1.0..=3999.0).contains(&f) {
        bail!("We can only convert numbers between 1 and 3,999");
    }
    roman::to(f.floor() as i32)
        .ok_or_else(|| anyhow!("We can only convert numbers between 1 and 3,999"))
}

pub(crate) fn roman_to_arabic(input: &str) -> Result<String> {
    let re =
        regex::Regex::new(r"^M{0,3}(CM|CD|D?C{0,3})(XC|XL|L?X{0,3})(IX|IV|V?I{0,3})$").unwrap();
    let up = input.to_uppercase();
    if up.is_empty() || !re.is_match(&up) {
        bail!("The input you entered is not a valid roman number");
    }
    let n = roman::from(&up)
        .ok_or_else(|| anyhow!("The input you entered is not a valid roman number"))?;
    Ok(n.to_string())
}

/// it-tools stacks "Arabic to roman" and "Roman to arabic", each with its own
/// input box — no mode select.
pub(crate) fn roman_numeral(args: &Args) -> Result<String> {
    let t = &TOOLS[23];
    let arabic = arg(args, t, "arabic").trim();
    let roman_in = arg(args, t, "roman").trim();
    let mut out = Vec::new();
    if !arabic.is_empty() {
        out.push(crate::section("Arabic to roman", arabic_to_roman(arabic)));
    }
    if !roman_in.is_empty() {
        out.push(crate::section("Roman to arabic", roman_to_arabic(roman_in)));
    }
    if out.is_empty() {
        bail!("no input");
    }
    Ok(out.join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "roman-numeral-converter",
    name: "Roman numeral converter",
    description: "Convert between arabic numbers (1-3999) and roman numerals.",
    category: Category::Converter,
    keywords: &["roman", "numeral", "arabic", "convert"],
    fields: &[
        Field {
            key: "arabic",
            label: "Arabic number to convert",
            kind: crate::FieldKind::Text,
            default: "42",
            options: &[],
        },
        Field {
            key: "roman",
            label: "Roman number to convert",
            kind: crate::FieldKind::Text,
            default: "XLII",
            options: &[],
        },
    ],
    kind: Kind::Pure,
    run: roman_numeral,
};
