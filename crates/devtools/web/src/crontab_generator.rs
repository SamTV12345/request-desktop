//! `crontab-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Result, bail};
use std::fmt::Write as _;

/// The field diagram and the symbol table it-tools shows under the expression.
pub(crate) const CRON_HELP: &str = "\
Field order:
  ┌──────────── [optional] seconds (0 - 59)
  | ┌────────── minute (0 - 59)
  | | ┌──────── hour (0 - 23)
  | | | ┌────── day of month (1 - 31)
  | | | | ┌──── month (1 - 12) OR jan,feb,mar,apr ...
  | | | | | ┌── day of week (0 - 6, sunday=0) OR sun,mon ...
  | | | | | |
  * * * * * * command

Symbols:
  *          Any value                                          * * * *      Every minute
  -          Range of values                                    1-10 * * *   Minutes 1 through 10
  ,          List of values                                     1,10 * * *   At minutes 1 and 10
  /          Step values                                        */10 * * *   Every 10 minutes
  @yearly    Once every year at midnight of 1 January            @yearly      0 0 1 1 *
  @annually  Same as @yearly                                    @annually    0 0 1 1 *
  @monthly   Once a month at midnight on the first day          @monthly     0 0 1 * *
  @weekly    Once a week at midnight on Sunday morning          @weekly      0 0 * * 0
  @daily     Once a day at midnight                             @daily       0 0 * * *
  @midnight  Same as @daily                                     @midnight    0 0 * * *
  @hourly    Once an hour at the beginning of the hour          @hourly      0 * * * *
  @reboot    Run at startup

AWS EventBridge (six fields, recognised by the `?`):
  ┌──────────── minute (0 - 59)
  | ┌────────── hour (0 - 23)
  | | ┌──────── day of month (1 - 31) OR ?
  | | | ┌────── month (1 - 12) OR jan,feb ...
  | | | | ┌──── day of week (1 - 7, sunday=1) OR sun,mon ... OR ?
  | | | | | ┌── year (1970 - 2199) OR *
  | | | | | |
  0 12 * * ? *   Every day at 12:00
  ?          No value: exactly one of the two day fields must be `?`
";

/// The AWS EventBridge dialect, if this is one (it-tools PR #1026).
///
/// EventBridge writes six fields — `minute hour day-of-month month day-of-week
/// year` — where a plain six-field cron writes `second minute hour dom month
/// dow`. The two are told apart by the `?` that EventBridge *requires* in
/// exactly one of the two day fields, plus, as a courtesy, a literal year in the
/// last field.
///
/// ponytail: no heuristic beyond those two. `0 12 * * MON-FRI *` is a valid
/// EventBridge rule with neither marker and is read as a seconds-cron here;
/// AWS rejects it anyway, since it demands the `?`.
fn is_event_bridge(fields: &[&str]) -> bool {
    fields.len() == 6
        && (fields[2].contains('?')
            || fields[4].contains('?')
            || fields[5]
                .split(['-', ',', '/'])
                .any(|p| p.len() == 4 && p.chars().all(|c| c.is_ascii_digit())))
}

/// Parse `UTC`, `Z` or a fixed offset like `+02:00`.
///
/// ponytail: named zones (`Europe/Berlin`) would need `chrono-tz`, which is not
/// in the dependency list, so a name that is not UTC is refused rather than
/// silently treated as UTC. Upgrade path: `chrono-tz`, when someone needs DST.
fn parse_offset(raw: &str) -> Result<chrono::FixedOffset> {
    let s = raw.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("utc") || s.eq_ignore_ascii_case("z") {
        return Ok(chrono::FixedOffset::east_opt(0).expect("0 is a valid offset"));
    }
    s.parse::<chrono::FixedOffset>().map_err(|_| {
        anyhow::anyhow!(
            "unknown time zone {s:?}: use UTC or a fixed offset like +02:00 \
             (named zones need a time zone database this build does not carry)"
        )
    })
}

pub(crate) fn crontab_generator(args: &Args) -> Result<String> {
    use std::str::FromStr as _;
    let t = tool("crontab-generator");
    let expr = arg(args, t, "expression").trim();
    if expr.is_empty() {
        bail!("no cron expression provided");
    }
    let offset = parse_offset(arg(args, t, "timezone"))?;
    let fields: Vec<&str> = expr.split_whitespace().collect();
    let event_bridge = is_event_bridge(&fields);
    let cron = if event_bridge {
        // croner reads seconds first and years last, so EventBridge's six fields
        // become seven by prepending the second. It already maps `?` to `*` in
        // both day fields, and Quartz weekday numbering (1 = Sunday) is what
        // EventBridge uses.
        croner::parser::CronParser::builder()
            .seconds(croner::parser::Seconds::Required)
            .year(croner::parser::Year::Required)
            .alternative_weekdays(true)
            .build()
            .parse(&format!("0 {expr}"))
    } else {
        croner::Cron::from_str(expr)
    }
    .map_err(|e| anyhow::anyhow!("invalid cron: {e}"))?;
    let mut out = String::new();
    let _ = writeln!(out, "Expression: {expr}");
    let _ = writeln!(
        out,
        "Dialect: {}",
        if event_bridge {
            "AWS EventBridge (minute hour day-of-month month day-of-week year, \
             `?` for \"no value\", weekdays 1-7 with Sunday = 1)"
        } else {
            "Unix cron (5 fields, or 6 with leading seconds)"
        }
    );
    let _ = writeln!(out, "Meaning: {}", cron.describe());
    // ponytail: next occurrences are ours, not it-tools'; cheap and the obvious
    // sanity check that the expression means what the description says.
    let _ = writeln!(out, "\nNext occurrences ({}):", offset_label(offset));
    let mut count = 0;
    for t in cron
        .iter_from(
            chrono::Utc::now().with_timezone(&offset),
            croner::Direction::Forward,
        )
        .take(5)
    {
        count += 1;
        let _ = writeln!(out, "  {}", t.format("%Y-%m-%d %H:%M:%S"));
    }
    if count == 0 {
        // An EventBridge year in the past matches nothing ever again.
        let _ = writeln!(out, "  (none — this expression will never fire again)");
    }
    let _ = writeln!(out, "\n{CRON_HELP}");
    Ok(out)
}

fn offset_label(offset: chrono::FixedOffset) -> String {
    use chrono::Offset as _;
    if offset.fix().local_minus_utc() == 0 {
        "UTC".into()
    } else {
        format!("UTC{offset}")
    }
}

pub static TOOL: Tool = Tool {
    id: "crontab-generator",
    name: "Crontab generator",
    description: "Validate and describe a crontab expression, and see its next occurrences.",
    category: Category::Development,
    keywords: &["crontab", "cron", "job", "schedule", "task", "generator"],
    fields: &[
        Field {
            key: "expression",
            label: "Cron expression",
            kind: crate::FieldKind::Text,
            default: "40 * * * *",
            options: &[],
        },
        Field::text("timezone", "Time zone (UTC or a fixed offset like +02:00)")
            .with_default("UTC"),
    ],
    kind: Kind::Pure,
    run: crontab_generator,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    #[test]
    fn crontab_describes_and_rejects() {
        let out = run("crontab-generator", &[("expression", "40 * * * *")]).unwrap();
        assert!(out.contains("Expression: 40 * * * *"), "{out}");
        assert!(out.contains("Meaning:"), "{out}");
        // it-tools always shows the field diagram and the symbol table.
        assert!(out.contains("day of week (0 - 6, sunday=0)"), "{out}");
        assert!(out.contains("@midnight"), "{out}");
        assert_eq!(
            out.lines().filter(|l| l.starts_with("  2")).count(),
            5,
            "{out}"
        );
        assert!(out.contains("Dialect: Unix cron"), "{out}");
        assert!(run("crontab-generator", &[("expression", "* * * *")]).is_err());
        assert!(run("crontab-generator", &[("expression", "99 * * * *")]).is_err());
        assert!(run("crontab-generator", &[("expression", "")]).is_err());
    }

    #[test]
    fn event_bridge_dialect() {
        // `?` marks the dialect; the year is the sixth field, not the seconds.
        let out = ok("crontab-generator", &[("expression", "0 12 * * ? *")]);
        assert!(out.contains("Dialect: AWS EventBridge"), "{out}");
        assert!(
            out.lines().filter(|l| l.starts_with("  2")).count() == 5,
            "{out}"
        );
        // A literal year is the other marker.
        let out = ok("crontab-generator", &[("expression", "0 12 * * MON 2030")]);
        assert!(out.contains("Dialect: AWS EventBridge"), "{out}");
        // Six fields with neither marker stay seconds-cron.
        let out = ok("crontab-generator", &[("expression", "0 40 * * * *")]);
        assert!(out.contains("Dialect: Unix cron"), "{out}");
        // A year that is over matches nothing, and must say so rather than
        // printing an empty list.
        let out = ok("crontab-generator", &[("expression", "0 12 * * ? 1999")]);
        assert!(out.contains("will never fire again"), "{out}");
    }

    #[test]
    fn timezone_field() {
        let out = ok(
            "crontab-generator",
            &[("expression", "40 * * * *"), ("timezone", "+02:00")],
        );
        assert!(out.contains("Next occurrences (UTC+02:00):"), "{out}");
        assert!(
            ok(
                "crontab-generator",
                &[("expression", "40 * * * *"), ("timezone", "")]
            )
            .contains("Next occurrences (UTC):")
        );
        // Named zones need a database this build does not carry; refusing beats
        // quietly pretending Europe/Berlin is UTC.
        assert!(
            run(
                "crontab-generator",
                &[("expression", "40 * * * *"), ("timezone", "Europe/Berlin")]
            )
            .is_err()
        );
    }
}
