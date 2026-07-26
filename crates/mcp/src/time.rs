//! Turning whatever an agent passes into Unix seconds, or refusing it.
//!
//! Booking the wrong day silently is worse than refusing, so everything here
//! validates first and returns a message that says which formats are accepted.
//!
//! Date arithmetic goes through SQLite rather than a date crate. Not to save a
//! dependency (`rusqlite` is already in the tree) but because `rp-store` groups
//! entries into local days with SQLite's `'localtime'` modifier. Resolving
//! `"today"` with the same implementation means the two cannot disagree about
//! where a day boundary sits.
use anyhow::{Context, Result, bail};
use rusqlite::Connection;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// 2000-01-01Z. Below this is a unit mistake, not a timestamp.
const MIN_TS: i64 = 946_684_800;
/// 2100-01-01Z.
const MAX_TS: i64 = 4_102_444_800;

/// Wall clock, in Unix seconds UTC.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        // A clock before 1970 is not worth a panic; the store just sees 0.
        .map_or(0, |d| d.as_secs() as i64)
}

/// `3735` -> `"1h 02m"`. Never seconds: nobody reads a timesheet in seconds.
pub fn fmt_hm(seconds: i64) -> String {
    let s = seconds.max(0);
    format!("{}h {:02}m", s / 3600, (s % 3600) / 60)
}

/// A scratch SQLite connection used purely as a date library.
pub struct Clock {
    conn: Connection,
}

impl Clock {
    pub fn new() -> Result<Self> {
        Ok(Self {
            conn: Connection::open_in_memory().context("cannot open a scratch connection")?,
        })
    }

    fn text(&self, sql: &str, args: impl rusqlite::Params) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(sql, args, |r| r.get::<_, Option<String>>(0))?)
    }

    /// Local `YYYY-MM-DD` for today.
    pub fn today(&self) -> Result<String> {
        self.text("SELECT date('now', 'localtime')", [])?
            .context("SQLite lost track of today")
    }

    /// `today` / `yesterday` / `tomorrow` / `YYYY-MM-DD`, or an error naming the
    /// accepted forms. An absent or empty value means today.
    pub fn resolve_date(&self, raw: Option<&str>) -> Result<String> {
        let raw = raw.unwrap_or("").trim();
        let shift = match raw.to_ascii_lowercase().as_str() {
            "" | "today" | "now" => Some("0 days"),
            "yesterday" => Some("-1 day"),
            "tomorrow" => Some("+1 day"),
            _ => None,
        };
        if let Some(shift) = shift {
            return self
                .text("SELECT date('now', ?1, 'localtime')", [shift])?
                .context("SQLite could not resolve that day");
        }
        // `date()` normalises, so comparing against the input is what rejects
        // "2026-02-30" (which SQLite would happily roll into March) as well as
        // "26/07/2026" and free text.
        let ok = self
            .text("SELECT date(?1)", [raw])?
            .is_some_and(|d| d == raw);
        if !ok {
            bail!(
                "{raw:?} is not a date I can use. Pass \"today\", \"yesterday\", \
                 or a real calendar date as YYYY-MM-DD (e.g. \"2026-07-26\")."
            );
        }
        Ok(raw.to_string())
    }

    /// `this month` / `last month` / `YYYY-MM`. Absent means this month.
    pub fn resolve_month(&self, raw: Option<&str>) -> Result<String> {
        let raw = raw.unwrap_or("").trim();
        let shift = match raw.to_ascii_lowercase().as_str() {
            "" | "this month" | "current month" => Some("0 days"),
            "last month" | "previous month" => Some("-1 month"),
            _ => None,
        };
        if let Some(shift) = shift {
            return self
                // 'localtime' comes *before* 'start of month': the other order
                // would take the month boundary in UTC and then shift it.
                .text(
                    "SELECT strftime('%Y-%m', 'now', 'localtime', 'start of month', ?1)",
                    [shift],
                )?
                .context("SQLite could not resolve that month");
        }
        // A month is only a date once it has a day, so validate YYYY-MM-01.
        let first = format!("{raw}-01");
        let ok = self
            .text("SELECT date(?1)", [&first])?
            .is_some_and(|d| d == first);
        if !ok {
            bail!(
                "{raw:?} is not a month I can use. Pass \"this month\", \"last month\", \
                 or YYYY-MM (e.g. \"2026-07\")."
            );
        }
        Ok(raw.to_string())
    }

    /// An ISO 8601 timestamp or Unix seconds, from a JSON string or number.
    ///
    /// An ISO timestamp with no timezone is read as *local* time, because that is
    /// what a human or an agent quoting a human means by "09:00". Add `Z` or an
    /// offset to be explicit.
    pub fn parse_ts(&self, field: &str, value: Option<&Value>) -> Result<i64> {
        let value = value.with_context(|| format!("{field} is required"))?;
        match value {
            Value::Number(n) => {
                let secs = n
                    .as_i64()
                    .or_else(|| n.as_f64().map(|f| f as i64))
                    .with_context(|| format!("{field}: {n} is not a whole number of seconds"))?;
                self.check_range(field, secs)
            }
            Value::String(s) => self.parse_ts_str(field, s.trim()),
            other => bail!(
                "{field} must be an ISO 8601 timestamp or Unix seconds, not {}",
                kind_of(other)
            ),
        }
    }

    fn parse_ts_str(&self, field: &str, s: &str) -> Result<i64> {
        if s.is_empty() {
            bail!("{field} is empty; pass an ISO 8601 timestamp or Unix seconds");
        }
        // Unix seconds may arrive as a string; agents are not fussy about types.
        if s.strip_prefix('-')
            .unwrap_or(s)
            .bytes()
            .all(|b| b.is_ascii_digit())
        {
            let secs: i64 = s
                .parse()
                .with_context(|| format!("{field}: {s:?} is not a number"))?;
            return self.check_range(field, secs);
        }
        // Refuse before asking SQLite, because SQLite is *too* generous: it reads
        // a bare "14:30" as 2000-01-01 and "now" as the current instant, either of
        // which would book time on a day nobody meant.
        if !looks_like_iso(s) {
            bail!(
                "{field}: {s:?} is not a timestamp I can use. Pass ISO 8601 \
                 (\"2026-07-26T09:00:00\", \"2026-07-26T09:00:00+02:00\", \
                 \"2026-07-26T07:00:00Z\") or Unix seconds. Without a timezone it is \
                 read as local time."
            );
        }
        // 'utc' reinterprets the literal as local time and converts. With an
        // explicit offset or Z the string already carries its own zone.
        let sql = if has_zone(s) {
            "SELECT strftime('%s', ?1)"
        } else {
            "SELECT strftime('%s', ?1, 'utc')"
        };
        let secs = self
            .text(sql, [s])?
            .and_then(|t| t.parse::<i64>().ok())
            .with_context(|| format!("{field}: {s:?} is not a valid date and time"))?;
        self.check_range(field, secs)
    }

    fn check_range(&self, field: &str, secs: i64) -> Result<i64> {
        if (MIN_TS..MAX_TS).contains(&secs) {
            return Ok(secs);
        }
        // Milliseconds is the mistake that actually happens, so name it.
        if secs > MAX_TS * 100 {
            bail!(
                "{field}: {secs} is far in the future -- that looks like milliseconds. \
                 This tool wants Unix *seconds* ({}).",
                secs / 1000
            );
        }
        bail!(
            "{field}: {secs} is not a plausible Unix-seconds timestamp (expected year 2000-2099)"
        );
    }

    /// Format a Unix timestamp with an SQLite `strftime` pattern, in local time.
    pub fn fmt_local(&self, ts: i64, fmt: &str) -> String {
        self.text(
            "SELECT strftime(?1, ?2, 'unixepoch', 'localtime')",
            rusqlite::params![fmt, ts],
        )
        .ok()
        .flatten()
        // Formatting is cosmetic; a failure must not fail the tool call.
        .unwrap_or_else(|| ts.to_string())
    }

    /// Local `HH:MM`.
    pub fn hm(&self, ts: i64) -> String {
        self.fmt_local(ts, "%H:%M")
    }
}

/// `YYYY-MM-DD` optionally followed by `T`/space and a time.
fn looks_like_iso(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() < 10 {
        return false;
    }
    let digits = [0, 1, 2, 3, 5, 6, 8, 9]
        .iter()
        .all(|&i| b[i].is_ascii_digit());
    let dashes = b[4] == b'-' && b[7] == b'-';
    let tail = b.len() == 10 || matches!(b[10], b'T' | b't' | b' ');
    digits && dashes && tail
}

/// Does the *time* part carry a `Z` or a `+HH:MM` / `-HH:MM` offset?
fn has_zone(s: &str) -> bool {
    let time = &s[10.min(s.len())..];
    time.ends_with('Z') || time.ends_with('z') || time.contains('+') || time.contains('-')
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn clock() -> Clock {
        Clock::new().unwrap()
    }

    fn ts(input: Value) -> Result<i64> {
        clock().parse_ts("started_at", Some(&input))
    }

    #[test]
    fn durations_render_as_hours_and_padded_minutes() {
        assert_eq!(fmt_hm(0), "0h 00m");
        assert_eq!(fmt_hm(59), "0h 00m");
        assert_eq!(fmt_hm(60), "0h 01m");
        assert_eq!(fmt_hm(3600), "1h 00m");
        assert_eq!(fmt_hm(3735), "1h 02m");
        assert_eq!(fmt_hm(86_400 + 1800), "24h 30m");
        assert_eq!(
            fmt_hm(-5),
            "0h 00m",
            "a negative span must not print garbage"
        );
    }

    #[test]
    fn an_iso_timestamp_with_z_is_utc() {
        assert_eq!(ts(json!("2026-07-26T07:00:00Z")).unwrap(), 1_785_049_200);
    }

    #[test]
    fn an_iso_timestamp_with_an_offset_honours_it() {
        // 09:00+02:00 is the same instant as 07:00Z.
        assert_eq!(
            ts(json!("2026-07-26T09:00:00+02:00")).unwrap(),
            1_785_049_200
        );
        assert_eq!(
            ts(json!("2026-07-26T05:00:00-02:00")).unwrap(),
            1_785_049_200
        );
    }

    #[test]
    fn an_iso_timestamp_without_a_zone_is_local_time() {
        let c = clock();
        let parsed = c
            .parse_ts("started_at", Some(&json!("2026-07-26T09:00:00")))
            .unwrap();
        // Whatever this machine's zone is, it must round-trip to the same wall
        // clock -- which is the whole promise of "read as local time".
        assert_eq!(
            c.fmt_local(parsed, "%Y-%m-%dT%H:%M:%S"),
            "2026-07-26T09:00:00"
        );
    }

    #[test]
    fn a_space_separated_timestamp_and_a_bare_date_are_accepted() {
        let c = clock();
        let spaced = c
            .parse_ts("started_at", Some(&json!("2026-07-26 09:00:00")))
            .unwrap();
        assert_eq!(c.fmt_local(spaced, "%H:%M"), "09:00");
        let midnight = c
            .parse_ts("started_at", Some(&json!("2026-07-26")))
            .unwrap();
        assert_eq!(c.fmt_local(midnight, "%Y-%m-%d %H:%M"), "2026-07-26 00:00");
    }

    #[test]
    fn unix_seconds_are_taken_as_a_number_or_a_string() {
        assert_eq!(ts(json!(1_785_049_200i64)).unwrap(), 1_785_049_200);
        assert_eq!(ts(json!("1785049200")).unwrap(), 1_785_049_200);
    }

    #[test]
    fn milliseconds_are_diagnosed_rather_than_booked() {
        let err = ts(json!(1_785_049_200_000i64)).unwrap_err().to_string();
        assert!(err.contains("milliseconds"), "{err}");
        assert!(
            err.contains("1785049200"),
            "the message should show the fix: {err}"
        );
    }

    #[test]
    fn garbage_is_refused_with_the_accepted_formats() {
        for bad in [
            json!("yesterday afternoon"),
            json!("26/07/2026"),
            json!("14:30"),
            json!("now"),
            json!(""),
            json!("2026-13-01T00:00:00"),
            json!("2026-07-26T99:99:99"),
            json!(true),
            json!({"when": "later"}),
            json!(0),
        ] {
            let err = ts(bad.clone()).unwrap_err().to_string();
            assert!(err.contains("started_at"), "{bad} -> {err}");
        }
    }

    #[test]
    fn a_missing_timestamp_says_which_field() {
        let err = clock()
            .parse_ts("stopped_at", None)
            .unwrap_err()
            .to_string();
        assert_eq!(err, "stopped_at is required");
    }

    #[test]
    fn relative_day_words_resolve_and_are_case_insensitive() {
        let c = clock();
        let today = c.today().unwrap();
        assert_eq!(c.resolve_date(None).unwrap(), today);
        assert_eq!(c.resolve_date(Some("")).unwrap(), today);
        assert_eq!(c.resolve_date(Some("Today")).unwrap(), today);
        let y = c.resolve_date(Some("YESTERDAY")).unwrap();
        assert_ne!(y, today);
        assert!(
            y < today,
            "yesterday must sort before today: {y} vs {today}"
        );
        assert!(c.resolve_date(Some("tomorrow")).unwrap() > today);
    }

    #[test]
    fn an_explicit_day_passes_through_and_a_fake_one_does_not() {
        let c = clock();
        assert_eq!(c.resolve_date(Some(" 2026-07-26 ")).unwrap(), "2026-07-26");
        for bad in [
            "2026-02-30",
            "2026-7-26",
            "26-07-2026",
            "last tuesday",
            "2026-07",
        ] {
            let err = c.resolve_date(Some(bad)).unwrap_err().to_string();
            assert!(err.contains("YYYY-MM-DD"), "{bad} -> {err}");
        }
    }

    #[test]
    fn months_resolve_relatively_and_explicitly() {
        let c = clock();
        let this = c.resolve_month(None).unwrap();
        assert_eq!(this, c.today().unwrap()[..7]);
        assert!(c.resolve_month(Some("last month")).unwrap() < this);
        assert_eq!(c.resolve_month(Some("2026-07")).unwrap(), "2026-07");
        for bad in ["2026-13", "July", "2026-07-26"] {
            let err = c.resolve_month(Some(bad)).unwrap_err().to_string();
            assert!(err.contains("YYYY-MM"), "{bad} -> {err}");
        }
    }

    #[test]
    fn zone_detection_looks_only_at_the_time_part() {
        // The dashes in the date must not read as a negative offset.
        assert!(!has_zone("2026-07-26T09:00:00"));
        assert!(has_zone("2026-07-26T09:00:00Z"));
        assert!(has_zone("2026-07-26T09:00:00+02:00"));
        assert!(has_zone("2026-07-26T09:00:00-05:00"));
    }
}
