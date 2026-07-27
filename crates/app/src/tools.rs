//! What the tools sidebar shows, decided without a window.
//!
//! The list used to be 86 rows in one flat run under a single "Tools" heading,
//! which is findable only by searching — you cannot skim it, and nothing tells
//! you a category exists. The redesign groups them and puts the ones you keep
//! coming back to at the top. That grouping is arithmetic on a slice, so it lives
//! here with tests rather than inside `render`, where gpui gives no way to test
//! anything (this workspace has no `test-support`).
//!
//! Favourites are stored as **one `kv` row**, newline-separated ids. No table and
//! no migration: `kv` has been there since v1, the list is short, and its *order*
//! is meaningful — it is the order of the ⌘1…⌘9 shortcuts — which a set-shaped
//! table would have thrown away.
use rp_devtools::{Category, Tool};

/// The `kv` key the favourites list lives under.
pub const FAVORITES_KEY: &str = "tool_favorites";

/// How many favourites get a numbered shortcut. Nine, because ⌘0 is not a tenth
/// and a two-digit chord for a shortcut list is worse than no shortcut.
pub const MAX_SHORTCUTS: usize = 9;

/// One heading and the rows under it.
///
/// No `Debug`/`PartialEq`: `rp_devtools::Tool` derives neither (it holds a `fn`
/// pointer), so the tests compare ids and labels rather than whole groups.
pub struct Group {
    pub label: &'static str,
    /// The rows, in the order they are drawn.
    pub tools: Vec<&'static Tool>,
    /// `true` for the favourites group, which is drawn with a star and whose rows
    /// carry a dot and a shortcut hint.
    pub pinned: bool,
}

/// Parse the stored list. Unknown ids are dropped here rather than at every use:
/// a favourite can outlive the tool it names — across a downgrade, or if a tool is
/// renamed — and a dangling id would otherwise be an unclickable row.
pub fn favorites_from(stored: &str, known: impl Fn(&str) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    for id in stored.lines().map(str::trim).filter(|s| !s.is_empty()) {
        // Also de-duplicates: two ⌘-slots pointing at one tool is a corrupt list,
        // not a feature.
        if known(id) && !out.iter().any(|k| k == id) {
            out.push(id.to_string());
        }
    }
    out
}

pub fn favorites_to(list: &[String]) -> String {
    list.join("\n")
}

/// Add or remove `id`, keeping the order of everything else.
///
/// A new favourite goes to the **end**, so pinning a tool never renumbers the
/// shortcuts you have already learned.
pub fn toggle(list: &mut Vec<String>, id: &str) {
    if let Some(at) = list.iter().position(|k| k == id) {
        list.remove(at);
    } else {
        list.push(id.to_string());
    }
}

/// One tag in the filter row.
pub struct Tag {
    pub label: &'static str,
    /// `None` is the "All" tag, which clears the filter.
    pub category: Option<Category>,
    /// Only "All" carries one, as in the design.
    pub count: Option<usize>,
    pub on: bool,
}

/// The filter row for `matches`.
///
/// **`matches` must be the search results *before* the category filter is
/// applied.** Building the row from the filtered set would leave only the selected
/// tag standing, so picking "Crypto" would hide every other category and the only
/// way back would be a tag that is no longer beside them. The row has to keep
/// offering what the search found.
///
/// Only categories that actually occur are offered — after a search for "json"
/// there is nothing behind "Images" but an empty list.
pub fn tags(matches: &[&'static Tool], active: Option<Category>) -> Vec<Tag> {
    let mut out = vec![Tag {
        label: "All",
        category: None,
        count: Some(matches.len()),
        on: active.is_none(),
    }];
    for cat in Category::ALL {
        if matches.iter().any(|t| t.category == cat) {
            out.push(Tag {
                label: cat.short(),
                category: Some(cat),
                count: None,
                on: active == Some(cat),
            });
        }
    }
    out
}

/// Narrow `matches` to `active`, or leave it alone.
pub fn only(matches: &[&'static Tool], active: Option<Category>) -> Vec<&'static Tool> {
    match active {
        None => matches.to_vec(),
        Some(cat) => matches
            .iter()
            .filter(|t| t.category == cat)
            .copied()
            .collect(),
    }
}

/// Split `matches` into the favourites group and one group per category.
///
/// `matches` is already filtered by the search box, so a search narrows the
/// groups and empties the ones with no hits — those are dropped rather than drawn
/// as a heading with nothing under it.
///
/// The category groups are ordered **by label**, alphabetically. The design shows
/// only three of the ten and gives no order for the rest, so anything else would
/// be an order I invented and nobody could predict.
pub fn grouped(matches: &[&'static Tool], favorites: &[String]) -> Vec<Group> {
    let mut out = Vec::new();

    // In favourites order, not in `matches` order — the numbering is the point.
    let pinned: Vec<&'static Tool> = favorites
        .iter()
        .filter_map(|id| matches.iter().find(|t| t.id == *id).copied())
        .collect();
    if !pinned.is_empty() {
        out.push(Group {
            label: "Pinned",
            tools: pinned,
            pinned: true,
        });
    }

    let mut cats: Vec<Category> = Vec::new();
    for tool in matches {
        if !cats.contains(&tool.category) {
            cats.push(tool.category);
        }
    }
    cats.sort_by_key(|c| c.label());
    for cat in cats {
        out.push(Group {
            label: cat.label(),
            tools: matches
                .iter()
                .filter(|t| t.category == cat)
                .copied()
                .collect(),
            pinned: false,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<&'static Tool> {
        rp_devtools::search("")
    }

    fn tool(id: &str) -> &'static Tool {
        all()
            .into_iter()
            .find(|t| t.id == id)
            .unwrap_or_else(|| panic!("no tool {id}"))
    }

    #[test]
    fn every_tool_lands_in_exactly_one_group() {
        let tools = all();
        let groups = grouped(&tools, &[]);
        let placed: usize = groups.iter().map(|g| g.tools.len()).sum();
        assert_eq!(placed, tools.len(), "no tool lost, none duplicated");
        // And every group has a real heading rather than a debug name.
        for g in &groups {
            assert!(!g.label.is_empty());
            assert!(
                !g.tools.is_empty(),
                "an empty group is a heading over nothing"
            );
        }
    }

    /// A pinned tool appears **twice** on purpose: once at the top where you go
    /// for it, and once in its category where you would look it up. Removing it
    /// from its category would make the category count wrong.
    #[test]
    fn a_pinned_tool_is_also_still_in_its_category() {
        let tools = all();
        let fav = vec![tool("uuid-generator").id.to_string()];
        let groups = grouped(&tools, &fav);
        assert!(groups[0].pinned, "the favourites group leads");
        assert_eq!(groups[0].label, "Pinned");
        assert_eq!(groups[0].tools.len(), 1);
        let elsewhere = groups[1..]
            .iter()
            .flat_map(|g| &g.tools)
            .filter(|t| t.id == "uuid-generator")
            .count();
        assert_eq!(elsewhere, 1);
    }

    /// The favourites group follows the *stored* order, so the numbering is
    /// stable; the categories do not, so they are predictable.
    #[test]
    fn the_pinned_group_keeps_its_own_order_and_the_categories_are_alphabetical() {
        let tools = all();
        let fav = vec!["uuid-generator".to_string(), "bcrypt".to_string()];
        let groups = grouped(&tools, &fav);
        let ids: Vec<&str> = groups[0].tools.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec!["uuid-generator", "bcrypt"], "not re-sorted");
        let labels: Vec<&str> = groups[1..].iter().map(|g| g.label).collect();
        let mut sorted = labels.clone();
        sorted.sort_unstable();
        assert_eq!(labels, sorted);
    }

    #[test]
    fn a_search_that_matches_nothing_is_no_groups_rather_than_empty_ones() {
        assert!(grouped(&[], &["bcrypt".to_string()]).is_empty());
    }

    /// A favourite naming a tool the search filtered out must not resurrect it.
    #[test]
    fn a_favourite_outside_the_search_results_is_not_shown() {
        let only = vec![tool("bcrypt")];
        let groups = grouped(&only, &["uuid-generator".to_string()]);
        assert!(
            groups.iter().all(|g| !g.pinned),
            "nothing pinned matched, so no pinned heading"
        );
        assert_eq!(groups.len(), 1);
    }

    /// The trap this exists to prevent: a row built from the *filtered* set holds
    /// only the tag you already picked, so there is no way back to the others.
    #[test]
    fn the_filter_row_still_offers_the_others_while_one_is_active() {
        let all_tools = all();
        let with_crypto = tags(&all_tools, Some(Category::Crypto));
        assert!(with_crypto.len() > 2, "not just All and Crypto");
        assert!(
            with_crypto
                .iter()
                .any(|tag| tag.category == Some(Category::Converter)),
            "a different category is still reachable in one click"
        );
        // Exactly one is lit, and "All" leads.
        assert_eq!(with_crypto.iter().filter(|tag| tag.on).count(), 1);
        assert_eq!(with_crypto[0].label, "All");
        assert!(!with_crypto[0].on);
        assert!(tags(&all_tools, None)[0].on, "no filter means All is lit");
    }

    /// "All" counts what the search found, not what the filter left.
    #[test]
    fn all_carries_the_count_and_the_categories_do_not() {
        let all_tools = all();
        let row = tags(&all_tools, Some(Category::Crypto));
        assert_eq!(row[0].count, Some(all_tools.len()));
        assert!(row[1..].iter().all(|tag| tag.count.is_none()));
    }

    #[test]
    fn a_category_with_no_hits_is_not_offered() {
        let one = vec![tool("bcrypt")];
        let row = tags(&one, None);
        assert_eq!(row.len(), 2, "All plus bcrypt's own category");
        assert_eq!(row[1].category, Some(tool("bcrypt").category));
    }

    #[test]
    fn filtering_keeps_only_that_category_and_none_keeps_everything() {
        let all_tools = all();
        assert_eq!(only(&all_tools, None).len(), all_tools.len());
        let crypto = only(&all_tools, Some(Category::Crypto));
        assert!(!crypto.is_empty());
        assert!(crypto.iter().all(|t| t.category == Category::Crypto));
    }

    #[test]
    fn toggling_appends_then_removes_and_leaves_the_rest_alone() {
        let mut list = vec!["a".to_string(), "b".to_string()];
        toggle(&mut list, "c");
        assert_eq!(list, ["a", "b", "c"], "a new one goes last, not first");
        toggle(&mut list, "a");
        assert_eq!(list, ["b", "c"], "and the others keep their order");
    }

    /// Both of these have actually shipped in other apps: a stale id after a
    /// rename, and a duplicate after two windows saved at once.
    #[test]
    fn parsing_drops_unknown_ids_duplicates_and_blank_lines() {
        let known = |id: &str| matches!(id, "bcrypt" | "uuid-generator");
        let got = favorites_from("bcrypt\n\n  gone-tool \nuuid-generator\nbcrypt\n", known);
        assert_eq!(got, ["bcrypt", "uuid-generator"]);
    }

    #[test]
    fn the_stored_form_round_trips() {
        let list = vec!["bcrypt".to_string(), "uuid-generator".to_string()];
        let back = favorites_from(&favorites_to(&list), |_| true);
        assert_eq!(back, list);
    }
}
