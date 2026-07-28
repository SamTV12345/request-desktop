//! `date-duration-calculator` — it-tools PRs #1324 and #984.
//!
//! Two fields, three answers, no direction toggle: the span between two dates,
//! the date you land on when you add a duration to the start, and the calendar
//! facts (weekday, ISO week) you were going to look up anyway. Working days are
//! Monday to Friday with no holiday calendar, which is the only definition that
//! is the same everywhere.
//!
//! A blank start date means today, so this tool is non-deterministic by design —
//! same reason `date-time-converter` is.

use crate::*;
use anyhow::{Result, bail};
use chrono::{DateTime, Datelike, NaiveDate, TimeDelta, Utc};

/// Dates come from a text field, so cap the length before handing a pasted
/// megabyte to the parser.
const MAX_DATE_INPUT: usize = 64;

fn parse_date_field(label: &str, raw: &str) -> Result<DateTime<Utc>> {
    let text = raw.trim();
    if text.len() > MAX_DATE_INPUT {
        bail!(
            "the {label} is {} characters long; a date looks like `2024-01-31` or `2024-01-31 09:00`",
            text.len()
        );
    }
    parse_iso_like(text)
}

fn start_date(raw: &str) -> Result<DateTime<Utc>> {
    if raw.trim().is_empty() {
        return Ok(Utc::now());
    }
    parse_date_field("start date", raw)
}

fn end_date(raw: &str) -> Result<DateTime<Utc>> {
    if raw.trim().is_empty() {
        bail!("enter an end date such as `2024-03-01` to measure the span from the start date");
    }
    parse_date_field("end date", raw)
}

/// Monday-to-Friday days in the half-open range, so it agrees with the plain day
/// count. Whole weeks are arithmetic and only the remainder is walked, which
/// keeps a 10,000-year span as cheap as a one-week one.
pub(crate) fn working_days(from: NaiveDate, to: NaiveDate) -> i64 {
    let (lo, hi, sign) = if to >= from {
        (from, to, 1)
    } else {
        (to, from, -1)
    };
    let days = (hi - lo).num_days();
    let weeks = days / 7;
    let mut count = weeks * 5;
    let mut day = match lo.checked_add_signed(TimeDelta::days(weeks * 7)) {
        Some(d) => d,
        None => return sign * count,
    };
    while day < hi {
        if day.weekday().num_days_from_monday() < 5 {
            count += 1;
        }
        match day.succ_opt() {
            Some(next) => day = next,
            None => break,
        }
    }
    sign * count
}

/// Whole calendar months, the way a human counts them: 31 Jan to 1 Mar is one
/// month and a day, not two months.
pub(crate) fn calendar_months(from: NaiveDate, to: NaiveDate) -> i64 {
    let (lo, hi, sign) = if to >= from {
        (from, to, 1)
    } else {
        (to, from, -1)
    };
    let mut months = (i64::from(hi.year()) - i64::from(lo.year())) * 12
        + (i64::from(hi.month()) - i64::from(lo.month()));
    if hi.day() < lo.day() {
        months -= 1;
    }
    sign * months.max(0)
}

fn span_rows(start: DateTime<Utc>, end: DateTime<Utc>) -> String {
    let delta = end - start;
    let days = delta.num_days();
    let months = calendar_months(start.date_naive(), end.date_naive());
    [
        row("Days", Ok(days.to_string())),
        row("Weeks", Ok(fmt_f((days as f64 / 7.0 * 1e3).round() / 1e3))),
        row("Hours", Ok(delta.num_hours().to_string())),
        row("Minutes", Ok(delta.num_minutes().to_string())),
        row("Calendar months", Ok(months.to_string())),
        row(
            "Calendar years",
            Ok(format!("{} years {} months", months / 12, months % 12)),
        ),
        row(
            "Working days (Mon-Fri)",
            Ok(working_days(start.date_naive(), end.date_naive()).to_string()),
        ),
    ]
    .concat()
}

fn facts(label: &str, dt: DateTime<Utc>) -> String {
    let week = dt.iso_week();
    [
        row(
            &format!("{label} date"),
            Ok(dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()),
        ),
        row(&format!("{label} weekday"), Ok(dt.format("%A").to_string())),
        row(
            &format!("{label} ISO week"),
            Ok(format!("{}-W{:02}", week.year(), week.week())),
        ),
        row(
            &format!("{label} day of year"),
            Ok(dt.ordinal().to_string()),
        ),
    ]
    .concat()
}

fn plus_duration(start: DateTime<Utc>, text: &str) -> Result<String> {
    let secs = parse_duration_secs(text)?;
    // chrono panics rather than wrapping on an out-of-range add, and this runs
    // on every keystroke, so both steps are checked.
    let target = TimeDelta::try_seconds(secs)
        .and_then(|d| start.checked_add_signed(d))
        .ok_or_else(|| anyhow::anyhow!("that duration lands outside the representable range"))?;
    Ok([
        row("Duration", Ok(format_duration_secs(secs))),
        row("Seconds", Ok(secs.to_string())),
        facts("Target", target),
    ]
    .concat())
}

fn date_duration(a: &Args) -> Result<String> {
    let start = start_date(s(a, "start", ""))?;
    let end = || end_date(s(a, "end", ""));
    Ok([
        section("Between the two dates", end().map(|e| span_rows(start, e))),
        section(
            "Start date plus a duration",
            plus_duration(start, s(a, "duration", "3d 4h")),
        ),
        section(
            "Calendar facts",
            Ok(format!(
                "{}{}",
                facts("Start", start),
                match end() {
                    Ok(e) => facts("End", e),
                    Err(e) => row("End date", Err(e)),
                }
            )),
        ),
    ]
    .join("\n"))
}

pub static TOOL: Tool = Tool {
    id: "date-duration-calculator",
    name: "Date duration calculator",
    description: "Span between two dates in days, weeks, months and working days, plus start date + duration.",
    category: Category::Math,
    keywords: &[
        "date", "duration", "days", "between", "workdays", "business", "calendar", "week",
    ],
    fields: &[
        Field::text("start", "Start date, YYYY-MM-DD (blank = today)"),
        Field::text("end", "End date, YYYY-MM-DD (optionally with a time)"),
        Field::text("duration", "Duration to add to the start date").with_default("3d 4h"),
    ],
    kind: Kind::Pure,
    run: date_duration,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).expect("valid date")
    }

    #[test]
    fn two_months_of_2024() {
        let out = ok(
            "date-duration-calculator",
            &[("start", "2024-01-01"), ("end", "2024-03-01")],
        );
        // 2024 is a leap year: 31 + 29 = 60 days.
        assert!(out.contains("Days: 60"), "{out}");
        assert!(out.contains("Hours: 1440"), "{out}");
        assert!(out.contains("Calendar months: 2"), "{out}");
        assert!(out.contains("Calendar years: 0 years 2 months"), "{out}");
        // 60 days from a Monday is 8 whole weeks plus Mon-Thu.
        assert!(out.contains("Working days (Mon-Fri): 44"), "{out}");
        assert!(out.contains("Start weekday: Monday"), "{out}");
        assert!(out.contains("Start ISO week: 2024-W01"), "{out}");
        assert!(out.contains("End weekday: Friday"), "{out}");
        // 2024-01-01 + 3d 4h.
        assert!(
            out.contains("Target date: 2024-01-04 04:00:00 UTC"),
            "{out}"
        );
    }

    #[test]
    fn working_day_arithmetic() {
        // A single Monday-to-Saturday week has five working days.
        assert_eq!(working_days(d(2024, 1, 1), d(2024, 1, 8)), 5);
        assert_eq!(working_days(d(2024, 1, 6), d(2024, 1, 8)), 0); // Sat, Sun
        assert_eq!(working_days(d(2024, 1, 1), d(2024, 1, 1)), 0);
        // Reversed dates are negative, not an error.
        assert_eq!(working_days(d(2024, 1, 8), d(2024, 1, 1)), -5);
        // Ten thousand years: whole weeks are arithmetic, so the answer is
        // 5/7 of the span give or take the four leftover days.
        let (from, to) = (d(1, 1, 1), d(9999, 12, 31));
        let span = (to - from).num_days();
        assert!((working_days(from, to) - span * 5 / 7).abs() <= 5);

        assert_eq!(calendar_months(d(2024, 1, 31), d(2024, 3, 1)), 1);
        assert_eq!(calendar_months(d(2020, 5, 10), d(2024, 7, 10)), 50);
        assert_eq!(calendar_months(d(2024, 3, 1), d(2020, 3, 1)), -48);
    }

    #[test]
    fn each_panel_fails_on_its_own() {
        // No end date: the span panel says what to type, the other two work.
        let out = ok("date-duration-calculator", &[("start", "2024-01-01")]);
        assert!(out.contains("(enter an end date"), "{out}");
        assert!(
            out.contains("Target date: 2024-01-04 04:00:00 UTC"),
            "{out}"
        );
        assert!(out.contains("Start weekday: Monday"), "{out}");
        // A blank start is today, so the tool still answers with no args at all.
        assert!(ok("date-duration-calculator", &[]).contains("Start weekday:"));
        // A bad start date is the one case where nothing can be produced.
        assert!(run("date-duration-calculator", &[("start", "yesterday")]).is_err());
    }

    #[test]
    fn adversarial_inputs_are_answered_not_crashed() {
        for (k, v) in [
            ("start", "9".repeat(1_000_000)),
            ("end", "(".repeat(10_000)),
            ("end", i64::MIN.to_string()),
            ("duration", u64::MAX.to_string()),
            ("duration", format!("{}y", "9".repeat(30))),
            ("duration", "\u{0}\u{1b}".to_string()),
        ] {
            let _ = run("date-duration-calculator", &[(k, v.as_str())]);
        }
        // A duration past the end of time reports that instead of panicking.
        let out = ok(
            "date-duration-calculator",
            &[("start", "2024-01-01"), ("duration", "999999999y")],
        );
        assert!(out.contains("representable range"), "{out}");
    }
}
