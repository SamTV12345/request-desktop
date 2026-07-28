//! `week-numbers-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Context, Result};
use chrono::Datelike;

// it-tools PR #1336.

pub(crate) fn week_numbers(a: &Args) -> Result<String> {
    let raw = s(a, "date", "").trim();
    let date = if raw.is_empty() {
        chrono::Utc::now().date_naive()
    } else {
        chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
            .with_context(|| format!("expected a date as YYYY-MM-DD, got {raw:?}"))?
    };
    let iso = date.iso_week();
    let monday = date - chrono::Duration::days(date.weekday().num_days_from_monday() as i64);
    let sunday = monday + chrono::Duration::days(6);
    // 53-week years are the ones whose 28 December falls in week 53.
    let weeks_in_year = chrono::NaiveDate::from_ymd_opt(iso.year(), 12, 28)
        .map(|d| d.iso_week().week())
        .unwrap_or(52);

    Ok(format!(
        "date: {date} ({})\nISO week: {}-W{:02}\nweek starts: {monday}\nweek ends: {sunday}\n\
         day of year: {} of {}\nquarter: Q{}\nweeks in {}: {weeks_in_year}\n",
        date.weekday(),
        iso.year(),
        iso.week(),
        date.ordinal(),
        if date.leap_year() { 366 } else { 365 },
        (date.month() - 1) / 3 + 1,
        iso.year(),
    ))
}

pub static TOOL: Tool = Tool {
    id: "week-numbers-converter",
    name: "Week number converter",
    description: "ISO week number, week bounds, day of year and quarter for a date.",
    category: Category::Measurement,
    keywords: &[
        "week",
        "iso",
        "calendar",
        "date",
        "quarter",
        "kalenderwoche",
    ],
    fields: &[Field::text("date", "Date (YYYY-MM-DD, blank = today)")],
    kind: Kind::Pure,
    run: week_numbers,
};
