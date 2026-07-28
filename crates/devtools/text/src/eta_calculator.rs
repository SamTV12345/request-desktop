//! `eta-calculator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

use chrono::Utc;

pub(crate) const TIME_SPAN_UNITS: &[&str] =
    &["milliseconds", "seconds", "minutes", "hours", "days"];

pub(crate) fn span_unit_ms(unit: &str) -> anyhow::Result<f64> {
    Ok(match unit {
        "milliseconds" => 1.0,
        "seconds" => 1000.0,
        "minutes" => 60_000.0,
        "hours" => 3_600_000.0,
        "days" => 86_400_000.0,
        other => anyhow::bail!("unknown time span unit `{other}`"),
    })
}

/// Port of `formatMsDuration`: zero units are omitted, ms appended when present.
pub(crate) fn format_ms_duration(duration: f64) -> String {
    let total = duration.trunc().max(0.0) as i64;
    let ms = total % 1000;
    let secs = total / 1000 % 60;
    let mins = total / 60_000 % 60;
    let hrs = total / 3_600_000;

    let mut parts = Vec::new();
    if hrs > 0 {
        parts.push(format!("{hrs} hour{}", if hrs == 1 { "" } else { "s" }));
    }
    if mins > 0 {
        parts.push(format!("{mins} minute{}", if mins == 1 { "" } else { "s" }));
    }
    if secs > 0 {
        parts.push(format!("{secs} second{}", if secs == 1 { "" } else { "s" }));
    }
    if ms > 0 {
        parts.push(format!("{ms} ms"));
    }
    if parts.is_empty() {
        "0 seconds".into()
    } else {
        parts.join(" ")
    }
}

pub(crate) fn eta_calculator(args: &Args) -> anyhow::Result<String> {
    let id = "eta-calculator";
    let unit_count = parse_num("amount of elements", a(args, id, "unit_count"))?;
    let per_span = parse_num("units per time span", a(args, id, "units_per_span"))?;
    let span = parse_num("time span", a(args, id, "span"))?;
    let span_ms = span * span_unit_ms(a(args, id, "span_unit"))?;
    if per_span <= 0.0 {
        anyhow::bail!("units per time span must be greater than 0");
    }
    let duration_ms = unit_count / (per_span / span_ms);
    if !duration_ms.is_finite() {
        anyhow::bail!("duration is not a finite number");
    }
    // it-tools' "The consumption started at" date picker, defaulting to now.
    let started_raw = a(args, id, "started_at").trim();
    let started_at = if started_raw.is_empty() {
        Utc::now()
    } else {
        parse_iso_like(started_raw)?
    };
    // `is_finite` is not enough: a large-but-finite duration overflows both
    // `TimeDelta` and the date arithmetic, and chrono panics rather than
    // returning. The UI calls this on every keystroke, so a panic is a crash.
    let end = chrono::Duration::try_milliseconds(duration_ms.trunc() as i64)
        .and_then(|d| started_at.checked_add_signed(d))
        .map(|end| end.to_rfc3339())
        .ok_or_else(|| anyhow::anyhow!("out of representable range"));
    Ok(format!(
        "{}{}",
        row("Total duration", Ok(format_ms_duration(duration_ms))),
        row("It will end", end),
    ))
}

pub static TOOL: Tool = Tool {
    id: "eta-calculator",
    name: "ETA calculator",
    description: "Estimate the end time and duration of a task from its consumption rate.",
    category: Category::Math,
    keywords: &[
        "eta",
        "calculator",
        "estimate",
        "duration",
        "time",
        "arrival",
    ],
    fields: &[
        Field::number("unit_count", "Amount of elements to consume", "186"),
        Field::number("units_per_span", "Units consumed per time span", "3"),
        Field::number("span", "Time span", "5"),
        Field::select("span_unit", "Time span unit", TIME_SPAN_UNITS, "minutes"),
        Field::text("started_at", "The consumption started at (blank = now)"),
    ],
    kind: Kind::Pure,
    run: eta_calculator,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn eta_duration_formatting() {
        assert_eq!(format_ms_duration(0.0), "0 seconds");
        assert_eq!(format_ms_duration(1000.0), "1 second");
        assert_eq!(format_ms_duration(61_000.0), "1 minute 1 second");
        assert_eq!(
            format_ms_duration(3_661_012.0),
            "1 hour 1 minute 1 second 12 ms"
        );
        // 500 plates, 5 per 3 minutes -> 5 hours.
        assert_eq!(
            format_ms_duration(500.0 / (5.0 / (3.0 * 60_000.0))),
            "5 hours"
        );
        assert!(
            run("eta-calculator", &[])
                .unwrap()
                .contains("Total duration")
        );
        assert!(run("eta-calculator", &[("units_per_span", "0")]).is_err());
        assert!(run("eta-calculator", &[("span_unit", "fortnights")]).is_err());
        assert!(run("eta-calculator", &[("unit_count", "abc")]).is_err());
    }

    /// Reproducers found by the property suite: a large-but-finite duration
    /// used to panic inside chrono instead of returning, which crashed the UI
    /// on a keystroke.
    #[test]
    fn eta_calculator_survives_out_of_range_durations() {
        let huge = "9".repeat(38);
        for (key, value) in [
            ("unit_count", huge.as_str()),
            ("span", huge.as_str()),
            ("unit_count", &i64::MIN.to_string()),
            ("unit_count", &u64::MAX.to_string()),
            ("span", &u64::MAX.to_string()),
        ] {
            let out = run("eta-calculator", &[(key, value)]);
            // Either answer is fine; panicking is not.
            if let Ok(text) = out {
                assert!(text.contains("Total duration"), "{key}={value}: {text}");
            }
        }
        // A duration beyond the representable range says so instead of lying.
        let text = run("eta-calculator", &[("unit_count", &huge)]).unwrap();
        assert!(text.contains("out of representable range"), "{text}");
    }
}
