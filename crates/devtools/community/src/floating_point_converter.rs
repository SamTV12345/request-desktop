//! `floating-point-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, section};
use anyhow::{Context, Result, bail};

// it-tools PR #1348.

pub(crate) fn f64_panel(v: f64) -> String {
    let bits = v.to_bits();
    format!(
        "value: {v}\nhex: 0x{bits:016x}\nsign: {} ({})\nexponent: {} (stored {}, bias 1023)\n\
         mantissa: 0x{:013x}\nbits: {} {:011b} {:052b}\n",
        bits >> 63,
        if bits >> 63 == 0 { "+" } else { "-" },
        ((bits >> 52) & 0x7FF) as i64 - 1023,
        (bits >> 52) & 0x7FF,
        bits & 0x000F_FFFF_FFFF_FFFF,
        bits >> 63,
        (bits >> 52) & 0x7FF,
        bits & 0x000F_FFFF_FFFF_FFFF,
    )
}

pub(crate) fn f32_panel(v: f64) -> String {
    let single = v as f32;
    let bits = single.to_bits();
    format!(
        "value: {single}\nhex: 0x{bits:08x}\nsign: {} ({})\nexponent: {} (stored {}, bias 127)\n\
         mantissa: 0x{:06x}\nbits: {} {:08b} {:023b}\nrounding error vs the double: {}\n",
        bits >> 31,
        if bits >> 31 == 0 { "+" } else { "-" },
        ((bits >> 23) & 0xFF) as i64 - 127,
        (bits >> 23) & 0xFF,
        bits & 0x007F_FFFF,
        bits >> 31,
        (bits >> 23) & 0xFF,
        bits & 0x007F_FFFF,
        single as f64 - v,
    )
}

pub(crate) fn floating_point(a: &Args) -> Result<String> {
    let raw = s(a, "value", "3.14159").trim();
    if raw.is_empty() {
        bail!("enter a number, or the hexadecimal bits of one (0x…)");
    }
    // Hex in, float out — the other half of the tool, and the reason there is
    // no encode/decode toggle.
    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        let bits = u64::from_str_radix(hex, 16).context("not a hexadecimal bit pattern")?;
        return Ok(match hex.len() {
            1..=8 => [
                section(
                    "From 32 bits",
                    Ok(f64::from(f32::from_bits(bits as u32)).to_string()),
                ),
                section(
                    "Single (32-bit)",
                    Ok(f32_panel(f64::from(f32::from_bits(bits as u32)))),
                ),
            ]
            .join("\n\n"),
            9..=16 => [
                section("From 64 bits", Ok(f64::from_bits(bits).to_string())),
                section("Double (64-bit)", Ok(f64_panel(f64::from_bits(bits)))),
            ]
            .join("\n\n"),
            _ => bail!("a float is 8 (single) or 16 (double) hexadecimal digits"),
        });
    }
    let v: f64 = raw
        .parse()
        .with_context(|| format!("not a number: {raw:?}"))?;
    Ok([
        section("IEEE 754 double (64-bit)", Ok(f64_panel(v))),
        section("IEEE 754 single (32-bit)", Ok(f32_panel(v))),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "floating-point-converter",
    name: "Floating point converter",
    description: "A number's IEEE 754 bits, both ways — sign, exponent, mantissa, rounding error.",
    category: Category::Development,
    keywords: &[
        "float", "ieee", "754", "double", "binary", "mantissa", "exponent",
    ],
    fields: &[Field::text("value", "Number, or 0x… bits").with_default("3.14159")],
    kind: Kind::Pure,
    run: floating_point,
};
