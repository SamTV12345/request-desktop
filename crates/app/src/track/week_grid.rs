//! The week as an hour grid, and the inspector for whatever entry was clicked in
//! it.
//!
//! A child module of `track` rather than a sibling of it, so it can read the
//! view's own state and reuse its buttons instead of growing a parallel set. The
//! pixel arithmetic is `crate::calendar`; the rest of what can be decided without
//! a window is a free function at the bottom of this file, with a test.
//!
//! Every day is placed against the *store's* local midnight — `Track::offset`,
//! the same basis `Track::clock` and `Track::day_start` use. Deriving a day from
//! the UTC clock was a real bug once (see `track::today`) and must not come back.
use super::{
    Entry, IconButton, Scope, Track, Ymd, days_from_civil, entry_day_of, fmt_hm, heading, hint,
    now, pill_button, step_day, tag, week_start, weekday_name,
};
use crate::calendar::{HOUR_HEIGHT, MIN_BLOCK, block, hour_window};
use gpui::{
    Context, IntoElement, ParentElement, Pixels, SharedString, Styled, div, prelude::*, px,
};
use rp_ui::theme::{FONT_HEADING, FONT_MONO, Theme, alpha};
use rp_ui::tokens::{radius, space, text};

/// The hour gutter's width and the gap between day columns. The design's own
/// numbers; neither is a spacing step.
const GUTTER: Pixels = px(66.);
const COL_GAP: Pixels = px(6.);
/// The corner on the wash behind today's column, and on a block too short to
/// carry the standard one. Both are the design's numbers.
const WASH_RADIUS: Pixels = px(14.);
const SHORT_RADIUS: Pixels = px(12.);
/// The inspector's width, from the design.
const INSPECTOR_W: Pixels = px(308.);
/// A block shows its note only once it has room for a third line; below that the
/// note would be a clipped sliver.
const NOTE_MIN_H: f32 = 74.0;
/// The accent wash behind today's column: `accent 7%`, as the design mixes it.
const TODAY_WASH: f32 = 0.07;

impl Track {
    /// The week: an hour grid and the inspector beside it.
    pub(super) fn week_view(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let now = now();
        let wk = week_start(self.date);
        // Clip every entry to every day it touches. A day-crossing entry is drawn
        // twice, once per day, each time with only that day's part of it — which
        // is also why no time is counted twice.
        let mut per_day: Vec<Vec<(Entry, i64, i64)>> = Vec::with_capacity(7);
        let mut spans: Vec<(i64, i64)> = Vec::new();
        for i in 0..7 {
            let start = self.local_midnight(step_day(wk, i));
            let mut here = Vec::new();
            for entry in &self.span {
                let end = entry.stopped_at.unwrap_or(now);
                if let Some(span) = within_day(entry.started_at, end, start) {
                    spans.push(span);
                    here.push((entry.clone(), span.0, span.1));
                }
            }
            per_day.push(here);
        }
        let (first_hour, hours) = hour_window(&spans);

        let mut heads = Vec::with_capacity(7);
        let mut columns = Vec::with_capacity(7);
        for (i, blocks) in per_day.into_iter().enumerate() {
            let date = step_day(wk, i as i64);
            let today = date == self.today;
            let total = self
                .week
                .iter()
                .find(|d| d.date == super::fmt_date(date))
                .map(|d| d.total_seconds)
                .unwrap_or(0);
            heads.push(self.day_head(t, date, i == 0, total, today));

            let mut column = div()
                .relative()
                .flex_1()
                .min_w_0()
                .h(px(hours as f32 * HOUR_HEIGHT));
            if today {
                column = column.rounded(WASH_RADIUS).bg(alpha(t.accent, TODAY_WASH));
            }
            for (entry, from, to) in blocks {
                // `None` is an entry outside the shown hours — not drawn at all,
                // rather than pinned to an edge where it would read as real.
                if let Some((top, height)) = block(from, to, first_hour, hours) {
                    column = column.child(self.entry_block(t, &entry, top, height, cx));
                }
            }
            columns.push(column);
        }

        let mut gutter = Vec::with_capacity(hours as usize);
        let mut rules = Vec::with_capacity(hours as usize);
        for hour in first_hour..first_hour + hours {
            gutter.push(
                div()
                    .h(px(HOUR_HEIGHT))
                    .flex_none()
                    .child(SharedString::from(format!("{hour:02}:00"))),
            );
            rules.push(
                div()
                    .h(px(HOUR_HEIGHT))
                    .border_t_1()
                    .border_color(t.divider),
            );
        }

        div()
            .flex()
            .flex_1()
            .min_h_0()
            .gap(space::S3)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .rounded(radius::LG)
                    .bg(t.surface)
                    .child(
                        div()
                            .flex()
                            .px(space::S4)
                            .pt(space::S3)
                            .pb(space::S3)
                            .gap(COL_GAP)
                            // The spacer keeps the day names off the gutter, so
                            // every heading sits over its own column.
                            .child(div().w(GUTTER).flex_none())
                            .children(heads),
                    )
                    .child(
                        div()
                            .id("week-hours")
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .px(space::S4)
                            .pt(space::S2)
                            .pb(space::S4)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_none()
                                    .w(GUTTER)
                                    .font_family(FONT_MONO)
                                    .text_size(text::TINY)
                                    .text_color(t.text_meta())
                                    .children(gutter),
                            )
                            .child(
                                div()
                                    .relative()
                                    .flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap(COL_GAP)
                                    // The rules are a layer of their own behind
                                    // the columns: one element per hour beats
                                    // giving every column its own borders.
                                    .child(
                                        div().absolute().top_0().left_0().right_0().children(rules),
                                    )
                                    .children(columns),
                            ),
                    ),
            )
            .child(self.inspector(t, now, cx))
    }

    /// One day's heading: the day, and what was booked on it.
    fn day_head(
        &self,
        t: Theme,
        date: Ymd,
        first: bool,
        total: i64,
        today: bool,
    ) -> impl IntoElement + use<> {
        let ink = if today { t.accent_ramp.s700 } else { t.text };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(px(3.))
            // A day with nothing booked recedes as a whole, rather than only its
            // dash: the week is read for where the work is.
            .when(total == 0 && !today, |d| d.opacity(0.45))
            .child(
                div()
                    .truncate()
                    .text_size(text::TINY)
                    .text_color(if today { ink } else { t.text_muted() })
                    .child(SharedString::from(day_label(date, first).to_uppercase())),
            )
            .child(
                div()
                    .font_family(FONT_MONO)
                    .text_size(text::SMALL)
                    .text_color(if today { ink } else { t.text_label() })
                    .child(SharedString::from(if total == 0 {
                        "–".to_string()
                    } else {
                        fmt_hm(total)
                    })),
            )
    }

    /// One entry, placed in its day column. Clicking it fills the inspector.
    fn entry_block(
        &self,
        t: Theme,
        entry: &Entry,
        top: f32,
        height: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = entry.id;
        // A running entry is the one the accent is for; a booked one is the
        // second categorical role, so the two never look like the same thing.
        let (bg, ink) = if entry.is_running() {
            (t.accent, t.on_accent())
        } else {
            (t.accent_2_ramp.s200, t.accent_2_ramp.s900)
        };
        let short = height <= MIN_BLOCK;
        let range = match entry.stopped_at {
            Some(end) => format!("{}–{}", self.clock(entry.started_at), self.clock(end)),
            None => format!("{}–now", self.clock(entry.started_at)),
        };
        let note = entry.note.clone().filter(|_| height >= NOTE_MIN_H);
        div()
            .id(SharedString::from(format!("wk-{id}")))
            .absolute()
            .top(px(top))
            .left_0()
            .right_0()
            .h(px(height))
            .overflow_hidden()
            .rounded(if short { SHORT_RADIUS } else { radius::MD })
            .px(space::S3)
            .py(space::S2)
            .bg(bg)
            .text_color(ink)
            // A transparent border always, so selecting a block cannot move the
            // text inside it by two pixels.
            .border_2()
            .border_color(if self.selected == Some(id) {
                ink
            } else {
                alpha(bg, 0.)
            })
            .child(
                div()
                    .truncate()
                    .text_size(text::SMALL)
                    .child(SharedString::from(entry.task.clone())),
            )
            .when(!short, |d| {
                d.child(
                    div()
                        .truncate()
                        .font_family(FONT_MONO)
                        .text_size(text::TINY)
                        .text_color(alpha(ink, 0.8))
                        .child(SharedString::from(range)),
                )
            })
            .when_some(note.filter(|_| !short), |d, note| {
                d.child(
                    div()
                        .truncate()
                        .text_size(text::TINY)
                        .text_color(alpha(ink, 0.75))
                        .child(SharedString::from(note)),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected = Some(id);
                cx.notify();
            }))
    }

    /// What the grid selected, in full. The two actions are the ones the entry
    /// table already offers, so an entry is edited the same way from either.
    fn inspector(&self, t: Theme, now: i64, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let card = div()
            .flex()
            .flex_col()
            .flex_none()
            .w(INSPECTOR_W)
            .gap(space::S3)
            .px(space::S6)
            .py(space::S5)
            .rounded(radius::LG)
            .bg(t.surface)
            .child(
                div()
                    .text_size(text::TINY)
                    .text_color(t.text_muted())
                    .child("SELECTED ENTRY"),
            );
        let Some(entry) = self.selected_entry() else {
            return card
                .child(heading(t, "Nothing selected"))
                .child(hint(t, "Click a block in the week to inspect it."))
                .into_any_element();
        };
        let (id, running) = (entry.id, entry.is_running());
        let (tag_bg, tag_ink) = t.tag_accent_2();
        let (id_bg, id_ink) = t.tag_neutral();
        let end = match entry.stopped_at {
            Some(end) => self.clock(end),
            None => "running".to_string(),
        };
        let clone = entry.clone();
        card.child(
            div()
                .font_family(FONT_HEADING)
                .text_size(text::H4)
                .text_color(t.text)
                .child(SharedString::from(entry.task.clone())),
        )
        .child(
            div()
                .flex()
                .gap(space::S2)
                .child(tag(
                    SharedString::from(entry.source.clone()),
                    tag_bg,
                    tag_ink,
                ))
                .child(
                    tag(SharedString::from(format!("#{id}")), id_bg, id_ink).font_family(FONT_MONO),
                ),
        )
        .child(field(t, "START", self.clock(entry.started_at)))
        .child(field(t, "END", end))
        .child(field(t, "DURATION", fmt_hm(entry.seconds(now))))
        .child(field(
            t,
            "NOTE",
            entry.note.clone().unwrap_or_else(|| "—".to_string()),
        ))
        .child(
            div()
                .flex()
                .items_center()
                .gap(space::S2)
                .child(
                    // A running entry has no end yet, so there is no range for
                    // the form to edit — the timer's Stop is what finishes it.
                    div().flex_1().when(!running, |d| {
                        d.child(pill_button(
                            t,
                            "inspect-edit".into(),
                            "Edit".into(),
                            // The inline form lives in the day's entry table, so
                            // editing steps to the day it belongs to first —
                            // otherwise the form opens where nothing draws it.
                            cx.listener(move |this, _, _, cx| {
                                this.go_to(
                                    entry_day_of(clone.started_at, this.offset),
                                    Scope::Day,
                                    cx,
                                );
                                this.edit(Some(&clone), cx);
                            }),
                        ))
                    }),
                )
                .child(
                    IconButton::new("inspect-delete", "trash")
                        .render(t, cx.listener(move |this, _, _, cx| this.delete(id, cx))),
                ),
        )
        .into_any_element()
    }

    /// The selected entry, if it is still there. Selection is by id, so an entry
    /// deleted by the other process disappears from here on the next reload
    /// instead of lingering as a stale card.
    fn selected_entry(&self) -> Option<&Entry> {
        let id = self.selected?;
        self.span.iter().find(|e| e.id == id)
    }

    /// Local midnight of `date`, on the same basis as [`Track::day_start`].
    ///
    /// ponytail: one offset for the whole week, so the two days a year a DST
    /// change lands inside the shown week put that week's columns an hour off.
    /// The upgrade path is probing the store per day — seven bisects a reload.
    fn local_midnight(&self, date: Ymd) -> i64 {
        days_from_civil(date) * 86_400 + self.offset
    }
}

/// One labelled value in the inspector.
fn field(t: Theme, label: &'static str, value: String) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .text_size(text::TINY)
                .text_color(t.text_meta())
                .child(label),
        )
        .child(
            div()
                .text_size(text::SMALL)
                .text_color(t.text)
                .child(SharedString::from(value)),
        )
}

// ---------------------------------------------------------------------------
// Pure logic.
// ---------------------------------------------------------------------------

/// The part of `[start, end)` that falls inside the local day beginning at
/// `day_start`, as seconds into that day. `None` when the entry does not touch
/// the day at all.
///
/// This is what puts an entry in the right column *and* keeps a day-crossing one
/// whole: each day gets its own clipped part, so the two halves add up to the
/// entry and to nothing more.
fn within_day(start: i64, end: i64, day_start: i64) -> Option<(i64, i64)> {
    let day_end = day_start + 86_400;
    if end <= day_start || start >= day_end {
        return None;
    }
    Some((
        start.max(day_start) - day_start,
        end.min(day_end) - day_start,
    ))
}

/// A day column's heading. The first column of the week and the first day of a
/// month spell the month out — `Wed 1 Jul` — because that is where a reader loses
/// track of which month they are looking at.
fn day_label(date: Ymd, first: bool) -> String {
    let day = &weekday_name(date)[..3];
    if first || date.d == 1 {
        format!("{day} {} {}", date.d, &super::month_name(date)[..3])
    } else {
        format!("{day} {}", date.d)
    }
}

/// The selection that survives a reload: the same id, or nothing when the entry
/// it named is gone.
pub(super) fn keep_selected(selected: Option<i64>, entries: &[Entry]) -> Option<i64> {
    selected.filter(|id| entries.iter().any(|e| e.id == *id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::HOUR_HEIGHT;

    /// A local midnight that is not a UTC one, so a test that quietly assumed UTC
    /// fails here.
    const MIDNIGHT: i64 = 1_753_484_400 + 7_200;

    fn entry(id: i64, started_at: i64, stopped_at: Option<i64>) -> Entry {
        Entry {
            id,
            task: "t".into(),
            note: None,
            started_at,
            stopped_at,
            source: "app".into(),
        }
    }

    #[test]
    fn an_entry_across_midnight_belongs_to_both_days_and_is_counted_once() {
        // 23:30 to 00:30 the next day.
        let (start, end) = (MIDNIGHT - 1_800, MIDNIGHT + 1_800);
        let before = within_day(start, end, MIDNIGHT - 86_400).unwrap();
        let after = within_day(start, end, MIDNIGHT).unwrap();
        assert_eq!(before, (86_400 - 1_800, 86_400));
        assert_eq!(after, (0, 1_800));
        // Each day holds its own half, and the halves are the whole entry.
        let counted = (before.1 - before.0) + (after.1 - after.0);
        assert_eq!(counted, end - start);
        // And it belongs to no other day.
        assert!(within_day(start, end, MIDNIGHT + 86_400).is_none());
        assert!(within_day(start, end, MIDNIGHT - 2 * 86_400).is_none());
    }

    #[test]
    fn the_column_an_entry_belongs_to_is_the_day_it_starts_in() {
        // 00:00 exactly is this day, not the previous one's midnight.
        assert_eq!(within_day(MIDNIGHT, MIDNIGHT + 60, MIDNIGHT), Some((0, 60)));
        assert!(within_day(MIDNIGHT, MIDNIGHT + 60, MIDNIGHT - 86_400).is_none());
        // 23:59 for a minute ends the day and does not spill into the next.
        let (start, end) = (MIDNIGHT + 86_340, MIDNIGHT + 86_400);
        assert_eq!(within_day(start, end, MIDNIGHT), Some((86_340, 86_400)));
        assert!(within_day(start, end, MIDNIGHT + 86_400).is_none());

        // The day the inspector's Edit steps to is the day the entry *starts* in,
        // read through the store's offset and not off the UTC clock. With an
        // offset that makes MIDNIGHT a local midnight, 00:00 and 23:59 are the
        // same day and the next midnight is the next one.
        let offset = MIDNIGHT.rem_euclid(86_400) - 86_400;
        let day = entry_day_of(MIDNIGHT, offset);
        assert_eq!(entry_day_of(start, offset), day);
        assert_eq!(entry_day_of(MIDNIGHT + 86_400, offset), step_day(day, 1));
        // The same instant read as if it were UTC lands on the wrong date, which
        // is the bug this offset exists to prevent.
        assert_ne!(entry_day_of(MIDNIGHT, 0), day);
    }

    #[test]
    fn an_entry_outside_the_shown_hours_is_not_placed() {
        // 03:00–03:30 on a day whose window is the default 08:00–19:00.
        let (from, to) = within_day(MIDNIGHT + 10_800, MIDNIGHT + 12_600, MIDNIGHT).unwrap();
        let (first, hours) = hour_window(&[(8 * 3_600, 9 * 3_600)]);
        assert!(block(from, to, first, hours).is_none());
    }

    #[test]
    fn a_running_entry_is_placed_and_stops_at_now() {
        let e = entry(1, MIDNIGHT + 9 * 3_600, None);
        let now = MIDNIGHT + 9 * 3_600 + 1_800;
        let (from, to) = within_day(e.started_at, e.stopped_at.unwrap_or(now), MIDNIGHT).unwrap();
        let (first, hours) = hour_window(&[(from, to)]);
        let (top, height) = block(from, to, first, hours).unwrap();
        assert_eq!(
            height,
            HOUR_HEIGHT / 2.0,
            "half an hour, not the rest of the day"
        );
        assert!(top > 0.0, "09:00 is not the top of a padded window");
        assert!(top + height < hours as f32 * HOUR_HEIGHT);
    }

    #[test]
    fn the_first_column_and_every_first_of_a_month_spell_the_month() {
        let ymd = |y, m, d| Ymd { y, m, d };
        // A week entirely inside one month: only the first column spells it.
        assert_eq!(day_label(ymd(2026, 7, 20), true), "Mon 20 Jul");
        assert_eq!(day_label(ymd(2026, 7, 21), false), "Tue 21");
        // A week that crosses into August: the 1st spells its new month.
        assert_eq!(day_label(ymd(2026, 7, 27), true), "Mon 27 Jul");
        assert_eq!(day_label(ymd(2026, 7, 31), false), "Fri 31");
        assert_eq!(day_label(ymd(2026, 8, 1), false), "Sat 1 Aug");
        assert_eq!(day_label(ymd(2026, 8, 2), false), "Sun 2");
    }

    #[test]
    fn selection_survives_a_reload_but_not_the_entry_going_away() {
        let entries = vec![entry(7, MIDNIGHT, Some(MIDNIGHT + 60))];
        assert_eq!(keep_selected(Some(7), &entries), Some(7));
        assert_eq!(keep_selected(Some(8), &entries), None);
        assert_eq!(keep_selected(Some(7), &[]), None);
        assert_eq!(keep_selected(None, &entries), None);
    }
}
