//! `date-time-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use regex::Regex;

pub(crate) const DATE_FORMATS: &[&str] = &[
    "auto",
    "iso8601",
    "iso9075",
    "rfc3339",
    "rfc7231",
    "unix",
    "timestamp",
    "utc",
    "mongo",
    "excel",
];

pub(crate) fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static pattern")
}

/// ponytail: the it-tools ISO 8601 regex leans on backreferences (`\3`, `\17`)
/// to force consistent separators; the Rust engine has none, so this variant
/// accepts a mixed `2016-0201` too. Upgrade path: hand-rolled scanner.
pub(crate) fn is_iso8601(s: &str) -> bool {
    re(r"^[+-]?\d{4}(-?(0[1-9]|1[0-2])(-?(0[1-9]|[12]\d|3[01]))?)?([T ](([01]\d|2[0-3])(:?[0-5]\d(:?[0-5]\d([.,]\d+)?)?)?)([zZ]|[+-]([01]\d|2[0-3])(:?[0-5]\d)?)?)?$")
        .is_match(s)
}

pub(crate) fn is_iso9075(s: &str) -> bool {
    re(r"^([0-9]{4})-([0-9]{2})-([0-9]{2}) ([0-9]{2}):([0-9]{2}):([0-9]{2})(\.[0-9]{1,6})?(([+-])([0-9]{2}):([0-9]{2})|Z)?$")
        .is_match(s)
}

pub(crate) fn is_rfc3339(s: &str) -> bool {
    re(r"^([0-9]{4})-([0-9]{2})-([0-9]{2})T([0-9]{2}):([0-9]{2}):([0-9]{2})(\.[0-9]{1,9})?(([+-])([0-9]{2}):([0-9]{2})|Z)$")
        .is_match(s)
}

pub(crate) fn is_rfc7231(s: &str) -> bool {
    re(r"^[A-Za-z]{3},\s[0-9]{2}\s[A-Za-z]{3}\s[0-9]{4}\s[0-9]{2}:[0-9]{2}:[0-9]{2}\sGMT$")
        .is_match(s)
}

pub(crate) fn is_unix_timestamp(s: &str) -> bool {
    re(r"^[0-9]{1,10}$").is_match(s)
}

pub(crate) fn is_timestamp(s: &str) -> bool {
    re(r"^[0-9]{1,13}$").is_match(s)
}

pub(crate) fn is_mongo_object_id(s: &str) -> bool {
    re(r"^[0-9a-fA-F]{24}$").is_match(s)
}

pub(crate) fn is_excel_format(s: &str) -> bool {
    re(r"^-?\d+(\.\d+)?$").is_match(s)
}

/// it-tools: `new Date(s).toUTCString() === s`.
pub(crate) fn is_utc_date_string(s: &str) -> bool {
    parse_rfc7231(s)
        .map(|d| format_rfc7231(d) == s)
        .unwrap_or(false)
}

pub(crate) fn date_to_excel_format(ms: i64) -> String {
    format!("{}", ms as f64 / 86_400_000.0 + 25569.0)
}

pub(crate) fn excel_format_to_ms(excel: f64) -> i64 {
    // JS `new Date(x)` truncates toward zero, and so does `as i64`.
    ((excel - 25569.0) * 86400.0 * 1000.0) as i64
}

pub(crate) fn format_rfc7231(dt: DateTime<Utc>) -> String {
    dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

pub(crate) fn parse_rfc7231(s: &str) -> anyhow::Result<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s, "%a, %d %b %Y %H:%M:%S GMT")?;
    Ok(Utc.from_utc_datetime(&naive))
}

pub(crate) fn from_ms(ms: i64) -> anyhow::Result<DateTime<Utc>> {
    DateTime::from_timestamp_millis(ms).ok_or_else(|| anyhow::anyhow!("timestamp out of range"))
}

pub(crate) fn parse_iso_like(s: &str) -> anyhow::Result<DateTime<Utc>> {
    let s = s.trim();
    // Normalise a trailing Z and the ISO 9075 space separator so one code path
    // covers ISO 8601, ISO 9075 and RFC 3339.
    let mut norm = s.replace(' ', "T").replace(',', ".");
    if norm.ends_with('Z') || norm.ends_with('z') {
        norm.truncate(norm.len() - 1);
        norm.push_str("+00:00");
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(&norm) {
        return Ok(dt.with_timezone(&Utc));
    }
    for f in ["%Y%m%dT%H%M%S%z", "%Y%m%dT%H%M%S%.f%z", "%Y-%m-%dT%H:%M%z"] {
        if let Ok(dt) = DateTime::parse_from_str(&norm, f) {
            return Ok(dt.with_timezone(&Utc));
        }
    }
    for f in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y%m%dT%H%M%S",
        "%Y%m%dT%H%M",
    ] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(&norm, f) {
            return Ok(Utc.from_utc_datetime(&dt));
        }
    }
    for f in ["%Y-%m-%d", "%Y%m%d"] {
        if let Ok(d) = NaiveDate::parse_from_str(&norm, f) {
            return Ok(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).expect("midnight")));
        }
    }
    // Year-only (`2016`) and year-month (`2016-02`) forms.
    let ym = match norm.split_once('-') {
        Some((y, m)) => (y.parse::<i32>().ok(), m.parse::<u32>().ok()),
        None => (norm.parse::<i32>().ok(), Some(1)),
    };
    if let (Some(y), Some(m)) = ym
        && let Some(d) = NaiveDate::from_ymd_opt(y, m, 1)
    {
        return Ok(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).expect("midnight")));
    }
    anyhow::bail!("`{s}` is not a recognisable date")
}

pub(crate) type DateMatcher = fn(&str) -> bool;

pub(crate) fn detect_date_format(s: &str) -> Option<&'static str> {
    // Order mirrors the it-tools format list, first match wins.
    let checks: &[(&str, DateMatcher)] = &[
        ("iso8601", is_iso8601),
        ("iso9075", is_iso9075),
        ("rfc3339", is_rfc3339),
        ("rfc7231", is_rfc7231),
        ("unix", is_unix_timestamp),
        ("timestamp", is_timestamp),
        ("utc", is_utc_date_string),
        ("mongo", is_mongo_object_id),
        ("excel", is_excel_format),
    ];
    checks.iter().find(|(_, f)| f(s)).map(|(n, _)| *n)
}

pub(crate) fn parse_date(format: &str, s: &str) -> anyhow::Result<DateTime<Utc>> {
    let s = s.trim();
    match format {
        "unix" => from_ms(s.parse::<i64>()? * 1000),
        "timestamp" => from_ms(s.parse::<i64>()?),
        "mongo" => {
            if !is_mongo_object_id(s) {
                anyhow::bail!("`{s}` is not a 24-character hex ObjectID");
            }
            from_ms(i64::from_str_radix(&s[0..8], 16)? * 1000)
        }
        "excel" => {
            from_ms(excel_format_to_ms(s.parse::<f64>().map_err(|_| {
                anyhow::anyhow!("`{s}` is not an Excel serial date")
            })?))
        }
        "rfc7231" | "utc" => parse_rfc7231(s),
        "iso8601" | "iso9075" | "rfc3339" => parse_iso_like(s),
        "auto" => {
            let detected = detect_date_format(s)
                .ok_or_else(|| anyhow::anyhow!("cannot detect the format of `{s}`"))?;
            parse_date(detected, s)
        }
        other => anyhow::bail!("unknown date format `{other}`"),
    }
}

/// Every format at once, in the order of the `.vue`'s `formats` array and with
/// its labels — the page is a list of readonly rows, not a pick-one converter.
/// Each row carries its own error so an unrepresentable one (a pre-epoch date
/// has no ObjectID) does not blank the other nine.
pub(crate) fn render_all_date_formats(dt: DateTime<Utc>) -> String {
    let ms = dt.timestamp_millis();
    let secs = dt.timestamp();
    [
        row(
            "JS locale date string",
            Ok(dt
                .format("%a %b %d %Y %H:%M:%S GMT+0000 (Coordinated Universal Time)")
                .to_string()),
        ),
        row(
            "ISO 8601",
            Ok(dt.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()),
        ),
        row("ISO 9075", Ok(dt.format("%Y-%m-%d %H:%M:%S").to_string())),
        row(
            "RFC 3339",
            Ok(dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        ),
        row("RFC 7231", Ok(format_rfc7231(dt))),
        row("Unix timestamp", Ok(secs.to_string())),
        row("Timestamp", Ok(ms.to_string())),
        row("UTC format", Ok(format_rfc7231(dt))),
        row(
            "Mongo ObjectID",
            if secs < 0 {
                Err(anyhow::anyhow!("no ObjectID before the unix epoch"))
            } else {
                Ok(format!("{secs:x}0000000000000000"))
            },
        ),
        row("Excel date/time", Ok(date_to_excel_format(ms))),
    ]
    .concat()
}

/// Empty input shows *now* in every format, exactly as the `.vue` does
/// (`if (!inputDate.value) return now.value`) — hence the tool's entry in the
/// property suite's `NONDETERMINISTIC` list.
pub(crate) fn date_time_converter(args: &Args) -> anyhow::Result<String> {
    let id = "date-time-converter";
    let input = a(args, id, "date").trim();
    let dt = if input.is_empty() {
        Utc::now()
    } else {
        parse_date(a(args, id, "format"), input)?
    };
    Ok(render_all_date_formats(dt))
}

pub static TOOL: Tool = Tool {
    id: "date-time-converter",
    name: "Date-time converter",
    description: "Convert date and time between ISO 8601, ISO 9075, RFC 3339, RFC 7231, unix, UTC, Mongo ObjectID and Excel formats.",
    category: Category::Converter,
    keywords: &[
        "date",
        "time",
        "converter",
        "iso",
        "rfc",
        "unix",
        "timestamp",
        "mongo",
        "objectid",
        "excel",
        "utc",
    ],
    fields: &[
        Field::text("date", "Date string"),
        Field::select("format", "Input format", DATE_FORMATS, "auto"),
    ],
    kind: Kind::Pure,
    run: date_time_converter,
};
