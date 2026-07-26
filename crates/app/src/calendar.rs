//! The arithmetic the `Time Calendar` design needs and `track.rs` did not have.
//!
//! **Only the additions.** `track.rs` already owns the date arithmetic — `Ymd`,
//! `days_from_civil`, `weekday`, `week_start`, `month_name`, `step_day`,
//! `step_month`, `days_in_month` — and it is correct and tested. This module was
//! first written with its own copy of all of it, which was a mistake worth
//! recording: the duplicate got the Monday-first weekday index wrong by three
//! days, a bug the incumbent had never had. Reuse, then add.
//!
//! What is genuinely new here is what the calendar layout needs and a day list
//! does not: a **month grid of whole weeks**, an **ISO week number** for the week
//! title, **bar heights** for the year strip, and the **placement of an entry in
//! an hour grid**. Every one of those is a thing that can only be checked by a
//! test — which cell a day lands in, whether a six-row month is clipped, whether a
//! two-minute entry is too small to click — so it lives apart from the drawing.
//!
//! Everything here is drawn by something: `month_grid` by the month view,
//! `year_bars` and `MONTHS_SHORT` by the year strip, `block` and `hour_window` by
//! the week grid, `iso_week` by the week title. The `#[allow(dead_code)]` markers
//! that stood here while those views were being built are gone — an allow that
//! outlives its reason is how a module starts rotting.
use crate::track::{Ymd, days_from_civil, days_in_month, step_day, week_start};

// A `View` enum lived here and was deleted unused: `track::Scope` already is the
// view switcher, with Day / Week / Month, and a second enum for the same choice
// would have been one more thing to keep in step. Year and Day-as-a-grid are the
// remaining design views; they belong in `Scope` when they land, not in a rival.

/// Three-letter month names for the year strip, which has twelve columns and no
/// room for "September".
pub const MONTHS_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The month grid: whole weeks, Monday first, covering every day of `date`'s month
/// plus the head and tail days that share those weeks.
///
/// Always whole weeks, so the grid is rectangular — the design draws seven columns
/// and dims the outside days rather than leaving holes. The number of *rows* is
/// not fixed: a 31-day month starting on a Sunday needs six, and February in a
/// common year starting on a Monday needs exactly four. Both happen, and a
/// hard-coded five would clip one and pad the other.
pub fn month_grid(date: Ymd) -> Vec<Ymd> {
    let first = Ymd { d: 1, ..date };
    let last = Ymd {
        d: days_in_month(first.y, first.m),
        ..first
    };
    let start = week_start(first);
    let end = step_day(week_start(last), 6);
    let span = days_from_civil(end) - days_from_civil(start);
    (0..=span).map(|i| step_day(start, i)).collect()
}

/// ISO 8601 week number, for the week view's title.
///
/// Worth the six lines rather than "day of year over seven": the first days of
/// January usually belong to the last week of the previous year, and a title
/// reading "week 1" over dates in December is the kind of wrong that makes someone
/// distrust the whole screen.
pub fn iso_week(date: Ymd) -> u32 {
    let thursday = step_day(week_start(date), 3);
    let jan1 = Ymd {
        y: thursday.y,
        m: 1,
        d: 1,
    };
    ((days_from_civil(thursday) - days_from_civil(jan1)) / 7 + 1) as u32
}

/// Bar heights for the year strip, as a fraction of the busiest month.
///
/// `totals` is indexed by month − 1. An empty year is flat rather than a division
/// by zero, and a single month with time in it is full height — the strip compares
/// months against each other, and with one month there is nothing to compare it
/// to.
pub fn year_bars(totals: &[i64; 12]) -> [f32; 12] {
    let peak = totals.iter().copied().max().unwrap_or(0);
    if peak <= 0 {
        return [0.; 12];
    }
    let mut out = [0.; 12];
    for (i, total) in totals.iter().enumerate() {
        out[i] = (*total as f32 / peak as f32).clamp(0., 1.);
    }
    out
}

/// The design's hour row height, and the floor a block is never drawn below.
///
/// A block of height 0 is an entry the user cannot click — and they need to click
/// it to correct or delete it.
pub const HOUR_HEIGHT: f32 = 56.;
/// 26 px, which is the height the design gives a short entry — its own number,
/// arrived at independently by the agent that built the week grid.
pub const MIN_BLOCK: f32 = 26.;

/// Where an entry sits in the hour grid: `(top, height)` in pixels from the top of
/// the first shown hour.
///
/// `None` when the entry does not overlap the shown hours at all. One that starts
/// before them or ends after them is clamped — an entry from 06:00 to 09:00 in a
/// grid starting at 08:00 belongs at the top edge, not at a negative offset that
/// would draw it over the day header.
pub fn block(
    start_seconds_into_day: i64,
    end_seconds_into_day: i64,
    first_hour: u32,
    hours_shown: u32,
) -> Option<(f32, f32)> {
    let top_s = (first_hour as i64) * 3600;
    let bottom_s = top_s + (hours_shown as i64) * 3600;
    let from = start_seconds_into_day.max(top_s);
    let to = end_seconds_into_day.min(bottom_s);
    if to <= from {
        return None;
    }
    let per_second = HOUR_HEIGHT / 3600.;
    let top = (from - top_s) as f32 * per_second;
    let height = ((to - from) as f32 * per_second).max(MIN_BLOCK);
    // A minimum-height block near the bottom must not hang out of the grid.
    let limit = (hours_shown as f32) * HOUR_HEIGHT;
    Some((top.min(limit - height).max(0.), height))
}

/// The hours worth showing: the span the entries actually occupy, padded an hour
/// each side, never narrower than a working day and never past midnight.
///
/// A fixed 08:00–19:00 hides a night shift completely; a full 24 hours makes every
/// real day mostly empty grid. Returns `(first_hour, hours_shown)`.
pub fn hour_window(spans: &[(i64, i64)]) -> (u32, u32) {
    const MIN_HOURS: u32 = 11;
    const DEFAULT_FIRST: u32 = 8;
    let mut first = u32::MAX;
    let mut last = 0u32;
    for (from, to) in spans {
        // The end is exclusive: an entry ending exactly at 17:00 does not need the
        // 17:00 row to exist.
        let end_hour = ((to.max(&(from + 1)) - 1) / 3600) as u32;
        first = first.min((from / 3600) as u32);
        last = last.max(end_hour);
    }
    if first == u32::MAX {
        return (DEFAULT_FIRST, MIN_HOURS);
    }
    let first = first.saturating_sub(1);
    let last = (last + 1).min(23);
    let shown = (last - first + 1).max(MIN_HOURS);
    // Widening to the minimum must not run past the end of the day.
    (first.min(24 - shown), shown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::weekday;

    fn y(y: i32, m: u32, d: u32) -> Ymd {
        Ymd { y, m, d }
    }

    /// The grid is rectangular and complete: every day of the month is in it
    /// exactly once, it starts on a Monday, ends on a Sunday, and it is a whole
    /// number of weeks with no gap or repeat.
    #[test]
    fn a_month_grid_is_whole_weeks_and_holds_every_day_once() {
        for year in [2024, 2025, 2026] {
            for month in 1..=12 {
                let grid = month_grid(y(year, month, 1));
                assert_eq!(grid.len() % 7, 0, "{year}-{month} is not whole weeks");
                assert_eq!(weekday(grid[0]), 0, "{year}-{month} starts mid-week");
                assert_eq!(weekday(grid[grid.len() - 1]), 6, "{year}-{month}");
                for day in 1..=days_in_month(year, month) {
                    let wanted = y(year, month, day);
                    assert_eq!(
                        grid.iter().filter(|g| **g == wanted).count(),
                        1,
                        "{wanted:?} is not in the grid exactly once"
                    );
                }
                for pair in grid.windows(2) {
                    assert_eq!(step_day(pair[0], 1), pair[1], "{pair:?}");
                }
            }
        }
    }

    /// The row count is not five. Both extremes happen, and a fixed grid gets one
    /// of them wrong.
    #[test]
    fn the_grid_grows_to_six_rows_and_shrinks_to_four_when_it_has_to() {
        let march = y(2026, 3, 1);
        assert_eq!(weekday(march), 6, "2026-03-01 is a Sunday");
        assert_eq!(month_grid(march).len() / 7, 6);

        let feb = y(2021, 2, 1);
        assert_eq!(weekday(feb), 0, "2021-02-01 is a Monday");
        assert_eq!(month_grid(feb).len() / 7, 4);

        assert_eq!(month_grid(y(2026, 7, 1)).len() / 7, 5, "the usual case");
        // Asked from any day of the month, the grid is the same.
        assert_eq!(month_grid(y(2026, 7, 1)), month_grid(y(2026, 7, 31)));
    }

    /// The week title carries this number, so a wrong one is visible and wrong.
    #[test]
    fn iso_week_numbers_match_the_standard_at_the_year_boundary() {
        // 2026-01-01 is a Thursday, so week 1 of 2026 starts on 2025-12-29.
        assert_eq!(iso_week(y(2025, 12, 29)), 1, "belongs to 2026's week 1");
        assert_eq!(iso_week(y(2026, 1, 1)), 1);
        assert_eq!(iso_week(y(2026, 1, 4)), 1, "the Sunday of that week");
        assert_eq!(iso_week(y(2026, 1, 5)), 2);
        // 2027-01-01 is a Friday, so it is still in 2026 — week 53.
        assert_eq!(iso_week(y(2027, 1, 1)), 53);
        // A mid-year check against a calendar.
        assert_eq!(iso_week(y(2026, 7, 26)), 30);
        // Every day of a week shares its number.
        let monday = week_start(y(2026, 7, 26));
        for offset in 0..7 {
            assert_eq!(iso_week(step_day(monday, offset)), iso_week(monday));
        }
    }

    #[test]
    fn year_bars_are_relative_to_the_busiest_month_and_survive_an_empty_year() {
        let mut totals = [0i64; 12];
        totals[6] = 3600 * 40; // July
        totals[7] = 3600 * 20; // August
        let bars = year_bars(&totals);
        assert_eq!(bars[6], 1.0, "the busiest month is full height");
        assert!((bars[7] - 0.5).abs() < 1e-6);
        assert_eq!(bars[0], 0.0);
        assert_eq!(year_bars(&[0; 12]), [0.; 12], "an empty year is flat");
        assert_eq!(
            year_bars(&[-5; 12]),
            [0.; 12],
            "a negative total cannot happen but must not draw upside down"
        );
        // One month with time in it is full height, not an eleventh.
        let mut lone = [0i64; 12];
        lone[3] = 60;
        assert_eq!(year_bars(&lone)[3], 1.0);
    }

    #[test]
    fn a_block_is_placed_clamped_and_never_invisible() {
        let hour = |h: i64| h * 3600;
        assert_eq!(block(hour(9), hour(10), 8, 11), Some((56., 56.)));
        assert_eq!(block(hour(9), hour(9) + 1800, 8, 11), Some((56., 28.)));
        // Starting before the window keeps only the part inside it.
        assert_eq!(block(hour(6), hour(9), 8, 11), Some((0., 56.)));
        // Ending after it is cut at the bottom edge, exactly.
        let (top, height) = block(hour(18), hour(23), 8, 11).expect("overlaps");
        assert!((top + height - 11. * 56.).abs() < 1e-3, "{top} {height}");
        // Entirely outside, on either side.
        assert_eq!(block(hour(2), hour(5), 8, 11), None);
        assert_eq!(block(hour(20), hour(22), 8, 11), None);
        // Zero-length and inverted are not blocks.
        assert_eq!(block(hour(9), hour(9), 8, 11), None);
        assert_eq!(block(hour(10), hour(9), 8, 11), None);
        // A two-minute entry stays big enough to hit with a pointer...
        let (_, height) = block(hour(9), hour(9) + 120, 8, 11).expect("short");
        assert_eq!(height, MIN_BLOCK);
        // ...and one at the very end stays inside the grid anyway.
        let (top, height) = block(hour(18) + 3480, hour(19), 8, 11).expect("late");
        assert!(top + height <= 11. * 56. + 1e-3, "{top} + {height}");
        assert!(top >= 0.);
    }

    #[test]
    fn the_hour_window_follows_the_entries_and_stays_inside_the_day() {
        let hour = |h: i64| h * 3600;
        assert_eq!(hour_window(&[]), (8, 11), "nothing booked: a working day");
        // A normal day, padded either side.
        let (first, shown) = hour_window(&[(hour(9), hour(17))]);
        assert!(first <= 8 && first + shown >= 18, "{first} {shown}");
        // A night shift is shown rather than hidden.
        let (first, shown) = hour_window(&[(hour(22), hour(23) + 3599)]);
        assert!(first + shown <= 24, "must not run past midnight");
        assert!(first <= 21, "the entry has to be inside: {first} {shown}");
        // The very start of the day.
        assert_eq!(hour_window(&[(0, 3600)]).0, 0);
        // A long day widens past the minimum.
        let (first, shown) = hour_window(&[(hour(6), hour(23))]);
        assert!(shown > 11, "{shown}");
        assert!(first + shown <= 24);
        // Whatever it is given, the window is a valid slice of one day.
        for spans in [
            vec![],
            vec![(0, 1)],
            vec![(hour(23), hour(24))],
            vec![(hour(1), hour(2)), (hour(20), hour(21))],
        ] {
            let (first, shown) = hour_window(&spans);
            assert!(shown >= 11, "{spans:?} -> {first} {shown}");
            assert!(first + shown <= 24, "{spans:?} -> {first} {shown}");
        }
    }
}
