//! Persistent tabs: a window holds an ordered list of open tabs plus an active
//! index. One model covers tools and requests, because two parallel code paths
//! for the same bookkeeping is how off-by-one bugs get written twice.
//!
//! The index bookkeeping lives on `Tabs<C>`, generic over the content, so the
//! interesting part (open / activate / close / move out) is testable without a
//! window or an `App`.
use gpui::{
    AnyView, Entity, IntoElement, ParentElement, Pixels, SharedString, Styled, div, prelude::*, px,
};
use rp_store::{TabRecord, WindowSession};
use std::path::{Path, PathBuf};

use crate::request_pane::RequestPane;
use crate::tool_pane::ToolPane;
use crate::{Workspace, theme};

/// Identity of a tab, so clicking a sidebar entry that is already open
/// activates it instead of opening a duplicate.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TabKey {
    Tool(&'static str),
    /// Index into `Workspace::requests`.
    Request(usize),
}

impl TabKey {
    /// Stable element id: gpui needs one per interactive element.
    fn id(&self) -> SharedString {
        match self {
            TabKey::Tool(id) => format!("tab-tool-{id}").into(),
            TabKey::Request(i) => format!("tab-req-{i}").into(),
        }
    }
}

/// What a tab shows. The view is owned by the tab and never rebuilt in
/// `render`, which is what makes half-typed input and running timers survive a
/// tab switch.
pub enum Content {
    Generic(Entity<ToolPane>),
    Custom(AnyView),
    /// A `Kind::Custom` tool this build cannot provide; carries the reason.
    Unavailable(&'static str),
    Request(Entity<RequestPane>),
}

pub struct Tab<C = Content> {
    pub key: TabKey,
    pub title: SharedString,
    pub content: C,
}

pub struct Tabs<C = Content> {
    tabs: Vec<Tab<C>>,
    /// Only meaningful when `tabs` is non-empty; kept `< tabs.len()`.
    active: usize,
}

impl<C> Default for Tabs<C> {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
        }
    }
}

impl<C> Tabs<C> {
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Tab<C>> {
        self.tabs.iter()
    }

    pub fn find(&self, key: &TabKey) -> Option<usize> {
        self.tabs.iter().position(|t| t.key == *key)
    }

    pub fn active_index(&self) -> Option<usize> {
        (!self.tabs.is_empty()).then_some(self.active)
    }

    pub fn active(&self) -> Option<&Tab<C>> {
        self.tabs.get(self.active_index()?)
    }

    pub fn active_key(&self) -> Option<&TabKey> {
        self.active().map(|t| &t.key)
    }

    pub fn activate(&mut self, i: usize) {
        if i < self.tabs.len() {
            self.active = i;
        }
    }

    /// Open `tab`, or activate the existing tab with the same key.
    pub fn open(&mut self, tab: Tab<C>) {
        match self.find(&tab.key) {
            Some(i) => self.active = i,
            None => {
                self.tabs.push(tab);
                self.active = self.tabs.len() - 1;
            }
        }
    }

    /// Remove tab `i` and hand it back, so a caller can move it to another
    /// window. Closing the active tab activates the one that slides into its
    /// place, or the new last tab.
    pub fn remove(&mut self, i: usize) -> Option<Tab<C>> {
        if i >= self.tabs.len() {
            return None;
        }
        let tab = self.tabs.remove(i);
        if self.tabs.is_empty() {
            self.active = 0;
        } else if i < self.active || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1).min(self.tabs.len() - 1);
        }
        Some(tab)
    }

    /// Drop every tab whose key fails `keep`. Used when the request file
    /// changes and the `Request(usize)` indices stop meaning anything.
    pub fn retain(&mut self, keep: impl Fn(&TabKey) -> bool) {
        for i in (0..self.tabs.len()).rev() {
            if !keep(&self.tabs[i].key) {
                self.remove(i);
            }
        }
    }
}

/// The two `TabRecord::kind` values the app writes. The store does not
/// interpret them, so they are ours to define.
const KIND_TOOL: &str = "tool";
const KIND_REQUEST: &str = "request";

/// What restoring a window asks for. Both variants are already validated: an
/// unknown tool id or an out-of-range request index never reaches the caller.
#[derive(Debug, PartialEq, Eq)]
pub enum Restore {
    /// A `Tool::id` that `rp_devtools::find` resolved.
    Tool(&'static str),
    /// `index` is known to be in range for the file at `path`.
    Request { path: PathBuf, index: usize },
}

/// A window's restorable contents, after every unresolvable tab was dropped.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub tabs: Vec<Restore>,
    /// Already clamped into `tabs`.
    pub active: Option<usize>,
    /// The request file that was loaded, if any — a window has exactly one.
    pub source: Option<PathBuf>,
    /// Why some tabs did not come back, for the status line. `None` when
    /// everything that was stored was restored (or skipped silently).
    pub note: Option<String>,
}

/// Turn a window's live tabs into something the store can hold.
///
/// **Only which tabs were open is persisted.** In-progress input inside a tool
/// pane and fetched responses are deliberately not saved: a restored tab is a
/// fresh one. That is the design, not a bug.
///
/// `TabKey::Request(i)` indexes `Workspace::requests`, which means nothing after
/// a restart — the file it came from is what makes it resolvable, so a window
/// without a `source_path` cannot contribute request tabs at all.
pub fn snapshot<C>(tabs: &Tabs<C>, source_path: Option<&Path>) -> WindowSession {
    let mut out = Vec::with_capacity(tabs.len());
    let mut active_tab = None;
    for (i, tab) in tabs.iter().enumerate() {
        let record = match (&tab.key, source_path) {
            (TabKey::Tool(id), _) => TabRecord {
                kind: KIND_TOOL.into(),
                key: (*id).into(),
                detail: None,
            },
            (TabKey::Request(index), Some(path)) => TabRecord {
                kind: KIND_REQUEST.into(),
                key: path.display().to_string(),
                detail: Some(index.to_string()),
            },
            // Nothing to point at: skip rather than write junk.
            (TabKey::Request(_), None) => continue,
        };
        if tabs.active_index() == Some(i) {
            active_tab = Some(out.len());
        }
        out.push(record);
    }
    WindowSession {
        tabs: out,
        active_tab,
    }
}

/// Work out what a stored window can actually reopen.
///
/// `requests_at` loads the request file and reports how many requests it holds,
/// or `None` if it has moved, been deleted or no longer parses. It is called at
/// most once, because a `Workspace` owns exactly one source file — so the first
/// request path in the window wins and records naming another are stale.
///
/// Kept a plain function over plain values so the mapping is testable without a
/// window; that is where this feature breaks, not in the SQL.
pub fn plan(session: &WindowSession, requests_at: impl FnOnce(&Path) -> Option<usize>) -> Plan {
    let mut out = Vec::with_capacity(session.tabs.len());
    let mut active = None;
    let mut source = None;
    let mut loader = Some(requests_at);
    let mut total: Option<usize> = None;
    let mut note = None;

    for (i, record) in session.tabs.iter().enumerate() {
        let restored = match record.kind.as_str() {
            // A tool that was renamed or dropped since: skip, silently.
            KIND_TOOL => rp_devtools::find(&record.key).map(|t| Restore::Tool(t.id)),
            KIND_REQUEST => {
                let path = PathBuf::from(&record.key);
                if let Some(load) = loader.take() {
                    total = load(&path);
                    match total {
                        Some(_) => source = Some(path.clone()),
                        None => note = Some(format!("could not reopen {}", path.display())),
                    }
                }
                // Records for a second file, or an index the file no longer
                // has, cannot be resolved.
                let usable = source.as_deref() == Some(path.as_path());
                record
                    .detail
                    .as_deref()
                    .and_then(|d| d.parse::<usize>().ok())
                    .filter(|index| usable && total.is_some_and(|n| *index < n))
                    .map(|index| Restore::Request { path, index })
            }
            _ => None,
        };
        if let Some(restored) = restored {
            if session.active_tab == Some(i) {
                active = Some(out.len());
            }
            out.push(restored);
        }
    }

    // The store clamps too, but a stored index has to survive tabs that were
    // dropped just now — so clamp against what actually came back.
    let active = active
        .filter(|i| *i < out.len())
        .or((!out.is_empty()).then_some(0));
    Plan {
        tabs: out,
        active,
        source,
        note,
    }
}

/// How wide a tab title may get before it is ellipsised. Not a spacing step, so
/// it is not in `theme::space`; it is a multiple of the 4-px grid.
const TITLE_W: Pixels = px(140.);

/// One tab action — detach, close. Reveals itself only while its own tab is
/// hovered, which is what keeps the strip quiet when nothing is being aimed at.
fn tab_action(
    id: SharedString,
    group: SharedString,
    glyph: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .p(theme::space::XXS)
        .rounded(theme::radius::XS)
        .text_color(theme::TEXT_TERTIARY)
        // gpui has no `invisible()`, so the reveal is an opacity swap driven by
        // the tab's group.
        .opacity(0.)
        .group_hover(group, |s| s.opacity(1.))
        .hover(|s| s.bg(theme::FILL_STRONG).text_color(theme::TEXT))
        .child(theme::icon_sized(glyph, theme::text::SMALL))
        .on_click(on_click)
}

/// The tab strip above the detail pane.
pub fn strip(ws: &Workspace, cx: &mut gpui::Context<Workspace>) -> impl IntoElement + use<> {
    let active = ws.tabs.active_index();
    // Built eagerly with a `for` loop: a lazy `map` closure would let the
    // `&mut Context` borrow escape.
    let mut items = Vec::with_capacity(ws.tabs.len());
    for (i, tab) in ws.tabs.iter().enumerate() {
        let is_active = active == Some(i);
        let id = tab.key.id();
        // One hover group per tab, so hovering a tab reveals only its own
        // actions: `GroupHitboxes` resolves a group name to the innermost
        // element that claimed it, and a shared name would make every tab's
        // close button light up at once.
        let group = SharedString::from(format!("{id}-group"));
        items.push(
            div()
                .id(id.clone())
                .group(group.clone())
                .flex()
                .flex_none()
                .gap(theme::space::XXS)
                .items_center()
                .h_full()
                .px(theme::space::SM)
                .text_size(theme::text::SMALL)
                .line_height(theme::text::SMALL_LINE)
                // The accent edge marks the active tab; the others carry the
                // same border in the strip's colour so nothing shifts.
                .border_t_2()
                .border_color(if is_active {
                    theme::ACCENT
                } else {
                    theme::SURFACE
                })
                .map(|d| {
                    if is_active {
                        d.bg(theme::RAISED).text_color(theme::TEXT)
                    } else {
                        d.text_color(theme::TEXT_TERTIARY)
                            .hover(|s| s.bg(theme::FILL).text_color(theme::TEXT_SECONDARY))
                    }
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tabs.activate(i);
                    cx.defer(crate::save_session);
                    cx.notify();
                }))
                .child(
                    div()
                        .max_w(TITLE_W)
                        .truncate()
                        .child(tab.title.clone()),
                )
                // Pull the tab out onto another monitor.
                .child(tab_action(
                    SharedString::from(format!("{id}-detach")),
                    group.clone(),
                    "window",
                    cx.listener(move |this, _, _, cx| this.detach_tab(i, cx)),
                ))
                .child(tab_action(
                    SharedString::from(format!("{id}-close")),
                    group,
                    "close",
                    cx.listener(move |this, _, _, cx| {
                        this.tabs.remove(i);
                        cx.defer(crate::save_session);
                        cx.notify();
                    }),
                )),
        );
    }
    div()
        .id("tab-strip")
        .flex()
        .flex_none()
        .h(theme::control::MD)
        .w_full()
        .overflow_x_scroll()
        .bg(theme::SURFACE)
        .border_b_1()
        .border_color(theme::BORDER)
        .children(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Content is irrelevant to the bookkeeping, so tests use a `&str`.
    fn tab(key: TabKey, title: &'static str) -> Tab<&'static str> {
        Tab {
            key,
            title: title.into(),
            content: title,
        }
    }

    fn tools(ids: &[&'static str]) -> Tabs<&'static str> {
        let mut t = Tabs::default();
        for id in ids {
            t.open(tab(TabKey::Tool(id), id));
        }
        t
    }

    fn keys(t: &Tabs<&'static str>) -> Vec<TabKey> {
        t.iter().map(|t| t.key.clone()).collect()
    }

    #[test]
    fn empty_has_no_active_tab() {
        let t: Tabs<&str> = Tabs::default();
        assert!(t.is_empty());
        assert_eq!(t.active_index(), None);
        assert!(t.active().is_none());
    }

    #[test]
    fn open_appends_and_activates() {
        let t = tools(&["a", "b"]);
        assert_eq!(t.len(), 2);
        assert_eq!(t.active_key(), Some(&TabKey::Tool("b")));
    }

    #[test]
    fn reopening_activates_instead_of_duplicating() {
        let mut t = tools(&["a", "b", "c"]);
        t.open(tab(TabKey::Tool("a"), "a again"));
        assert_eq!(t.len(), 3);
        assert_eq!(t.active_index(), Some(0));
        // The existing tab is kept, not replaced.
        assert_eq!(t.active().map(|x| x.content), Some("a"));
    }

    #[test]
    fn tools_and_requests_share_one_list() {
        let mut t = tools(&["a"]);
        t.open(tab(TabKey::Request(0), "req"));
        t.open(tab(TabKey::Request(0), "req"));
        assert_eq!(t.len(), 2);
        assert_eq!(t.active_key(), Some(&TabKey::Request(0)));
    }

    #[test]
    fn closing_active_activates_the_next_one() {
        let mut t = tools(&["a", "b", "c"]);
        t.activate(1);
        t.remove(1);
        assert_eq!(keys(&t), vec![TabKey::Tool("a"), TabKey::Tool("c")]);
        assert_eq!(t.active_key(), Some(&TabKey::Tool("c")));
    }

    #[test]
    fn closing_the_active_last_tab_falls_back_left() {
        let mut t = tools(&["a", "b", "c"]);
        assert_eq!(t.active_index(), Some(2));
        t.remove(2);
        assert_eq!(t.active_key(), Some(&TabKey::Tool("b")));
    }

    #[test]
    fn closing_before_the_active_one_keeps_it_active() {
        let mut t = tools(&["a", "b", "c"]);
        t.activate(2);
        t.remove(0);
        assert_eq!(t.active_key(), Some(&TabKey::Tool("c")));
        assert_eq!(t.active_index(), Some(1));
    }

    #[test]
    fn closing_after_the_active_one_keeps_it_active() {
        let mut t = tools(&["a", "b", "c"]);
        t.activate(0);
        t.remove(2);
        assert_eq!(t.active_index(), Some(0));
    }

    #[test]
    fn closing_down_to_empty_is_not_a_panic() {
        let mut t = tools(&["a", "b"]);
        assert!(t.remove(0).is_some());
        assert!(t.remove(0).is_some());
        assert!(t.is_empty());
        assert_eq!(t.active_index(), None);
        // Out-of-range operations are inert rather than fatal.
        assert!(t.remove(0).is_none());
        t.activate(3);
        assert_eq!(t.active_index(), None);
    }

    #[test]
    fn remove_hands_the_tab_over_for_another_window() {
        let mut from = tools(&["a", "b"]);
        let moved = from.remove(1).expect("tab");
        let mut to: Tabs<&str> = Tabs::default();
        to.open(moved);
        assert_eq!(keys(&from), vec![TabKey::Tool("a")]);
        assert_eq!(to.active_key(), Some(&TabKey::Tool("b")));
        // It really left the source window.
        assert!(from.find(&TabKey::Tool("b")).is_none());
    }

    // --- session snapshot / restore mapping -------------------------------
    //
    // These use real tool ids, because resolving them is half the point.
    const TOOL: &str = "chronometer";

    fn record(kind: &str, key: &str, detail: Option<&str>) -> TabRecord {
        TabRecord {
            kind: kind.into(),
            key: key.into(),
            detail: detail.map(Into::into),
        }
    }

    /// A window with one `.http` file holding `n` requests.
    fn http_file(tag: &str, n: usize) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rp-tabs-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("api.http");
        let body: String = (0..n)
            .map(|i| format!("### r{i}\nGET https://example.com/{i}\n\n"))
            .collect();
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn a_tool_tab_round_trips() {
        let mut t: Tabs<&str> = Tabs::default();
        t.open(tab(TabKey::Tool(TOOL), TOOL));
        let stored = snapshot(&t, None);
        assert_eq!(stored.tabs, [record("tool", TOOL, None)]);
        assert_eq!(stored.active_tab, Some(0));

        let plan = plan(&stored, |_| unreachable!("no request tabs to load"));
        assert_eq!(plan.tabs, [Restore::Tool(TOOL)]);
        assert_eq!(plan.active, Some(0));
        assert_eq!(plan.source, None);
        assert_eq!(plan.note, None);
    }

    #[test]
    fn a_request_tab_round_trips_with_its_source_path_and_index() {
        let path = http_file("roundtrip", 3);
        let mut t: Tabs<&str> = Tabs::default();
        t.open(tab(TabKey::Tool(TOOL), TOOL));
        t.open(tab(TabKey::Request(2), "r2"));
        let stored = snapshot(&t, Some(&path));
        assert_eq!(
            stored.tabs,
            [
                record("tool", TOOL, None),
                record("request", &path.display().to_string(), Some("2")),
            ]
        );
        assert_eq!(stored.active_tab, Some(1));

        let mut loads = 0;
        let plan = plan(&stored, |p| {
            loads += 1;
            crate::session::load(p).ok().map(|l| l.requests.len())
        });
        assert_eq!(loads, 1, "the file must be loaded once");
        assert_eq!(
            plan.tabs,
            [
                Restore::Tool(TOOL),
                Restore::Request {
                    path: path.clone(),
                    index: 2
                },
            ]
        );
        assert_eq!(plan.active, Some(1));
        assert_eq!(plan.source.as_deref(), Some(path.as_path()));
        assert_eq!(plan.note, None);
    }

    #[test]
    fn a_window_with_no_source_path_contributes_no_request_tabs() {
        let mut t: Tabs<&str> = Tabs::default();
        t.open(tab(TabKey::Request(0), "r0"));
        t.open(tab(TabKey::Tool(TOOL), TOOL));
        let stored = snapshot(&t, None);
        assert_eq!(stored.tabs, [record("tool", TOOL, None)]);
        // The active tab was the tool, and it stayed the active one.
        assert_eq!(stored.active_tab, Some(0));
    }

    #[test]
    fn a_tool_id_that_no_longer_exists_is_skipped_silently() {
        let stored = WindowSession {
            tabs: vec![
                record("tool", "tool-that-was-renamed", None),
                record("tool", TOOL, None),
                // An unknown kind is stale data too, not a crash.
                record("sandwich", "blt", None),
            ],
            active_tab: Some(1),
        };
        let plan = plan(&stored, |_| unreachable!());
        assert_eq!(plan.tabs, [Restore::Tool(TOOL)]);
        assert_eq!(plan.active, Some(0));
        assert_eq!(plan.note, None, "a dropped tool is not worth a message");
    }

    #[test]
    fn a_request_whose_file_is_gone_is_skipped_with_a_note() {
        let stored = WindowSession {
            tabs: vec![
                record("request", "definitely/not/here.http", Some("0")),
                record("tool", TOOL, None),
            ],
            active_tab: Some(0),
        };
        let plan = plan(&stored, |p| {
            crate::session::load(p).ok().map(|l| l.requests.len())
        });
        assert_eq!(plan.tabs, [Restore::Tool(TOOL)]);
        // The active tab is the one that failed, so it falls back rather than
        // leaving the window blank.
        assert_eq!(plan.active, Some(0));
        assert_eq!(plan.source, None);
        let note = plan.note.expect("a lost file must be visible, not silent");
        assert!(note.contains("here.http"), "{note}");
    }

    #[test]
    fn a_stored_index_past_the_end_of_the_file_is_skipped() {
        let path = http_file("shrunk", 1);
        let stored = WindowSession {
            tabs: vec![
                record("request", &path.display().to_string(), Some("0")),
                record("request", &path.display().to_string(), Some("7")),
                // Not a number at all.
                record("request", &path.display().to_string(), Some("x")),
                record("request", &path.display().to_string(), None),
            ],
            active_tab: Some(1),
        };
        let plan = plan(&stored, |p| {
            crate::session::load(p).ok().map(|l| l.requests.len())
        });
        assert_eq!(plan.tabs, [Restore::Request { path, index: 0 }]);
        assert_eq!(plan.active, Some(0), "a stale active index must be clamped");
    }

    #[test]
    fn an_out_of_range_active_index_never_escapes_the_plan() {
        let stored = WindowSession {
            tabs: vec![record("tool", TOOL, None)],
            active_tab: Some(9),
        };
        let plan = plan(&stored, |_| unreachable!());
        assert_eq!(plan.active, Some(0));

        // And nothing restorable means nothing active.
        let empty = WindowSession {
            tabs: vec![record("tool", "gone", None)],
            active_tab: Some(0),
        };
        assert_eq!(super::plan(&empty, |_| unreachable!()).active, None);
    }

    #[test]
    fn request_records_for_a_second_file_are_stale_and_dropped() {
        // A window owns one source file, so only the first path is loaded.
        let first = http_file("two-a", 2);
        let second = http_file("two-b", 2);
        let stored = WindowSession {
            tabs: vec![
                record("request", &first.display().to_string(), Some("0")),
                record("request", &second.display().to_string(), Some("1")),
            ],
            active_tab: Some(1),
        };
        let mut loads = 0;
        let plan = plan(&stored, |p| {
            loads += 1;
            crate::session::load(p).ok().map(|l| l.requests.len())
        });
        assert_eq!(loads, 1);
        assert_eq!(
            plan.tabs,
            [Restore::Request {
                path: first,
                index: 0
            }]
        );
        assert_eq!(plan.active, Some(0));
    }

    #[test]
    fn retain_drops_request_tabs_and_fixes_the_active_index() {
        let mut t: Tabs<&str> = Tabs::default();
        t.open(tab(TabKey::Request(0), "r0"));
        t.open(tab(TabKey::Tool("a"), "a"));
        t.open(tab(TabKey::Request(1), "r1"));
        t.retain(|k| !matches!(k, TabKey::Request(_)));
        assert_eq!(keys(&t), vec![TabKey::Tool("a")]);
        assert_eq!(t.active_index(), Some(0));
    }
}
