//! The year strip: twelve bars, one per month, that takes the place of the
//! per-day chart when the Time section is looking at a month.
//!
//! A per-day chart of the month sits directly under a table of the same month's
//! days, which says the same thing twice. Zoomed out one step it says something
//! the table cannot: whether this month is a normal one.
//!
//! The arithmetic is [`crate::calendar`]'s — this module only draws it and
//! collects the totals to feed it.
use crate::calendar::{MONTHS_SHORT, year_bars};
use gpui::{
    App, Div, ParentElement, Pixels, SharedString, Stateful, Styled, Window, div, prelude::*, px,
};
use rp_store::DaySummary;
use rp_ui::theme::{FONT_MONO, Theme};
use rp_ui::tokens::{space, text};

// The design's own numbers. None of them is on a token scale: the bar is a fixed
// 34-px track with a 10-px corner and its caption is 10.5 px.
//
// `BTN_H = px(32.)` used to sit here for this file's own `secondary_button`. That
// button is `rp_ui::Button` with `Size::Sm` now, whose height is `control::SM` —
// so the loose 32 is gone rather than promoted to a token.
const BAR_H: Pixels = px(34.);
const BAR_RADIUS: Pixels = px(10.);
const CAPTION: Pixels = px(10.5);

/// The twelve monthly totals of `year`, summed from the store's per-day
/// summaries. `load` takes a `YYYY-MM` key — i.e. `Store::track_summary_month`.
///
/// Index 0 is January and index 11 December, which is the indexing
/// [`year_bars`] and [`MONTHS_SHORT`] already use; getting that off by one would
/// draw December's work under "Nov" and be believed.
pub fn year_totals(
    year: i32,
    mut load: impl FnMut(&str) -> anyhow::Result<Vec<DaySummary>>,
) -> anyhow::Result<[i64; 12]> {
    let mut out = [0i64; 12];
    for (i, slot) in out.iter_mut().enumerate() {
        let days = load(&format!("{:04}-{:02}", year, i + 1))?;
        *slot = days.iter().map(|d| d.total_seconds).sum();
    }
    Ok(out)
}

/// The strip: twelve fixed-height tracks with a bottom-aligned fill.
///
/// `current` is the index of the month the pane is showing, if it is in `year`.
///
/// The fraction is drawn as the *filled part* of a fixed 34-px bar rather than
/// as the bar's height, which is the design's shape and also the readable one: a
/// bar whose height ran from 0 to 34 px would make an empty month invisible —
/// nothing to click, nothing to compare — and a quiet month a two-pixel smear
/// that cannot be told from an empty one. The track is always there; only how
/// full it is changes.
pub fn strip(t: Theme, totals: &[i64; 12], current: Option<usize>) -> Div {
    let bars = year_bars(totals);
    let mut columns = Vec::with_capacity(MONTHS_SHORT.len());
    for (i, name) in MONTHS_SHORT.iter().enumerate() {
        let now = current == Some(i);
        let worked = totals[i] > 0;
        let fill = if now { t.accent } else { t.accent_2_ramp.s400 };
        // The shown month is washed rather than filled, so it still reads as the
        // one on screen in a month where nothing has been booked yet.
        let track = if now { t.accent_wash() } else { t.hover() };
        let ink = if now {
            t.accent_ramp.s700
        } else if worked {
            t.text_muted()
        } else {
            t.text_disabled()
        };
        columns.push(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(space::S1)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .justify_end()
                        .w_full()
                        .h(BAR_H)
                        .rounded(BAR_RADIUS)
                        .bg(track)
                        .when(worked, |d| {
                            d.child(
                                div()
                                    .w_full()
                                    .h(BAR_H * bars[i])
                                    .rounded(BAR_RADIUS)
                                    .bg(fill),
                            )
                        }),
                )
                .child(
                    div()
                        .font_family(FONT_MONO)
                        .text_size(CAPTION)
                        .text_color(ink)
                        .child(SharedString::from(*name)),
                ),
        );
    }
    div().flex().items_end().gap(space::S1).children(columns)
}

/// A previous year, as text that navigates. Two of these sit beside the strip,
/// which is the whole year picker the design has — a year is one click away, not
/// twelve presses of ‹.
pub fn year_link(
    t: Theme,
    year: i32,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("year-{year}")))
        .flex_none()
        .text_size(text::SMALL)
        .text_color(t.text_muted())
        .hover(|s| s.text_color(t.accent))
        .child(SharedString::from(year.to_string()))
        .on_click(on_click)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(date: &str, seconds: i64) -> DaySummary {
        DaySummary {
            date: date.into(),
            total_seconds: seconds,
            per_task: vec![("work".into(), seconds)],
        }
    }

    /// The keys asked for and the slots they land in. A year is twelve queries
    /// and each one has exactly one home; an off-by-one here mislabels every bar
    /// and nothing on screen says so.
    #[test]
    fn the_twelve_months_are_asked_for_in_order_and_land_where_they_belong() {
        let mut asked = Vec::new();
        let totals = year_totals(2026, |key| {
            asked.push(key.to_string());
            Ok(match key {
                "2026-07" => vec![day("2026-07-01", 3_600), day("2026-07-02", 1_800)],
                _ => Vec::new(),
            })
        })
        .expect("no store error");
        assert_eq!(asked.first().map(String::as_str), Some("2026-01"));
        assert_eq!(asked.last().map(String::as_str), Some("2026-12"));
        assert_eq!(asked.len(), 12);
        assert_eq!(totals[6], 5_400, "July is index 6, and its days sum");
        assert_eq!(totals.iter().filter(|t| **t > 0).count(), 1);
    }

    /// December is index 11 and January index 0 — the boundary the strip would
    /// otherwise be one month out at, in either direction.
    #[test]
    fn december_and_january_are_the_two_ends_and_not_each_other() {
        let totals = year_totals(2026, |key| {
            Ok(match key {
                "2026-01" => vec![day("2026-01-01", 60)],
                "2026-12" => vec![day("2026-12-31", 120)],
                // The neighbouring years must not leak in.
                "2025-12" | "2027-01" => vec![day("leak", 99_999)],
                _ => Vec::new(),
            })
        })
        .expect("no store error");
        assert_eq!(totals[0], 60, "January");
        assert_eq!(totals[11], 120, "December");
        assert_eq!(totals.iter().sum::<i64>(), 180, "nothing else got in");
        // And the bars agree about which end is which.
        let bars = year_bars(&totals);
        assert_eq!(bars[11], 1.0, "December is the busier of the two");
        assert!(bars[0] > 0. && bars[0] < 1.0);
    }

    #[test]
    fn an_empty_year_is_twelve_zeroes_rather_than_no_strip() {
        let totals = year_totals(2026, |_| Ok(Vec::new())).expect("no store error");
        assert_eq!(totals, [0; 12]);
        assert_eq!(year_bars(&totals), [0.; 12]);
        // A failing query is reported, not silently drawn as an empty year.
        assert!(year_totals(2026, |_| anyhow::bail!("no database")).is_err());
    }
}
