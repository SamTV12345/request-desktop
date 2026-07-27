//! The Time rail's two facets — which source booked the time, and what kind of
//! work it was — and the rows that draw them.
//!
//! Neither facet is a stored field. The source is (`app` or `mcp`), but "kind" is
//! read out of the task name, because that is where the demo data and every real
//! timesheet put it: `Feature: parser`, `Bug Fix: eof`. Inferring it beats asking
//! the user to categorise twice, and it costs nothing when they don't.
use gpui::{
    App, Div, ParentElement, Pixels, Rgba, SharedString, Stateful, Styled, Window, div, prelude::*,
    px,
};
use rp_store::Entry;
use rp_ui::layout::section_label;
use rp_ui::theme::{FONT_MONO, Theme};
use rp_ui::tokens::{radius, space, text};

/// The design's dot: a rounded square, not a circle, and 6 px is its own number.
const DOT_RADIUS: Pixels = px(6.);

/// The kind of a task: whatever it says before the first `": "`.
///
/// The separator is `": "` and not `':'` on purpose — a task called
/// `standup 09:00: notes` has a kind, one called `09:00 standup` does not, and
/// splitting on the bare colon would invent a kind named `09`. A task with no
/// prefix is its own kind rather than being swept into an "other" bin: the user
/// named it, so it can name itself.
/// The split comes before the trim, not after: trimming first eats the space in
/// `"Feature: "` and leaves `"Feature:"`, which then has no separator to find and
/// becomes a kind with a colon stuck on the end.
pub fn kind_of(task: &str) -> &str {
    match task.split_once(": ") {
        Some((prefix, _)) if !prefix.trim().is_empty() => prefix.trim(),
        _ => task.trim(),
    }
}

/// What the rail is filtering to. `None` on both sides is everything, which is
/// the state the rail has to be one click away from at all times — hence
/// [`Filters::toggle`] rather than a select.
#[derive(Default, Clone, PartialEq, Eq)]
pub struct Filters {
    pub source: Option<String>,
    pub kind: Option<String>,
}

impl Filters {
    pub fn any(&self) -> bool {
        self.source.is_some() || self.kind.is_some()
    }

    /// Clicking the row that is already on turns it off; clicking another
    /// switches to it. One slot per facet, so a facet is never "two of three".
    pub fn toggle(slot: &mut Option<String>, value: &str) {
        *slot = match slot.as_deref() {
            Some(current) if current == value => None,
            _ => Some(value.to_string()),
        };
    }

    /// Every active facet has to pass: the two narrow together, they do not
    /// widen. An inactive facet passes everything.
    pub fn keeps(&self, entry: &Entry) -> bool {
        self.source.as_deref().is_none_or(|s| s == entry.source)
            && self
                .kind
                .as_deref()
                .is_none_or(|k| k == kind_of(&entry.task))
    }
}

/// The entries a filter leaves, in the order they came in.
///
/// Cloned rather than borrowed because the caller holds the result across a
/// render while `&self` is already borrowed elsewhere; a span is at most a
/// month of entries.
pub fn retain(entries: &[Entry], filters: &Filters) -> Vec<Entry> {
    entries
        .iter()
        .filter(|e| filters.keeps(e))
        .cloned()
        .collect()
}

/// `(source, seconds)` for every source in `sources`, whether or not it booked
/// anything.
///
/// Sources that booked nothing keep their row: the rows are how the filter is
/// reached, and a facet that appears and disappears as the month changes is a
/// control the user cannot learn.
pub fn source_totals(
    entries: &[Entry],
    sources: &[&'static str],
    now: i64,
) -> Vec<(&'static str, i64)> {
    let mut out = Vec::with_capacity(sources.len());
    for source in sources {
        let total = entries
            .iter()
            .filter(|e| e.source == *source)
            .map(|e| e.seconds(now))
            .sum();
        out.push((*source, total));
    }
    out
}

/// `(kind, entry count)`, busiest first and alphabetical within a tie so the
/// rows do not swap places between two reloads that say the same thing.
///
/// A count, not a duration: the kinds answer "what have I been doing", and a
/// running entry with no end yet still counts as one piece of work.
pub fn kind_counts(entries: &[Entry]) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for entry in entries {
        let kind = kind_of(&entry.task);
        // A task with no name at all is not a kind — it would draw a row with
        // nothing to click and nothing to read.
        if kind.is_empty() {
            continue;
        }
        match out.iter_mut().find(|(k, _)| k == kind) {
            Some(slot) => slot.1 += 1,
            None => out.push((kind.to_string(), 1)),
        }
    }
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

/// The dot beside a source. Organic has no categorical palette, so the two
/// sources are separated by family the way `method_color` separates
/// methods — and by the same family the `mcp` tag already uses in the entry
/// list, or the rail would colour-code one thing two ways.
pub fn source_color(t: Theme, source: &str) -> Rgba {
    if source == "mcp" {
        t.accent_2
    } else {
        t.accent
    }
}

/// The dot beside a kind. There is no meaning to attach, so the ramps are walked
/// in a fixed order by position — stable per list, which is all a legend needs.
pub fn kind_color(t: Theme, index: usize) -> Rgba {
    match index % 3 {
        0 => t.accent_ramp.s500,
        1 => t.accent_2_ramp.s500,
        _ => t.neutral.s500,
    }
}

/// One facet row: the dot, the name, and the number the row stands for.
///
/// Active is a wash and accent ink rather than a tick — the row *is* the control,
/// so it has to look pressed rather than carry a second widget.
pub fn facet_row(
    t: Theme,
    id: SharedString,
    dot: Rgba,
    label: SharedString,
    value: SharedString,
    active: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(space::S2)
        .px(space::S2)
        .py(space::S1)
        .rounded(radius::PILL)
        .when(active, |d| d.bg(t.accent_wash()))
        .hover(|s| {
            s.bg(if active {
                t.accent_wash_strong()
            } else {
                t.hover()
            })
        })
        .child(div().size(space::S2).rounded(DOT_RADIUS).bg(dot))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(text::SMALL)
                .text_color(if active { t.accent_ramp.s700 } else { t.text })
                .child(label),
        )
        .child(
            div()
                .flex_none()
                .font_family(FONT_MONO)
                .text_size(text::TINY)
                .text_color(t.text_muted())
                .child(value),
        )
        .on_click(on_click)
}

/// A rail facet section: the `h6` and its rows.
pub fn section(t: Theme, label: &'static str, rows: Vec<Stateful<Div>>) -> Div {
    div()
        .flex()
        .flex_col()
        .child(section_label(t, label).px_0().pt_0())
        .child(div().flex().flex_col().gap(space::S1).children(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i64, task: &str, source: &str, seconds: i64) -> Entry {
        Entry {
            id,
            task: task.into(),
            note: None,
            started_at: 0,
            stopped_at: Some(seconds),
            source: source.into(),
        }
    }

    #[test]
    fn a_kind_is_the_prefix_and_the_odd_names_do_not_invent_one() {
        assert_eq!(kind_of("Feature: parser"), "Feature");
        assert_eq!(kind_of("Bug Fix: eof in a string"), "Bug Fix");
        // No prefix at all: the task is its own kind, not dropped.
        assert_eq!(kind_of("Review & Deploy"), "Review & Deploy");
        // Only the *first* separator splits, so a colon in the detail stays put.
        assert_eq!(kind_of("Bug Fix: parser: eof: again"), "Bug Fix");
        // A bare colon is not a separator, or every clock time becomes a kind.
        assert_eq!(kind_of("09:00 standup"), "09:00 standup");
        assert_eq!(kind_of("Feature:parser"), "Feature:parser");
        // Nothing but a prefix is still that prefix — the separator has to be
        // found before the name is trimmed, or the colon comes along with it.
        assert_eq!(kind_of("Feature: "), "Feature");
        assert_eq!(kind_of("  Feature:   "), "Feature");
        assert_eq!(kind_of("Feature:"), "Feature:", "no separator, no split");
        // Nothing at all is nothing — `kind_counts` drops it rather than drawing
        // a nameless row.
        assert_eq!(kind_of(""), "");
        assert_eq!(kind_of("   "), "");
        assert_eq!(kind_of(": orphan"), ": orphan");
    }

    fn span() -> Vec<Entry> {
        vec![
            entry(1, "Feature: parser", "app", 3_600),
            entry(2, "Feature: rail", "mcp", 1_800),
            entry(3, "Bug Fix: eof", "app", 600),
            entry(4, "Review & Deploy", "mcp", 900),
        ]
    }

    #[test]
    fn no_filter_keeps_everything() {
        let f = Filters::default();
        assert!(!f.any());
        assert_eq!(retain(&span(), &f).len(), 4);
    }

    #[test]
    fn one_filter_narrows_and_two_narrow_together() {
        let f = Filters {
            source: Some("app".into()),
            ..Default::default()
        };
        assert!(f.any());
        assert_eq!(ids(&retain(&span(), &f)), vec![1, 3]);

        let f = Filters {
            kind: Some("Feature".into()),
            ..Default::default()
        };
        assert_eq!(ids(&retain(&span(), &f)), vec![1, 2]);

        // Both: an AND, not a union — `mcp` alone would be 2 and 4, `Feature`
        // alone 1 and 2.
        let f = Filters {
            source: Some("mcp".into()),
            kind: Some("Feature".into()),
        };
        assert_eq!(ids(&retain(&span(), &f)), vec![2]);
    }

    /// A combination nothing matches has to come back empty. The tempting bug is
    /// a filter that falls back to "everything" when it finds nothing, which
    /// silently shows the user the opposite of what they asked for.
    #[test]
    fn a_filter_that_matches_nothing_yields_nothing() {
        let f = Filters {
            source: Some("mcp".into()),
            kind: Some("Bug Fix".into()),
        };
        assert!(retain(&span(), &f).is_empty());
        let f = Filters {
            source: Some("carrier pigeon".into()),
            kind: None,
        };
        assert!(retain(&span(), &f).is_empty());
        assert!(retain(&[], &Filters::default()).is_empty());
    }

    #[test]
    fn toggling_a_facet_off_is_how_everything_comes_back() {
        let mut f = Filters::default();
        Filters::toggle(&mut f.source, "app");
        assert_eq!(f.source.as_deref(), Some("app"));
        // The same row again clears it.
        Filters::toggle(&mut f.source, "app");
        assert_eq!(f.source, None);
        // A different row switches rather than accumulating.
        Filters::toggle(&mut f.source, "app");
        Filters::toggle(&mut f.source, "mcp");
        assert_eq!(f.source.as_deref(), Some("mcp"));
        assert!(f.any());
        f = Filters::default();
        assert!(!f.any());
    }

    fn ids(entries: &[Entry]) -> Vec<i64> {
        entries.iter().map(|e| e.id).collect()
    }

    #[test]
    fn source_rows_are_every_source_and_sum_the_span() {
        let totals = source_totals(&span(), &["app", "mcp"], 0);
        assert_eq!(totals, vec![("app", 4_200), ("mcp", 2_700)]);
        // A source with nothing booked keeps its row, at zero.
        let totals = source_totals(&[], &["app", "mcp"], 0);
        assert_eq!(totals, vec![("app", 0), ("mcp", 0)]);
    }

    #[test]
    fn kind_rows_count_entries_busiest_first() {
        assert_eq!(
            kind_counts(&span()),
            vec![
                ("Feature".to_string(), 2),
                ("Bug Fix".to_string(), 1),
                ("Review & Deploy".to_string(), 1),
            ],
            "ties are alphabetical so the rows hold still"
        );
        // A nameless task is not a row.
        let odd = vec![
            entry(9, "  ", "app", 60),
            entry(10, "Feature: x", "app", 60),
        ];
        assert_eq!(kind_counts(&odd), vec![("Feature".to_string(), 1)]);
        assert!(kind_counts(&[]).is_empty());
    }
}
