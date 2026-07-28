//! `yaml-viewer`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Result, bail};

/// A plain integer scalar sitting in a value position.
///
/// `\d{19,}` is a pre-filter only — u64 still holds twenty digits, so the real
/// decision is the `parse` below. Both delimiters are *captured* rather than
/// looked ahead at, because the `regex` crate has no lookaround; they are put
/// back verbatim by the replacement. What the leading delimiter buys is the
/// exclusion of `abc1234…` and of `"1234…"`: the character before the digits
/// must be whitespace or YAML punctuation, never a quote or a letter.
fn big_int_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?m)(^|[\s,\[\{])([+-]?[0-9]{19,})($|[\s,\]\}])").expect("static regex")
    })
}

/// Carry integers wider than 64 bits across the round trip as placeholders.
///
/// it-tools PR #1434: a big integer either killed the parse outright ("JSON
/// number out of range", for anything between `u64::MAX` and `u128::MAX`) or,
/// past `u128`, was quietly rounded into an f64 and printed back as
/// `1.2345678901234567e69`. A formatter must not edit the number it is
/// formatting.
///
/// ponytail: the digits ride along as a placeholder scalar instead of through a
/// bignum-aware `Value`, because no `Value` within reach can *emit* an integer
/// wider than 64 bits — `serde_norway`'s serializer has nothing to hand it, so
/// a custom `Value` would only move the problem to the output side. Known
/// ceiling: only plain, unquoted scalars are shielded. A fully quoted numeric
/// string is skipped on purpose, since replacing it would drop the quoting that
/// makes it a string. Upgrade path, if it ever matters: a YAML emitter of our
/// own.
fn shield_big_ints(raw: &str) -> (String, String, Vec<String>) {
    let mut tag = "RPBIGINT".to_string();
    while raw.contains(&tag) {
        tag.push('Q');
    }
    let mut digits: Vec<String> = Vec::new();
    let shielded = big_int_re()
        .replace_all(raw, |c: &regex::Captures| {
            let lit = &c[2];
            if lit.parse::<u64>().is_ok() || lit.parse::<i64>().is_ok() {
                return c[0].to_string();
            }
            digits.push(lit.to_string());
            format!("{}{tag}{}Z{}", &c[1], digits.len() - 1, &c[3])
        })
        .into_owned();
    (shielded, tag, digits)
}

fn restore_big_ints(out: &str, tag: &str, digits: &[String]) -> String {
    if digits.is_empty() {
        return out.to_string();
    }
    // One pass, so a document full of big integers stays linear rather than
    // turning into one `replace` per literal.
    let re = match regex::Regex::new(&format!("{tag}([0-9]+)Z")) {
        Ok(re) => re,
        Err(_) => return out.to_string(),
    };
    re.replace_all(out, |c: &regex::Captures| {
        c[1].parse::<usize>()
            .ok()
            .and_then(|i| digits.get(i))
            .cloned()
            .unwrap_or_else(|| c[0].to_string())
    })
    .into_owned()
}

pub(crate) fn yaml_viewer(args: &Args) -> Result<String> {
    let t = &TOOLS[13];
    let raw = arg(args, t, "input").trim();
    if raw.is_empty() {
        bail!("no YAML input");
    }
    let (shielded, tag, digits) = shield_big_ints(raw);
    let v = yaml_str(&shielded)?;
    let out = to_yaml(&if flag(args, t, "sortKeys") {
        sorted(&v)
    } else {
        v
    })?;
    Ok(restore_big_ints(&out, &tag, &digits))
}

pub static TOOL: Tool = Tool {
    id: "yaml-viewer",
    name: "YAML prettify and validate",
    description: "Format and validate a YAML document.",
    category: Category::Data,
    keywords: &["yaml", "yml", "viewer", "prettify", "validate"],
    fields: &[
        Field::area("input", "YAML"),
        Field::bool("sortKeys", "Sort keys", "false"),
    ],
    kind: Kind::Pure,
    run: yaml_viewer,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;
    #[test]
    fn yaml_viewer_normalises() {
        let out = ok("yaml-viewer", &[("input", "b:   2\na: 1")]);
        assert!(out.starts_with("b: 2"), "{out}");
        let sorted = ok(
            "yaml-viewer",
            &[("input", "b:   2\na: 1"), ("sortKeys", "true")],
        );
        assert!(sorted.starts_with("a: 1"), "{sorted}");
    }

    /// PR #1434: digits in, the same digits out.
    #[test]
    fn yaml_viewer_keeps_big_integers_exact() {
        for n in [
            "18446744073709551615",           // u64::MAX, already fine
            "123456789012345678901234567890", // used to be a hard error
            "1234567890123456789012345678901234567890123456789012345678901234567890", // used to go exponential
            "-123456789012345678901234567890",
        ] {
            let out = ok("yaml-viewer", &[("input", &format!("a: {n}"))]);
            assert_eq!(out.trim(), format!("a: {n}"), "{n}");
        }
        // In a list and in a flow sequence too.
        let big = "123456789012345678901234567890";
        let out = ok(
            "yaml-viewer",
            &[("input", &format!("a:\n  - {big}\nb: [{big}, 1]"))],
        );
        assert!(out.contains(big), "{out}");
        assert!(!out.contains('e'), "went exponential: {out}");
        // A quoted numeric string keeps its quoting: it is a string, not a
        // number, and shielding it would silently change that.
        let out = ok("yaml-viewer", &[("input", &format!("a: \"{big}\""))]);
        assert!(out.contains(big) && out.contains('\''), "{out}");
    }

    /// PR #957: the message must say where.
    #[test]
    fn yaml_viewer_error_names_line_and_column() {
        let e = run("yaml-viewer", &[("input", "a:\n - b\n\tc: 1")])
            .unwrap_err()
            .to_string();
        assert!(e.contains("line") && e.contains("column"), "{e}");
        assert!(run("yaml-viewer", &[("input", "")]).is_err());
    }

    // ── xml ─────────────────────────────────────────────────────────────
}
