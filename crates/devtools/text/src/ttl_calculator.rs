//! `ttl-calculator` — it-tools PR #1033.
//!
//! A TTL lives as a bare number of seconds in the zone file and as a duration in
//! your head, and the mistake is always the conversion between the two. Both
//! panels are computed at once, so there is nothing to switch: the seconds field
//! answers "how long is that", the duration field answers "what do I paste into
//! `max-age=`".

use crate::*;
use anyhow::{Result, bail};

/// The duration input is a config value, not a document. Anything longer than
/// this is paste noise, and refusing it keeps the regex off megabyte strings.
pub(crate) const MAX_DURATION_INPUT: usize = 200;

/// Seconds per unit suffix. `m` is minutes and `mo` is months, which is the
/// reading every cache header and every `sleep` call already assumes. Months and
/// years are the fixed 30/365 day approximations — a TTL has no calendar.
fn unit_secs(unit: &str) -> Option<i64> {
    Some(match unit {
        "" | "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3_600,
        "d" | "day" | "days" => 86_400,
        "w" | "wk" | "wks" | "week" | "weeks" => 604_800,
        "mo" | "mon" | "month" | "months" => 2_592_000,
        "y" | "yr" | "yrs" | "year" | "years" => 31_536_000,
        _ => return None,
    })
}

/// `1d 2h 30m`, `90 minutes`, `3600` — a run of number/unit pairs, summed.
///
/// Shared with `date-duration-calculator`, which needs exactly the same grammar
/// for its "start date plus duration" panel.
pub(crate) fn parse_duration_secs(text: &str) -> Result<i64> {
    let text = text.trim();
    if text.is_empty() {
        bail!("enter a duration such as `1d 2h 30m`, `90 minutes` or a plain number of seconds");
    }
    if text.len() > MAX_DURATION_INPUT {
        bail!(
            "a duration should be shorter than {MAX_DURATION_INPUT} characters; this one is {}",
            text.len()
        );
    }
    let mut total: i64 = 0;
    let mut seen = false;
    for cap in re(r"(?i)(-?\d+(?:\.\d+)?)\s*([a-z]*)").captures_iter(text) {
        let unit = cap[2].to_lowercase();
        let per = unit_secs(&unit)
            .ok_or_else(|| anyhow::anyhow!("unknown unit `{unit}`; use s, m, h, d, w, mo or y"))?;
        // A 200-character literal still parses to infinity, so check the product
        // rather than trusting the field to hold a sane number.
        let part = cap[1].parse::<f64>().unwrap_or(f64::NAN) * per as f64;
        if !part.is_finite() || part.abs() > i64::MAX as f64 {
            bail!("`{}` is too large to be a duration", &cap[1]);
        }
        total = total.saturating_add(part.trunc() as i64);
        seen = true;
    }
    if !seen {
        bail!("`{text}` contains no number; try `1d 2h 30m` or `90 minutes`");
    }
    Ok(total)
}

/// Days is the coarsest unit on purpose: nobody reads a TTL in months, and
/// `31536000s` should say `365d`, not `1y`.
pub(crate) fn format_duration_secs(total: i64) -> String {
    let mut left = total.unsigned_abs();
    let mut parts = Vec::new();
    for (size, label) in [(86_400u64, "d"), (3_600, "h"), (60, "m"), (1, "s")] {
        let count = left / size;
        if count > 0 {
            parts.push(format!("{count}{label}"));
            left -= count * size;
        }
    }
    if parts.is_empty() {
        return "0s".into();
    }
    format!("{}{}", if total < 0 { "-" } else { "" }, parts.join(" "))
}

fn conversions(secs: i64) -> String {
    let v = secs as f64;
    [
        row("Human readable", Ok(format_duration_secs(secs))),
        row("Seconds", Ok(secs.to_string())),
        row("Minutes", Ok(fmt_f((v / 60.0 * 1e3).round() / 1e3))),
        row("Hours", Ok(fmt_f((v / 3_600.0 * 1e3).round() / 1e3))),
        row("Days", Ok(fmt_f((v / 86_400.0 * 1e3).round() / 1e3))),
        row("Cache-Control", Ok(format!("max-age={secs}"))),
    ]
    .concat()
}

fn ttl(a: &Args) -> Result<String> {
    let secs = n(a, "seconds", 86_400);
    Ok([
        section("From seconds", Ok(conversions(secs))),
        section(
            "From a written duration",
            parse_duration_secs(s(a, "duration", "1d 2h 30m")).map(conversions),
        ),
    ]
    .join("\n"))
}

pub static TOOL: Tool = Tool {
    id: "ttl-calculator",
    name: "TTL calculator",
    description: "Seconds to a readable duration and back, for DNS TTLs, cache headers and max-age.",
    category: Category::Converter,
    keywords: &[
        "ttl", "duration", "seconds", "dns", "cache", "max-age", "expiry",
    ],
    fields: &[
        Field::number("seconds", "Seconds", "86400"),
        Field::text("duration", "Written duration").with_default("1d 2h 30m"),
    ],
    kind: Kind::Pure,
    run: ttl,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn both_directions_at_once() {
        let out = ok("ttl-calculator", &[]);
        assert!(out.contains("Human readable: 1d"), "{out}");
        assert!(out.contains("Minutes: 1440"), "{out}");
        assert!(out.contains("Hours: 24"), "{out}");
        assert!(out.contains("Cache-Control: max-age=86400"), "{out}");
        // 1d 2h 30m = 86400 + 7200 + 1800.
        assert!(out.contains("Seconds: 95400"), "{out}");
    }

    #[test]
    fn duration_grammar() {
        assert_eq!(parse_duration_secs("1d 2h 30m").unwrap(), 95_400);
        assert_eq!(parse_duration_secs("90 minutes").unwrap(), 5_400);
        assert_eq!(parse_duration_secs("3600").unwrap(), 3_600);
        assert_eq!(parse_duration_secs("1d2h").unwrap(), 93_600);
        assert_eq!(parse_duration_secs("2 weeks").unwrap(), 1_209_600);
        assert_eq!(parse_duration_secs("-30m").unwrap(), -1_800);
        assert!(parse_duration_secs("").is_err());
        assert!(parse_duration_secs("soon").is_err());
        assert!(parse_duration_secs("5 fortnights").is_err());
        assert!(parse_duration_secs(&"1d ".repeat(200)).is_err());
        // A literal that overflows f64 says so instead of wrapping.
        assert!(parse_duration_secs(&format!("{}y", "9".repeat(40))).is_err());
    }

    #[test]
    fn formatting_round_trips() {
        assert_eq!(format_duration_secs(0), "0s");
        assert_eq!(format_duration_secs(59), "59s");
        assert_eq!(format_duration_secs(95_400), "1d 2h 30m");
        assert_eq!(format_duration_secs(31_536_000), "365d");
        assert_eq!(format_duration_secs(-3_600), "-1h");
        // i64::MIN has no positive counterpart; unsigned_abs is why this works.
        assert!(format_duration_secs(i64::MIN).starts_with('-'));
    }

    #[test]
    fn adversarial_inputs_are_answered_not_crashed() {
        for (k, v) in [
            ("seconds", i64::MIN.to_string()),
            ("seconds", u64::MAX.to_string()),
            ("duration", "\u{0}\u{7}\u{1b}".to_string()),
            ("duration", "(".repeat(10_000)),
            ("duration", "x".repeat(1_000_000)),
        ] {
            let out = ok("ttl-calculator", &[(k, v.as_str())]);
            assert!(out.contains("From seconds:"), "{k}");
        }
    }
}
