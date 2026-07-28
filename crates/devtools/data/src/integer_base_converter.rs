//! `integer-base-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Result, anyhow, bail};
use num_bigint::BigUint;

pub(crate) const BASE_ALPHABET: &[u8] =
    b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ+/";

pub(crate) fn convert_base(value: &str, from: u32, to: u32) -> Result<String> {
    if !(2..=64).contains(&from) || !(2..=64).contains(&to) {
        bail!("bases must be between 2 and 64");
    }
    let from_range = &BASE_ALPHABET[..from as usize];
    let mut acc = BigUint::ZERO;
    let base = BigUint::from(from);
    for ch in value.chars() {
        let b = u8::try_from(ch as u32)
            .ok()
            .filter(|b| from_range.contains(b));
        let digit = b.ok_or_else(|| anyhow!("Invalid digit \"{ch}\" for base {from}."))?;
        let idx = from_range.iter().position(|c| *c == digit).unwrap();
        acc = acc * &base + BigUint::from(idx);
    }
    if acc == BigUint::ZERO {
        return Ok("0".into());
    }
    let to_big = BigUint::from(to);
    let mut digits = Vec::new();
    while acc > BigUint::ZERO {
        let rem: u32 = (&acc % &to_big).try_into().unwrap();
        digits.push(BASE_ALPHABET[rem as usize]);
        acc /= &to_big;
    }
    digits.reverse();
    Ok(String::from_utf8(digits).unwrap())
}

/// The prefix a base is conventionally written with, on both sides.
///
/// Bases 10 and 64 have none, so their rows stay bare digits.
fn prefix_of(base: u32) -> &'static str {
    match base {
        2 => "0b",
        8 => "0o",
        16 => "0x",
        _ => "",
    }
}

/// Split a prefix notation off the input, returning `(digits, base)`.
///
/// it-tools PR #901: `0x1f`, `#1f`, `1fh`, `0b1010` and `0o17` are how people
/// actually paste numbers around. **A recognised notation overrides the "Input
/// base" field**, because typing `0x` is an explicit statement about the value
/// while the base select is often just whatever it was left on. That is also
/// why the trailing-`h` form is only honoured for a declared base of 16 or
/// less: in base 36 or 64 `h` is a perfectly ordinary digit, so silently
/// re-reading `abch` as hex there would corrupt a valid number.
fn detect_base(value: &str, declared: u32) -> (&str, u32) {
    let lower = value.to_ascii_lowercase();
    for (p, base) in [("0x", 16u32), ("0b", 2), ("0o", 8), ("#", 16)] {
        if let Some(rest) = lower.strip_prefix(p)
            && !rest.is_empty()
        {
            return (&value[p.len()..], base);
        }
    }
    if declared <= 16
        && let Some(rest) = lower.strip_suffix('h')
        && !rest.is_empty()
    {
        return (&value[..value.len() - 1], 16);
    }
    (value, declared)
}

/// it-tools shows one input base and *every* interesting output base at once
/// (2, 8, 10, 16, 64 plus one custom), so there is no output-base picker to
/// get wrong — `toBase` is only the custom row.
pub(crate) fn integer_base_converter(args: &Args) -> Result<String> {
    let t = &TOOLS[22];
    let raw = arg(args, t, "input").trim();
    if raw.is_empty() {
        bail!("no input");
    }
    let declared = u32::try_from(num(args, t, "fromBase")?).unwrap_or(0);
    let (digits, from) = detect_base(raw, declared);
    // Hex is written in either case, and for any base up to 16 the alphabet has
    // no uppercase digits at all, so lowering is purely permissive there. From
    // base 17 up, `A` and `a` are two different digits and must stay apart.
    let owned;
    let value: &str = if from <= 16 {
        owned = digits.to_ascii_lowercase();
        &owned
    } else {
        digits
    };
    let custom = u32::try_from(num(args, t, "toBase")?).unwrap_or(0);
    // A bad input base or a stray digit fails every row alike; it-tools puts that
    // in an error alert above the (blank) rows, we make it the whole output.
    convert_base(value, from, 10)?;
    let rows = [
        ("Binary (2)".to_string(), 2u32),
        ("Octal (8)".to_string(), 8),
        ("Decimal (10)".to_string(), 10),
        ("Hexadecimal (16)".to_string(), 16),
        ("Base64 (64)".to_string(), 64),
        (format!("Base {custom}"), custom),
    ];
    Ok(rows
        .iter()
        .map(|(label, to)| match convert_base(value, from, *to) {
            Ok(s) => format!("{label}: {}{s}", prefix_of(*to)),
            Err(e) => format!("{label}: ({e})"),
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

pub static TOOL: Tool = Tool {
    id: "integer-base-converter",
    name: "Integer base converter",
    description: "Show an arbitrarily large integer in base 2, 8, 10, 16, 64 and one of your choice.",
    category: Category::Converter,
    keywords: &[
        "integer", "base", "binary", "hex", "radix", "convert", "prefix",
    ],
    fields: &[
        Field {
            key: "input",
            label: "Input number (0x, 0b, 0o, # or trailing h win over the base below)",
            kind: crate::FieldKind::Text,
            default: "42",
            options: &[],
        },
        Field::number("fromBase", "Input base", "10"),
        Field::number("toBase", "Custom output base", "42"),
    ],
    kind: Kind::Pure,
    run: integer_base_converter,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;
    #[test]
    fn integer_base_converter_vectors() {
        let cases = [
            ("0", 2, 11, "0"),
            ("0", 5, 2, "0"),
            ("0", 10, 16, "0"),
            ("10100101", 2, 16, "a5"),
            ("192654", 10, 8, "570216"),
            ("zz", 64, 10, "2275"),
            (
                "42540766411283223938465490632011909384",
                10,
                10,
                "42540766411283223938465490632011909384",
            ),
            (
                "42540766411283223938465490632011909384",
                10,
                16,
                "20010db8000085a300000000ac1f8908",
            ),
            (
                "20010db8000085a300000000ac1f8908",
                16,
                10,
                "42540766411283223938465490632011909384",
            ),
        ];
        // The output-base picker is gone: it-tools lists 2/8/10/16/64 plus the
        // custom base at once, so `toBase` is only the custom row. Since PR #901
        // every row carries the conventional prefix for its base, custom row
        // included — the prefix is a property of the base, not of the row.
        for (value, from, to, want) in cases {
            let out = ok(
                "integer-base-converter",
                &[
                    ("input", value),
                    ("fromBase", &from.to_string()),
                    ("toBase", &to.to_string()),
                ],
            );
            let want = format!("{}{want}", super::prefix_of(to));
            assert!(
                out.lines().any(|l| l == format!("Base {to}: {want}")),
                "{value} base {from} -> {to}:\n{out}"
            );
        }
        // all six rows, always
        let out = ok("integer-base-converter", &[("input", "255")]);
        assert!(out.contains("Binary (2): 0b11111111"), "{out}");
        assert!(out.contains("Octal (8): 0o377"), "{out}");
        assert!(out.contains("Decimal (10): 255"), "{out}");
        assert!(out.contains("Hexadecimal (16): 0xff"), "{out}");
        assert!(out.contains("Base64 (64): 3/"), "{out}");
        assert_eq!(out.lines().count(), 6, "{out}");
    }

    /// PR #901: the notations people actually paste, and the rule that a
    /// recognised one beats the "Input base" field.
    #[test]
    fn integer_base_converter_reads_prefixes() {
        let dec = |input: &str, base: &str| {
            ok(
                "integer-base-converter",
                &[("input", input), ("fromBase", base)],
            )
            .lines()
            .find_map(|l| l.strip_prefix("Decimal (10): ").map(str::to_string))
            .expect("decimal row")
        };
        // The base field still says 10 in every one of these.
        assert_eq!(dec("0xff", "10"), "255");
        assert_eq!(dec("0XFF", "10"), "255");
        assert_eq!(dec("#ff", "10"), "255");
        assert_eq!(dec("ffh", "10"), "255");
        assert_eq!(dec("FFH", "10"), "255");
        assert_eq!(dec("0b1010", "10"), "10");
        assert_eq!(dec("0o17", "10"), "15");
        // Hex is case-insensitive even without a prefix.
        assert_eq!(dec("DEADBEEF", "16"), "3735928559");
        // Bare digits still obey the field.
        assert_eq!(dec("1010", "2"), "10");
        // From base 17 up, case is meaningful, so `h` stays a digit and `A`
        // stays distinct from `a`.
        assert_ne!(dec("Az", "62"), dec("az", "62"));
        assert_eq!(dec("h", "36"), "17");
        // A lone marker is not a number in prefix notation — it is the digit.
        assert_eq!(dec("0", "10"), "0");
    }
    #[test]
    fn integer_base_converter_rejects_bad_input() {
        // A stray digit or a bad *input* base kills every row, so it errors.
        assert!(
            run(
                "integer-base-converter",
                &[("input", "12"), ("fromBase", "2")]
            )
            .is_err()
        );
        assert!(
            run(
                "integer-base-converter",
                &[("input", "1"), ("fromBase", "1")]
            )
            .is_err()
        );
        assert!(
            run(
                "integer-base-converter",
                &[("input", "1"), ("fromBase", "abc")]
            )
            .is_err()
        );
        // An out-of-range custom base only spoils its own row.
        let out = ok(
            "integer-base-converter",
            &[("input", "1"), ("toBase", "65")],
        );
        assert!(out.contains("Decimal (10): 1"), "{out}");
        assert!(
            out.contains("Base 65: (bases must be between 2 and 64)"),
            "{out}"
        );
        assert!(run("integer-base-converter", &[("input", "")]).is_err());
    }

    // ── roman-numeral-converter (vectors from the service test) ─────────
}
