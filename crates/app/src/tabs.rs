//! Persistent tabs: a window holds an ordered list of open tabs plus an active
//! index. One model covers tools and requests, because two parallel code paths
//! for the same bookkeeping is how off-by-one bugs get written twice.
//!
//! A window holds **one such list per section**, not one overall. Requests and
//! Tools each have their own strip, so switching the rail switches the strip;
//! a tool tab sitting in the Requests strip was only ever noise. `Workspace`
//! owns the two lists and `Workspace::tabs`/`tabs_mut` hand out the one the
//! current section shows — this module stays generic and does not know about
//! sections at all.
//!
//! The index bookkeeping lives on `Tabs<C>`, generic over the content, so the
//! interesting part (open / activate / close / move out) is testable without a
//! window or an `App`.
use crate::log::note;
use gpui::{
    AnyElement, AnyView, Entity, IntoElement, MouseDownEvent, ParentElement, SharedString, Styled,
    div, prelude::*,
};
use rp_store::{TabRecord, WindowSession};
use rp_ui::layout::divider;
use rp_ui::theme;
use rp_ui::tokens::{control, radius, space, text};
use rp_ui::{TabItem, TabMenuRequest, TabOverflowRequest, TabStrip};
use std::path::{Path, PathBuf};

use crate::Workspace;
use crate::request_pane::RequestPane;
use crate::tool_pane::ToolPane;

/// Identity of a tab, so clicking a sidebar entry that is already open
/// activates it instead of opening a duplicate.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TabKey {
    Tool(&'static str),
    /// A request in one open file: which file, and its position in that file.
    ///
    /// The path travels with the key rather than an index into a list of open
    /// sources, because several files are open at once and closing one would
    /// renumber every tab behind it. It is also exactly what has to be persisted,
    /// so a saved tab needs no lookup table to come back.
    Request {
        source: PathBuf,
        index: usize,
    },
    /// The environment and file variables, and what earlier responses captured.
    ///
    /// A tab and not a sidebar panel: it used to sit under the request list and
    /// take half the rail's height, which left the list of requests about nine
    /// rows tall. It is also workspace-wide rather than per request, which is why
    /// it is a tab of the Requests section and not one of the request pane's own.
    Vars,
}

impl TabKey {
    /// Stable element id: gpui needs one per interactive element.
    fn id(&self) -> SharedString {
        match self {
            TabKey::Tool(id) => format!("tab-tool-{id}").into(),
            TabKey::Request { source, index } => {
                format!("tab-req-{}-{index}", source.display()).into()
            }
            TabKey::Vars => "tab-vars".into(),
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
    /// Drawn from `Workspace` state, so it carries nothing of its own.
    Vars,
}

pub struct Tab<C = Content> {
    pub key: TabKey,
    pub title: SharedString,
    pub content: C,
    /// Pinned tabs sort ahead of the rest and survive the bulk closes. A
    /// property of the *tab*, not of the window, which is why it travels with a
    /// tab that is detached into a new one.
    pub pinned: bool,
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

    /// The key of tab `i`, for a caller that has an index and needs the identity.
    pub fn key_at(&self, i: usize) -> Option<&TabKey> {
        self.tabs.get(i).map(|t| &t.key)
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

    /// Drop every tab whose key fails `keep`. Used when a source is closed and
    /// its `Request { source, .. }` keys stop pointing at anything open.
    pub fn retain(&mut self, keep: impl Fn(&TabKey) -> bool) {
        for i in (0..self.tabs.len()).rev() {
            if !keep(&self.tabs[i].key) {
                self.remove(i);
            }
        }
    }

    // -- pinning -----------------------------------------------------------

    pub fn is_pinned(&self, i: usize) -> bool {
        self.tabs.get(i).is_some_and(|t| t.pinned)
    }

    pub fn pinned_count(&self) -> usize {
        self.tabs.iter().filter(|t| t.pinned).count()
    }

    /// Pin or unpin tab `i`, then restore the pinned-first order.
    ///
    /// The active tab is tracked by **key, not index**: the reorder moves tabs
    /// around it, and following the old index would silently activate a
    /// different tab. That is the whole risk in this function.
    pub fn set_pinned(&mut self, i: usize, pinned: bool) {
        let Some(tab) = self.tabs.get_mut(i) else {
            return;
        };
        tab.pinned = pinned;
        let active_key = self.tabs.get(self.active).map(|t| t.key.clone());
        // `sort_by_key` is stable, so both runs keep the order they had: a tab
        // just pinned lands at the end of the pinned run and one just unpinned at
        // the start of the unpinned run, which is where IntelliJ leaves them.
        self.tabs.sort_by_key(|t| !t.pinned);
        if let Some(at) = active_key.and_then(|k| self.find(&k)) {
            self.active = at;
        }
    }

    pub fn toggle_pin(&mut self, i: usize) {
        self.set_pinned(i, !self.is_pinned(i));
    }

    // -- the close family behind the tab menu ------------------------------
    //
    // Every one of these goes through `remove`, back to front, so the active
    // index is fixed up by the one piece of code that knows how — which is
    // where the off-by-one lives if it lives anywhere. Iterating downwards also
    // means a removal never shifts an index still to be visited.
    //
    // Every bulk close **skips pinned tabs**; only `close_all` and closing one
    // named tab can take a pinned one away. Otherwise a pin could never be
    // undone except one tab at a time — see the note on `close_all`.

    /// Whether any of `range` is closable, i.e. exists and is not pinned. What
    /// tells "this would do nothing" apart from "there is nothing there".
    fn any_closable(&self, range: impl IntoIterator<Item = usize>) -> bool {
        range
            .into_iter()
            .any(|j| self.tabs.get(j).is_some_and(|t| !t.pinned))
    }

    /// Close every unpinned tab before `i`.
    pub fn close_left_of(&mut self, i: usize) {
        for j in (0..i.min(self.tabs.len())).rev() {
            if !self.tabs[j].pinned {
                self.remove(j);
            }
        }
    }

    /// Close every unpinned tab after `i`.
    pub fn close_right_of(&mut self, i: usize) {
        for j in ((i + 1)..self.tabs.len()).rev() {
            if !self.tabs[j].pinned {
                self.remove(j);
            }
        }
    }

    /// Close every unpinned tab but `i`, which is left active. Inert when `i` is
    /// not a tab, rather than emptying the window.
    pub fn close_others(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.close_right_of(i);
        self.close_left_of(i);
    }

    /// Keep only the pinned tabs.
    pub fn close_all_but_pinned(&mut self) {
        for j in (0..self.tabs.len()).rev() {
            if !self.tabs[j].pinned {
                self.remove(j);
            }
        }
    }

    /// Close everything, pinned included.
    ///
    /// The one bulk close that ignores pins, deliberately: if it respected them
    /// it would *be* [`Self::close_all_but_pinned`], the label would be a lie,
    /// and there would be no way to clear a window without unpinning tab by tab.
    /// IntelliJ splits the two the same way.
    pub fn close_all(&mut self) {
        self.tabs.clear();
        self.active = 0;
    }
}

/// The two `TabRecord::kind` values the app writes. The store does not
/// interpret them, so they are ours to define.
const KIND_TOOL: &str = "tool";
const KIND_REQUEST: &str = "request";
const KIND_VARS: &str = "vars";

/// What restoring a window asks for. Both variants are already validated: an
/// unknown tool id or an out-of-range request index never reaches the caller.
///
/// `pinned` and `active` ride on the variant rather than in a second `Vec<bool>`
/// beside `Plan::tabs`: two parallel lists that must stay in step is how the
/// index bugs this module keeps testing for get written. `active` is a flag and
/// not an index for the same reason it is one in the store — each section has
/// its own active tab, and one index into a flat list cannot say both.
#[derive(Debug, PartialEq, Eq)]
pub enum Restore {
    /// A `Tool::id` that `rp_devtools::find` resolved.
    Tool {
        id: &'static str,
        pinned: bool,
        active: bool,
    },
    /// `index` is known to be in range for the file at `path`.
    Request {
        path: PathBuf,
        index: usize,
        pinned: bool,
        active: bool,
    },
    Vars {
        pinned: bool,
        active: bool,
    },
}

impl Restore {
    pub fn pinned(&self) -> bool {
        match self {
            Restore::Tool { pinned, .. }
            | Restore::Request { pinned, .. }
            | Restore::Vars { pinned, .. } => *pinned,
        }
    }

    pub fn active(&self) -> bool {
        match self {
            Restore::Tool { active, .. }
            | Restore::Request { active, .. }
            | Restore::Vars { active, .. } => *active,
        }
    }

    /// Which section's list this tab belongs in — the one fact the caller needs
    /// that the variant already knows.
    fn is_tool(&self) -> bool {
        matches!(self, Restore::Tool { .. })
    }

    fn activate(&mut self) {
        match self {
            Restore::Tool { active, .. }
            | Restore::Request { active, .. }
            | Restore::Vars { active, .. } => *active = true,
        }
    }
}

/// A window's restorable contents, after every unresolvable tab was dropped.
///
/// One flat list, with each entry saying which section it belongs to; the caller
/// opens each tab into that section's list, in this order. Which tab is active
/// is [`Restore::active`], at most one per section.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub tabs: Vec<Restore>,
    /// Every request file that loaded, in the order the tabs first named it. The
    /// caller opens these as its sources; a file that would not load is absent and
    /// accounted for in `note`.
    pub sources: Vec<PathBuf>,
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
/// Both of the window's lists go into the one flat record list, requests first;
/// each marks its own active tab, so restoring gives each section back the tab
/// it was on rather than leaving one of them at the front.
///
/// Every request tab already carries its own file, so there is nothing to look up
/// and no request tab that cannot be written — which was the case while a window
/// had exactly one source.
pub fn snapshot<C>(
    request_tabs: &Tabs<C>,
    tool_tabs: &Tabs<C>,
    sources: &[std::path::PathBuf],
) -> WindowSession {
    let mut out = Vec::with_capacity(request_tabs.len() + tool_tabs.len());
    for tabs in [request_tabs, tool_tabs] {
        for (i, tab) in tabs.iter().enumerate() {
            let active = tabs.active_index() == Some(i);
            out.push(match &tab.key {
                TabKey::Tool(id) => TabRecord {
                    kind: KIND_TOOL.into(),
                    key: (*id).into(),
                    detail: None,
                    pinned: tab.pinned,
                    active,
                },
                TabKey::Vars => TabRecord {
                    kind: KIND_VARS.into(),
                    key: String::new(),
                    detail: None,
                    pinned: tab.pinned,
                    active,
                },
                TabKey::Request { source, index } => TabRecord {
                    kind: KIND_REQUEST.into(),
                    key: source.display().to_string(),
                    detail: Some(index.to_string()),
                    pinned: tab.pinned,
                    active,
                },
            });
        }
    }
    WindowSession {
        // The open files in their own right: one with no tab open in it is still
        // open, and inferring the list from the tabs would lose it.
        sources: sources.iter().map(|p| p.display().to_string()).collect(),
        // Superseded by the per-tab flag; still written so a *downgrade* to a
        // build before v4 reopens something rather than an empty window.
        active_tab: out.iter().position(|r| r.active),
        tabs: out,
    }
}

/// Work out what a stored window can actually reopen.
///
/// `requests_at` loads a request file and reports how many requests it holds, or
/// `None` if it has moved, been deleted or no longer parses. Called **once per
/// distinct path**: a window holds several files now, and each is loaded exactly
/// once no matter how many of its requests were open.
///
/// Kept a plain function over plain values so the mapping is testable without a
/// window; that is where this feature breaks, not in the SQL.
pub fn plan(session: &WindowSession, mut requests_at: impl FnMut(&Path) -> Option<usize>) -> Plan {
    let mut out = Vec::with_capacity(session.tabs.len());
    let mut note: Option<String> = None;
    // The stored file list comes first and in its own order, so reopening puts the
    // sidebar back the way it was rather than in the order the tabs happen to be.
    let mut stored: Vec<PathBuf> = Vec::new();
    // path -> how many requests it holds, or `None` for one that would not load.
    // Doubles as the "already tried" set, so a file is read once however many of
    // its requests were open.
    let mut loaded: Vec<(PathBuf, Option<usize>)> = Vec::new();
    for path in &session.sources {
        let path = PathBuf::from(path);
        if loaded.iter().any(|(p, _)| *p == path) {
            continue;
        }
        let total = requests_at(&path);
        if total.is_some() {
            stored.push(path.clone());
        } else {
            note.get_or_insert_with(|| format!("could not reopen {}", path.display()));
        }
        loaded.push((path, total));
    }
    // Databases written before migration v4 have no per-tab flag, only the one
    // index per window. Fall back to it so an older session still reopens on the
    // tab it was on; `active` on every record is how we tell the two apart.
    let legacy_active = (!session.tabs.iter().any(|r| r.active))
        .then_some(session.active_tab)
        .flatten();

    for (i, record) in session.tabs.iter().enumerate() {
        let active = record.active || legacy_active == Some(i);
        let restored = match record.kind.as_str() {
            // A tool that was renamed or dropped since: skip, silently.
            KIND_TOOL => rp_devtools::find(&record.key).map(|t| Restore::Tool {
                id: t.id,
                pinned: record.pinned,
                active,
            }),
            KIND_VARS => Some(Restore::Vars {
                pinned: record.pinned,
                active,
            }),
            KIND_REQUEST => {
                let path = PathBuf::from(&record.key);
                let total = match loaded.iter().find(|(p, _)| *p == path) {
                    Some((_, total)) => *total,
                    None => {
                        let total = requests_at(&path);
                        if total.is_none() {
                            // One note per lost file, and the first one wins:
                            // three missing files are one problem to go and look
                            // at, not three lines in a status bar.
                            note.get_or_insert_with(|| {
                                format!("could not reopen {}", path.display())
                            });
                        }
                        loaded.push((path.clone(), total));
                        total
                    }
                };
                record
                    .detail
                    .as_deref()
                    .and_then(|d| d.parse::<usize>().ok())
                    .filter(|index| total.is_some_and(|n| *index < n))
                    .map(|index| Restore::Request {
                        path,
                        index,
                        pinned: record.pinned,
                        active,
                    })
            }
            _ => None,
        };
        if let Some(restored) = restored {
            out.push(restored);
        }
    }

    // The tab that was active may be one of the ones just dropped, which would
    // leave its section with no active tab at all. Fall back to the first tab of
    // that section, per section — the old code clamped a single index for the
    // same reason.
    for tools in [false, true] {
        let of_kind = |r: &Restore| r.is_tool() == tools;
        if let Some(first) = out.iter().position(of_kind)
            && !out.iter().any(|r| of_kind(r) && r.active())
        {
            out[first].activate();
        }
    }
    Plan {
        tabs: out,
        sources: {
            // Anything a tab named that was not in the stored list is appended: a
            // database written before v5 has no list at all, and its tabs are then
            // the only record of which files were open.
            let mut all = stored;
            for (path, total) in loaded {
                if total.is_some() && !all.contains(&path) {
                    all.push(path);
                }
            }
            all
        },
        note,
    }
}

/// Which tab carries this element key.
///
/// [`rp_ui::TabStrip`] hands back the key it was given rather than an index, so
/// the lookup belongs here — and a click that lands after the list changed
/// underneath resolves to nothing rather than to whichever tab now sits at that
/// position.
fn index_of(ws: &Workspace, key: &SharedString) -> Option<usize> {
    ws.tabs().iter().position(|t| t.key.id() == *key)
}

/// The tab strip above the detail pane.
///
/// The drawing is [`rp_ui::TabStrip`]'s; what stays here is the part that knows
/// about `TabKey` — turning tabs into items, and turning a key back into the index
/// every operation on [`Tabs`] takes.
pub fn strip(ws: &Workspace, cx: &mut gpui::Context<Workspace>) -> impl IntoElement + use<> {
    let t = theme::theme(cx);
    let tabs = ws.tabs();
    let active = tabs.active_index();
    let items = tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| TabItem {
            key: tab.key.id(),
            title: tab.title.clone(),
            pinned: tab.pinned,
            active: active == Some(i),
        })
        .collect();
    TabStrip::new("tab-strip", items)
        // Measured last frame by the strip's own `canvas`; without it the strip
        // cannot know how many tabs fit and draws them all.
        .width(ws.tab_strip_width)
        .on_measure(cx.listener(|this, width: &gpui::Pixels, _, cx| {
            this.tab_strip_width = *width;
            cx.notify();
        }))
        .on_activate(cx.listener(|this, key: &SharedString, _, cx| {
            if let Some(i) = index_of(this, key) {
                this.tabs_mut().activate(i);
                cx.defer(crate::save_session);
                cx.notify();
            }
        }))
        .on_close(cx.listener(|this, key: &SharedString, _, cx| {
            if let Some(i) = index_of(this, key) {
                this.tabs_mut().remove(i);
                cx.defer(crate::save_session);
                cx.notify();
            }
        }))
        .on_toggle_pin(cx.listener(|this, key: &SharedString, _, cx| {
            if let Some(i) = index_of(this, key) {
                this.tabs_mut().toggle_pin(i);
                cx.defer(crate::save_session);
                cx.notify();
            }
        }))
        // Pull the tab out onto another monitor.
        .on_detach(cx.listener(|this, key: &SharedString, _, cx| {
            if let Some(i) = index_of(this, key) {
                this.detach_tab(i, cx);
            }
        }))
        // Right-click selects the tab and opens the menu at the pointer, as
        // IntelliJ does.
        .on_menu(cx.listener(|this, req: &TabMenuRequest, window, cx| {
            if let Some(i) = index_of(this, &req.key) {
                this.tabs_mut().activate(i);
                this.tab_menu = Some((i, req.at));
                // Escape only reaches the menu if the menu holds focus.
                window.focus(&this.menu_focus, cx);
                cx.defer(crate::save_session);
                cx.notify();
            }
        }))
        // The chevron on the `+6` chip: the tabs that did not fit, as a list.
        .on_overflow(cx.listener(|this, req: &TabOverflowRequest, window, cx| {
            this.tab_overflow = Some((req.hidden.clone(), req.at));
            window.focus(&this.menu_focus, cx);
            cx.notify();
        }))
        // The `+` at the end of the strip. It opens the command palette rather
        // than inventing a blank tab: the design gives the button no behaviour,
        // and "jump to anything" is already how this app opens a tab. Which tab
        // a `+` should conjure out of nothing is not a question the strip can
        // answer for both sections.
        .on_new_tab(cx.listener(|this, _, window, cx| this.toggle_palette(window, cx)))
        .render(t)
}

/// The list behind the overflow chip: the tabs the strip could not fit.
///
/// Drawn at the root of the window for the same reason the context menu is — the
/// tab run clips its own children. Rows are resolved from keys per frame, so one
/// that was closed while the list was open is simply absent.
///
/// Activating a row is enough to bring it on screen: `rp_ui::tabs::visible_range`
/// shifts the drawn window onto the active tab.
pub fn overflow(
    ws: &Workspace,
    cx: &mut gpui::Context<Workspace>,
) -> Option<impl IntoElement + use<>> {
    let (hidden, at) = ws.tab_overflow.as_ref()?;
    let t = theme::theme(cx);
    let mut children = Vec::new();
    for key in hidden {
        let Some(title) = index_of(ws, key)
            .and_then(|i| ws.tabs().iter().nth(i))
            .map(|tab| tab.title.clone())
        else {
            continue;
        };
        let key = key.clone();
        children.push(
            div()
                .id(key.clone())
                .flex()
                .flex_none()
                .items_center()
                .h(control::MD)
                .px(space::S3)
                .rounded(radius::SM)
                .overflow_hidden()
                .child(div().min_w_0().overflow_hidden().truncate().child(title))
                .hover(|s| s.bg(t.hover()))
                // Resolved again on click, not held as an index: the list may have
                // been open while a tab was closed.
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(i) = index_of(this, &key) {
                        this.tabs_mut().activate(i);
                    }
                    this.tab_overflow = None;
                    cx.defer(crate::save_session);
                    cx.notify();
                })),
        );
    }
    if children.is_empty() {
        return None;
    }
    Some(
        gpui::deferred(
            gpui::anchored()
                .position(*at)
                .snap_to_window_with_margin(space::S2)
                .child(
                    div()
                        .id("tab-overflow-list")
                        .track_focus(&ws.menu_focus)
                        .occlude()
                        .flex()
                        .flex_col()
                        .w(popup::WIDTH)
                        .p(space::S1)
                        .rounded(radius::MD)
                        .bg(t.surface)
                        .border_1()
                        .border_color(t.divider)
                        .shadow_md()
                        .text_size(text::BODY)
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.tab_overflow = None;
                            cx.notify();
                        }))
                        .on_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, _, cx| {
                            if ev.keystroke.key == "escape" {
                                this.tab_overflow = None;
                                cx.notify();
                            }
                        }))
                        .children(children),
                ),
        )
        .with_priority(1),
    )
}

// ---------------------------------------------------------------------------
// The tab context menu, in the spirit of IntelliJ's editor-tab menu.
// ---------------------------------------------------------------------------
//
// gpui 0.2 has no menu widget — `App::set_menus` builds the *application* menu
// bar — so this is an `anchored()` surface inside a `deferred()`, which is how
// gpui's own popovers are positioned.
//
// What the menu deliberately does **not** carry, and why:
//
// * *Split and Move Right/Down*, *Move to Opposite Group* — there is no split
//   view to move into, and inventing one is a different feature.
// * *Local History*, *Compare With…* — no VCS and no diff view in this app.
// * Keyboard-shortcut hints — nothing in this app binds a tab action to a key,
//   so every row's hint would be blank.

/// Menu geometry `theme.rs` does not carry yet. Local for the same reason
/// `main.rs` keeps its `shell` module: `theme.rs` has another owner.
mod popup {
    use gpui::{Pixels, px};
    /// Wide enough for "Close Tabs to the Right" at `text::BODY` without wrapping.
    pub const WIDTH: Pixels = px(246.);
}

/// What a menu row does when it is clicked.
///
/// An enum rather than a boxed closure per row, so [`menu_rows`] returns plain
/// data a test can read and the behaviour has exactly one implementation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuAction {
    Close,
    CloseOthers,
    CloseAll,
    CloseAllButPinned,
    CloseLeft,
    CloseRight,
    /// Pin or unpin — one action, because the row's label already says which way
    /// it will go.
    TogglePin,
    /// Reuses `Workspace::detach_tab` — the same path as the tab's own icon.
    Detach,
    CopyPath,
    Reveal,
    Settings,
}

#[derive(PartialEq, Eq, Debug)]
pub struct MenuRow {
    pub label: &'static str,
    pub action: MenuAction,
    /// A row that is meaningless *right here* is greyed; one that never applies
    /// to this kind of tab is absent. A row that silently does nothing is the
    /// thing to avoid.
    pub enabled: bool,
}

/// The menu for tab `i`, in groups — one divider is drawn between groups.
///
/// Pure over the tab list plus the one fact about the window that matters, so
/// every enable/disable rule is testable without a window. Empty when `i` is not
/// a tab, which is also how a menu left open over a list that changed underneath
/// it disappears instead of acting on the wrong tab.
pub fn menu_rows<C>(tabs: &Tabs<C>, i: usize) -> Vec<Vec<MenuRow>> {
    let n = tabs.len();
    let Some(tab) = tabs.tabs.get(i) else {
        return Vec::new();
    };
    let row = |label, action, enabled| MenuRow {
        label,
        action,
        enabled,
    };
    let pinned = tabs.pinned_count();
    let mut groups = vec![
        vec![
            // Closing the tab you named closes it, pinned or not: an explicit
            // single target is not a bulk close.
            row("Close", MenuAction::Close, true),
            // The bulk closes skip pinned tabs, so "would this do anything?" is
            // "is there an unpinned tab there?", not "is there a tab there?".
            row(
                "Close Others",
                MenuAction::CloseOthers,
                tabs.any_closable(0..i) || tabs.any_closable(i + 1..n),
            ),
            row("Close All", MenuAction::CloseAll, true),
            row(
                "Close All but Pinned",
                MenuAction::CloseAllButPinned,
                pinned > 0 && pinned < n,
            ),
        ],
        vec![
            row(
                "Close Tabs to the Left",
                MenuAction::CloseLeft,
                tabs.any_closable(0..i),
            ),
            row(
                "Close Tabs to the Right",
                MenuAction::CloseRight,
                tabs.any_closable(i + 1..n),
            ),
        ],
        vec![
            row(
                if tab.pinned { "Unpin Tab" } else { "Pin Tab" },
                MenuAction::TogglePin,
                true,
            ),
            row("Move to New Window", MenuAction::Detach, true),
        ],
    ];
    // Only a request tab has a file behind it — and it carries its own, so these
    // rows are never inert for one.
    if matches!(tab.key, TabKey::Request { .. }) {
        groups.push(vec![
            row("Copy Path", MenuAction::CopyPath, true),
            row("Reveal in Explorer", MenuAction::Reveal, true),
        ]);
    }
    groups.push(vec![row(
        "Configure Editor Tabs…",
        MenuAction::Settings,
        true,
    )]);
    groups
}

impl MenuAction {
    /// Carry the action out and close the menu.
    ///
    /// Every close goes through [`Tabs`] and the detach through
    /// `Workspace::detach_tab`, so there is one implementation per behaviour
    /// rather than a second copy living in the menu.
    fn apply(self, ws: &mut Workspace, i: usize, cx: &mut gpui::Context<Workspace>) {
        match self {
            MenuAction::Close => {
                ws.tabs_mut().remove(i);
            }
            MenuAction::CloseOthers => ws.tabs_mut().close_others(i),
            MenuAction::CloseAll => ws.tabs_mut().close_all(),
            MenuAction::CloseAllButPinned => ws.tabs_mut().close_all_but_pinned(),
            MenuAction::CloseLeft => ws.tabs_mut().close_left_of(i),
            MenuAction::CloseRight => ws.tabs_mut().close_right_of(i),
            MenuAction::TogglePin => ws.tabs_mut().toggle_pin(i),
            MenuAction::Detach => ws.detach_tab(i, cx),
            // The path of the tab that was right-clicked, not of the window: with
            // several files open the window has no single one.
            MenuAction::CopyPath => {
                if let Some(TabKey::Request { source, .. }) = ws.tabs().key_at(i) {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                        source.display().to_string(),
                    ));
                }
            }
            MenuAction::Reveal => {
                if let Some(TabKey::Request { source, .. }) = ws.tabs().key_at(i).cloned() {
                    reveal(&source);
                }
            }
            MenuAction::Settings => crate::open_settings(cx),
        }
        ws.tab_menu = None;
        cx.defer(crate::save_session);
        cx.notify();
    }
}

/// Show `path` in the OS file manager, with the file selected.
///
/// ponytail: Windows only — that is where this app ships, and `shell::` in
/// `main.rs` already assumes it. Add the `open`/`xdg-open` arms when another
/// platform is actually built. `explorer` exits non-zero even on success, so the
/// status is deliberately not checked; only a failure to spawn is reported.
fn reveal(path: &Path) {
    if let Err(e) = std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn()
    {
        note!("cannot reveal {}: {e:#}", path.display());
    }
}

fn menu_item(
    t: theme::Theme,
    i: usize,
    row: &MenuRow,
    cx: &mut gpui::Context<Workspace>,
) -> AnyElement {
    let action = row.action;
    let item = div()
        .id(row.label)
        .flex()
        .flex_none()
        .items_center()
        .h(control::MD)
        .px(space::S3)
        .rounded(radius::SM)
        .child(row.label);
    if row.enabled {
        item.text_color(t.text)
            .hover(|s| s.bg(t.hover()))
            .on_click(cx.listener(move |this, _, _, cx| action.apply(this, i, cx)))
            .into_any_element()
    } else {
        item.text_color(t.text_disabled()).into_any_element()
    }
}

/// The open tab menu, if there is one.
///
/// Rendered at the root of the window rather than inside the tab strip, because
/// the strip scrolls horizontally and would clip it.
///
/// Positioned at the click in window coordinates; `snap_to_window_with_margin`
/// pulls it back inside the viewport, so opening one on the last tab does not
/// put it half off the right edge. Dismissed by a mouse-down anywhere outside it
/// (`on_mouse_down_out`, capture phase) or by Escape — the latter needs the menu
/// to hold focus, which is why `Workspace` owns a handle for it.
pub fn menu(ws: &Workspace, cx: &mut gpui::Context<Workspace>) -> Option<impl IntoElement + use<>> {
    let (i, at) = ws.tab_menu?;
    let groups = menu_rows(ws.tabs(), i);
    if groups.is_empty() {
        return None;
    }
    let t = theme::theme(cx);
    // Built with `for` loops: a lazy `map` would let the `&mut Context` escape.
    let mut children = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        if g > 0 {
            children.push(divider(t).my(space::S1).into_any_element());
        }
        for row in group {
            children.push(menu_item(t, i, row, cx));
        }
    }
    Some(
        gpui::deferred(
            gpui::anchored()
                .position(at)
                .snap_to_window_with_margin(space::S2)
                .child(
                    div()
                        .id("tab-menu")
                        .track_focus(&ws.menu_focus)
                        .occlude()
                        .flex()
                        .flex_col()
                        .w(popup::WIDTH)
                        .p(space::S1)
                        .rounded(radius::MD)
                        .bg(t.surface)
                        .border_1()
                        .border_color(t.divider)
                        .shadow_md()
                        .text_size(text::BODY)
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.tab_menu = None;
                            cx.notify();
                        }))
                        .on_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, _, cx| {
                            if ev.keystroke.key == "escape" {
                                this.tab_menu = None;
                                cx.notify();
                            }
                        }))
                        .children(children),
                ),
        )
        // Above anything else deferred in the same frame.
        .with_priority(1),
    )
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
            pinned: false,
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
    fn reopening_a_request_activates_it_too() {
        let mut t: Tabs<&str> = Tabs::default();
        t.open(tab(req(Path::new("api.http"), 0), "req"));
        t.open(tab(req(Path::new("api.http"), 1), "req1"));
        t.open(tab(req(Path::new("api.http"), 0), "req"));
        assert_eq!(t.len(), 2);
        assert_eq!(t.active_key(), Some(&req(Path::new("api.http"), 0)));
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

    // --- the close family behind the tab menu ------------------------------

    #[test]
    fn closing_to_the_left_keeps_the_clicked_tab_active() {
        let mut t = tools(&["a", "b", "c", "d"]);
        t.activate(0);
        t.close_left_of(2);
        assert_eq!(keys(&t), vec![TabKey::Tool("c"), TabKey::Tool("d")]);
        // The active tab was one of the closed ones, so it falls to the survivor.
        assert_eq!(t.active_key(), Some(&TabKey::Tool("c")));

        // And an already-active tab further right keeps its identity.
        let mut t = tools(&["a", "b", "c", "d"]);
        t.activate(3);
        t.close_left_of(2);
        assert_eq!(t.active_key(), Some(&TabKey::Tool("d")));
        assert_eq!(t.active_index(), Some(1));
    }

    #[test]
    fn closing_to_the_left_of_the_first_tab_closes_nothing() {
        let mut t = tools(&["a", "b"]);
        t.activate(1);
        t.close_left_of(0);
        assert_eq!(keys(&t), vec![TabKey::Tool("a"), TabKey::Tool("b")]);
        assert_eq!(t.active_index(), Some(1));
    }

    #[test]
    fn closing_to_the_right_keeps_the_clicked_tab_active() {
        let mut t = tools(&["a", "b", "c", "d"]);
        assert_eq!(t.active_index(), Some(3));
        t.close_right_of(1);
        assert_eq!(keys(&t), vec![TabKey::Tool("a"), TabKey::Tool("b")]);
        assert_eq!(t.active_key(), Some(&TabKey::Tool("b")));

        // An active tab to the left of the cut is untouched.
        let mut t = tools(&["a", "b", "c"]);
        t.activate(0);
        t.close_right_of(1);
        assert_eq!(t.active_index(), Some(0));
    }

    #[test]
    fn closing_to_the_right_of_the_last_tab_closes_nothing() {
        let mut t = tools(&["a", "b"]);
        t.close_right_of(1);
        assert_eq!(keys(&t), vec![TabKey::Tool("a"), TabKey::Tool("b")]);
        // Past the end is inert too, not a panic.
        t.close_right_of(9);
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn closing_others_leaves_exactly_the_clicked_tab_active() {
        for active in 0..4 {
            let mut t = tools(&["a", "b", "c", "d"]);
            t.activate(active);
            t.close_others(2);
            assert_eq!(keys(&t), vec![TabKey::Tool("c")], "active was {active}");
            assert_eq!(t.active_index(), Some(0));
            assert_eq!(t.active_key(), Some(&TabKey::Tool("c")));
        }
        // From the first and the last position, and with one tab already.
        let mut t = tools(&["a", "b", "c"]);
        t.close_others(0);
        assert_eq!(keys(&t), vec![TabKey::Tool("a")]);
        let mut t = tools(&["a", "b", "c"]);
        t.close_others(2);
        assert_eq!(keys(&t), vec![TabKey::Tool("c")]);
        let mut t = tools(&["a"]);
        t.close_others(0);
        assert_eq!(keys(&t), vec![TabKey::Tool("a")]);
    }

    #[test]
    fn closing_others_on_a_tab_that_is_gone_empties_nothing() {
        let mut t = tools(&["a", "b"]);
        t.close_others(5);
        assert_eq!(t.len(), 2, "a stale index must not clear the window");
    }

    #[test]
    fn closing_all_leaves_an_empty_window_with_no_active_tab() {
        let mut t = tools(&["a", "b", "c"]);
        t.close_all();
        assert!(t.is_empty());
        assert_eq!(t.active_index(), None);
        // And it is idempotent, so a second call is not a panic.
        t.close_all();
        assert!(t.is_empty());
        // The list is genuinely reusable afterwards.
        t.open(tab(TabKey::Tool("d"), "d"));
        assert_eq!(t.active_index(), Some(0));
    }

    #[test]
    fn closing_left_and_right_together_is_closing_others() {
        let mut a = tools(&["a", "b", "c", "d", "e"]);
        a.activate(4);
        a.close_left_of(2);
        a.close_right_of(0);
        let mut b = tools(&["a", "b", "c", "d", "e"]);
        b.activate(4);
        b.close_others(2);
        assert_eq!(keys(&a), keys(&b));
        assert_eq!(a.active_index(), b.active_index());
    }

    // --- what the menu offers ---------------------------------------------

    fn labels(groups: &[Vec<MenuRow>]) -> Vec<(&'static str, bool)> {
        groups
            .iter()
            .flatten()
            .map(|r| (r.label, r.enabled))
            .collect()
    }

    fn find(groups: &[Vec<MenuRow>], label: &str) -> Option<bool> {
        groups
            .iter()
            .flatten()
            .find(|r| r.label == label)
            .map(|r| r.enabled)
    }

    #[test]
    fn the_range_closes_are_greyed_at_the_ends_of_the_strip() {
        let t = tools(&["a", "b", "c"]);
        let first = menu_rows(&t, 0);
        assert_eq!(find(&first, "Close Tabs to the Left"), Some(false));
        assert_eq!(find(&first, "Close Tabs to the Right"), Some(true));

        let last = menu_rows(&t, 2);
        assert_eq!(find(&last, "Close Tabs to the Left"), Some(true));
        assert_eq!(find(&last, "Close Tabs to the Right"), Some(false));

        let middle = menu_rows(&t, 1);
        assert_eq!(find(&middle, "Close Tabs to the Left"), Some(true));
        assert_eq!(find(&middle, "Close Tabs to the Right"), Some(true));
    }

    #[test]
    fn close_others_is_greyed_when_there_is_nothing_else_to_close() {
        let one = tools(&["a"]);
        let rows = menu_rows(&one, 0);
        assert_eq!(find(&rows, "Close Others"), Some(false));
        // Close and Close All always mean something, even for a lone tab.
        assert_eq!(find(&rows, "Close"), Some(true));
        assert_eq!(find(&rows, "Close All"), Some(true));
        assert_eq!(find(&rows, "Move to New Window"), Some(true));

        let two = tools(&["a", "b"]);
        assert_eq!(find(&menu_rows(&two, 0), "Close Others"), Some(true));
    }

    #[test]
    fn the_file_rows_are_absent_for_a_tool_tab_and_live_for_a_request() {
        let mut t = tools(&["a"]);
        t.open(tab(req(Path::new("api.http"), 0), "r0"));

        // A tool tab has no file behind it at all, so the rows are not shown.
        let tool = menu_rows(&t, 0);
        assert_eq!(find(&tool, "Copy Path"), None);
        assert_eq!(find(&tool, "Reveal in Explorer"), None);

        // A request tab carries its own file, so the rows always mean something.
        let req = menu_rows(&t, 1);
        assert_eq!(find(&req, "Copy Path"), Some(true));
        assert_eq!(find(&req, "Reveal in Explorer"), Some(true));

        // There is no longer a "no file loaded" case to grey them out for: the tab
        // carries the path, so if the tab exists the path does.
    }

    #[test]
    fn a_menu_for_a_tab_that_is_gone_has_no_rows() {
        let t = tools(&["a"]);
        assert!(menu_rows(&t, 1).is_empty());
        let empty: Tabs<&str> = Tabs::default();
        assert!(menu_rows(&empty, 0).is_empty());
    }

    /// The menu is grouped, and the groups are what the dividers are drawn from —
    /// so an empty group would paint two rules in a row.
    #[test]
    fn every_group_holds_at_least_one_row() {
        let mut t = tools(&["a"]);
        t.open(tab(req(Path::new("api.http"), 0), "r0"));
        for i in 0..t.len() {
            let groups = menu_rows(&t, i);
            assert!(groups.len() >= 4, "{i}: {:?}", labels(&groups));
            assert!(groups.iter().all(|g| !g.is_empty()), "{i}");
        }
    }

    /// Every action the menu can name must be reachable from some tab, or a
    /// variant was added and never wired up.
    #[test]
    fn no_menu_action_is_unreachable() {
        let mut t = tools(&["a", "b"]);
        t.open(tab(req(Path::new("api.http"), 0), "r0"));
        let mut seen = Vec::new();
        for i in 0..t.len() {
            for row in menu_rows(&t, i).iter().flatten() {
                if !seen.contains(&row.action) {
                    seen.push(row.action);
                }
            }
        }
        for action in [
            MenuAction::Close,
            MenuAction::CloseOthers,
            MenuAction::CloseAll,
            MenuAction::CloseAllButPinned,
            MenuAction::CloseLeft,
            MenuAction::CloseRight,
            MenuAction::TogglePin,
            MenuAction::Detach,
            MenuAction::CopyPath,
            MenuAction::Reveal,
            MenuAction::Settings,
        ] {
            assert!(seen.contains(&action), "{action:?} is in no menu");
        }
    }

    // --- pinning ----------------------------------------------------------

    fn pinned(t: &Tabs<&'static str>) -> Vec<bool> {
        t.iter().map(|t| t.pinned).collect()
    }

    #[test]
    fn pinning_moves_the_tab_ahead_of_the_unpinned_ones() {
        let mut t = tools(&["a", "b", "c", "d"]);
        t.set_pinned(2, true);
        assert_eq!(
            keys(&t),
            vec![
                TabKey::Tool("c"),
                TabKey::Tool("a"),
                TabKey::Tool("b"),
                TabKey::Tool("d"),
            ]
        );
        assert_eq!(pinned(&t), vec![true, false, false, false]);
        // A second pin joins the end of the pinned run, keeping the order the two
        // pinned tabs had.
        t.set_pinned(3, true);
        assert_eq!(
            keys(&t),
            vec![
                TabKey::Tool("c"),
                TabKey::Tool("d"),
                TabKey::Tool("a"),
                TabKey::Tool("b"),
            ]
        );
        assert_eq!(t.pinned_count(), 2);
        // And the unpinned run kept its own order throughout.
        assert!(t.is_pinned(0) && t.is_pinned(1));
        assert!(!t.is_pinned(2) && !t.is_pinned(3));
    }

    /// The reorder is where this breaks: the active tab has to stay the *same
    /// tab*, not the same index.
    #[test]
    fn a_reorder_keeps_the_same_tab_active_not_the_same_index() {
        // Active tab is the one being pinned: it moves, and follows.
        let mut t = tools(&["a", "b", "c"]);
        t.activate(2);
        t.set_pinned(2, true);
        assert_eq!(t.active_index(), Some(0));
        assert_eq!(t.active_key(), Some(&TabKey::Tool("c")));

        // Active tab is a bystander pushed to the right by someone else's pin.
        let mut t = tools(&["a", "b", "c"]);
        t.activate(0);
        t.set_pinned(2, true);
        assert_eq!(t.active_key(), Some(&TabKey::Tool("a")));
        assert_eq!(t.active_index(), Some(1));

        // And unpinning, which moves the tab the other way.
        let mut t = tools(&["a", "b", "c"]);
        t.set_pinned(0, true);
        t.activate(1); // "b", now at 1 because "a" is pinned and first anyway
        assert_eq!(t.active_key(), Some(&TabKey::Tool("b")));
        t.set_pinned(0, false);
        assert_eq!(t.active_key(), Some(&TabKey::Tool("b")));
    }

    #[test]
    fn toggling_a_pin_twice_is_a_no_op_on_a_pinned_first_list() {
        let mut t = tools(&["a", "b", "c"]);
        t.activate(1);
        t.toggle_pin(1);
        assert!(t.is_pinned(0));
        // The tab moved to the front; toggle it back where it now is.
        t.toggle_pin(0);
        assert_eq!(t.pinned_count(), 0);
        assert_eq!(t.active_key(), Some(&TabKey::Tool("b")));
        // The tab stays where the pin put it — at the start of the unpinned run,
        // which with nothing pinned is the front. Unpinning does *not* restore the
        // position the tab had before, and IntelliJ does not either; remembering
        // it would mean storing a second index per tab for no stated need.
        assert_eq!(
            keys(&t),
            vec![TabKey::Tool("b"), TabKey::Tool("a"), TabKey::Tool("c")]
        );
        // Out of range is inert.
        t.toggle_pin(9);
        assert_eq!(t.pinned_count(), 0);
    }

    #[test]
    fn a_pinned_tab_survives_close_others_and_the_range_closes() {
        // c is pinned, so it sits first; close others on "a".
        let mut t = tools(&["a", "b", "c", "d"]);
        t.set_pinned(2, true);
        let at = t.find(&TabKey::Tool("a")).expect("a");
        t.close_others(at);
        assert_eq!(keys(&t), vec![TabKey::Tool("c"), TabKey::Tool("a")]);
        assert_eq!(t.active_key(), Some(&TabKey::Tool("a")));

        // Closing to the right of the pinned tab spares nothing but pins.
        let mut t = tools(&["a", "b", "c"]);
        t.set_pinned(1, true); // b first
        t.close_right_of(0);
        assert_eq!(keys(&t), vec![TabKey::Tool("b")]);

        // Closing to the left spares a pinned tab sitting there.
        let mut t = tools(&["a", "b", "c"]);
        t.set_pinned(0, true); // a stays first, pinned
        t.close_left_of(2);
        assert_eq!(keys(&t), vec![TabKey::Tool("a"), TabKey::Tool("c")]);
        assert_eq!(t.active_key(), Some(&TabKey::Tool("c")));
    }

    #[test]
    fn closing_the_named_tab_closes_it_even_when_it_is_pinned() {
        let mut t = tools(&["a", "b"]);
        t.set_pinned(0, true);
        assert!(
            t.remove(0).is_some(),
            "an explicit target is not a bulk close"
        );
        assert_eq!(keys(&t), vec![TabKey::Tool("b")]);
    }

    #[test]
    fn close_all_but_pinned_keeps_exactly_the_pins_and_close_all_keeps_nothing() {
        let mut t = tools(&["a", "b", "c", "d"]);
        t.set_pinned(1, true);
        t.set_pinned(3, true); // b, d pinned and first
        t.activate(3); // one of the doomed ones
        t.close_all_but_pinned();
        assert_eq!(keys(&t), vec![TabKey::Tool("b"), TabKey::Tool("d")]);
        assert_eq!(t.active_index(), Some(1));
        assert!(t.active_index().is_some_and(|i| i < t.len()));

        // Nothing pinned: it empties the window, and does not panic doing it.
        let mut t = tools(&["a", "b"]);
        t.close_all_but_pinned();
        assert!(t.is_empty());
        assert_eq!(t.active_index(), None);

        // Close All is the escape hatch and ignores pins entirely.
        let mut t = tools(&["a", "b"]);
        t.set_pinned(0, true);
        t.close_all();
        assert!(t.is_empty());
        assert_eq!(t.active_index(), None);
    }

    #[test]
    fn a_window_of_nothing_but_pinned_tabs_resists_every_bulk_close() {
        let mut t = tools(&["a", "b", "c"]);
        for i in 0..3 {
            t.set_pinned(i, true);
        }
        t.close_others(0);
        t.close_left_of(2);
        t.close_right_of(0);
        t.close_all_but_pinned();
        assert_eq!(t.len(), 3, "pins are the point");
        // And the menu says so rather than offering dead rows.
        let rows = menu_rows(&t, 1);
        assert_eq!(find(&rows, "Close Others"), Some(false));
        assert_eq!(find(&rows, "Close Tabs to the Left"), Some(false));
        assert_eq!(find(&rows, "Close Tabs to the Right"), Some(false));
        assert_eq!(
            find(&rows, "Close All but Pinned"),
            Some(false),
            "there is nothing left for it to close"
        );
        // Close All still means something: it is the only way out.
        assert_eq!(find(&rows, "Close All"), Some(true));
    }

    #[test]
    fn the_menu_names_the_direction_the_pin_will_go() {
        let mut t = tools(&["a", "b"]);
        let rows = menu_rows(&t, 0);
        assert_eq!(find(&rows, "Pin Tab"), Some(true));
        assert_eq!(find(&rows, "Unpin Tab"), None);

        t.set_pinned(0, true);
        let rows = menu_rows(&t, 0);
        assert_eq!(find(&rows, "Unpin Tab"), Some(true));
        assert_eq!(find(&rows, "Pin Tab"), None);
    }

    #[test]
    fn close_all_but_pinned_is_greyed_unless_it_would_do_something() {
        // Nothing pinned: it would be a second Close All.
        let t = tools(&["a", "b"]);
        assert_eq!(find(&menu_rows(&t, 0), "Close All but Pinned"), Some(false));

        // Something pinned and something not: it means something.
        let mut t = tools(&["a", "b"]);
        t.set_pinned(0, true);
        assert_eq!(find(&menu_rows(&t, 0), "Close All but Pinned"), Some(true));

        // Everything pinned: nothing to close.
        t.set_pinned(1, true);
        assert_eq!(find(&menu_rows(&t, 0), "Close All but Pinned"), Some(false));
    }

    #[test]
    fn a_detached_tab_takes_its_pin_with_it() {
        let mut from = tools(&["a", "b"]);
        from.set_pinned(1, true);
        let at = from.find(&TabKey::Tool("b")).expect("b");
        let moved = from.remove(at).expect("tab");
        assert!(moved.pinned, "a pin belongs to the tab, not to the window");
        let mut to: Tabs<&str> = Tabs::default();
        to.open(moved);
        assert!(to.is_pinned(0));
        assert_eq!(to.active_key(), Some(&TabKey::Tool("b")));
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
            pinned: false,
            active: false,
        }
    }

    /// The same record, marked as its section's active tab.
    fn active(mut r: TabRecord) -> TabRecord {
        r.active = true;
        r
    }

    /// A request tab key. One helper, so a change to the key's shape is one edit
    /// here rather than thirty.
    fn req(source: &Path, index: usize) -> TabKey {
        TabKey::Request {
            source: source.to_path_buf(),
            index,
        }
    }

    fn requests(source: &Path, indices: &[usize]) -> Tabs<&'static str> {
        let mut t = Tabs::default();
        for i in indices {
            t.open(tab(req(source, *i), "r"));
        }
        t
    }

    fn none() -> Tabs<&'static str> {
        Tabs::default()
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

    /// `snapshot` as a workspace with no extra files open would call it: the source
    /// list is exactly the files the request tabs name.
    fn snap(reqs: &Tabs<&'static str>, tools: &Tabs<&'static str>) -> WindowSession {
        let mut sources: Vec<PathBuf> = Vec::new();
        for tab in reqs.iter() {
            if let TabKey::Request { source, .. } = &tab.key
                && !sources.contains(source)
            {
                sources.push(source.clone());
            }
        }
        snapshot(reqs, tools, &sources)
    }

    fn load_requests(p: &Path) -> Option<usize> {
        request_pilot::session::load(p)
            .ok()
            .map(|l| l.requests.len())
    }

    #[test]
    fn a_tool_tab_round_trips() {
        let stored = snap(&none(), &tools(&[TOOL]));
        assert_eq!(stored.tabs, [active(record("tool", TOOL, None))]);
        assert_eq!(stored.active_tab, Some(0));

        let plan = plan(&stored, |_| unreachable!("no request tabs to load"));
        assert_eq!(
            plan.tabs,
            [Restore::Tool {
                id: TOOL,
                pinned: false,
                active: true,
            }]
        );
        assert!(plan.sources.is_empty());
        assert_eq!(plan.note, None);
    }

    /// The point of the split: each strip remembers the tab *it* was on, so
    /// switching back to a section lands where you left it. One index per window
    /// could only ever hold one of the two.
    #[test]
    fn each_section_keeps_its_own_active_tab() {
        let path = http_file("per-section-active", 3);
        let mut reqs = requests(&path, &[0, 1, 2]);
        reqs.activate(1);
        let mut tls = tools(&[TOOL, "qr-code-generator"]);
        tls.activate(0);

        let stored = snap(&reqs, &tls);
        let marks: Vec<bool> = stored.tabs.iter().map(|r| r.active).collect();
        assert_eq!(
            marks,
            vec![false, true, false, true, false],
            "one active tab per section, requests first: {:?}",
            stored.tabs
        );

        let plan = plan(&stored, load_requests);
        let back: Vec<bool> = plan.tabs.iter().map(Restore::active).collect();
        assert_eq!(back, vec![false, true, false, true, false]);
        // And each mark is on the tab it belonged to, not merely in the right slot.
        assert_eq!(
            plan.tabs[1],
            Restore::Request {
                path,
                index: 1,
                pinned: false,
                active: true,
            }
        );
        assert_eq!(
            plan.tabs[3],
            Restore::Tool {
                id: TOOL,
                pinned: false,
                active: true,
            }
        );
    }

    /// A pin has to survive a restart or it is decoration — and the case that
    /// matters is the *request* tab, which is why `pinned` is its own column
    /// rather than an overload of `detail`.
    #[test]
    fn a_pin_round_trips_for_both_kinds_of_tab() {
        let path = http_file("pin-roundtrip", 2);
        let mut reqs = requests(&path, &[1, 0]);
        // Pin the first request: it moves to the front, so the stored order is
        // already pinned-first and restoring cannot reshuffle it.
        let at = reqs.find(&req(&path, 1)).expect("r1");
        reqs.set_pinned(at, true);
        reqs.activate(reqs.find(&req(&path, 0)).expect("r0"));

        let stored = snap(&reqs, &tools(&[TOOL]));
        let flags: Vec<bool> = stored.tabs.iter().map(|r| r.pinned).collect();
        assert_eq!(flags, vec![true, false, false], "{:?}", stored.tabs);
        assert_eq!(stored.tabs[0].detail.as_deref(), Some("1"));

        let plan = plan(&stored, load_requests);
        let back: Vec<bool> = plan.tabs.iter().map(Restore::pinned).collect();
        assert_eq!(back, vec![true, false, false]);
        assert_eq!(
            plan.tabs[0],
            Restore::Request {
                path,
                index: 1,
                pinned: true,
                active: false,
            }
        );

        // A pinned *tool* tab too, and the flag rides on the tool variant.
        let mut tls = tools(&[TOOL]);
        tls.set_pinned(0, true);
        let stored = snap(&none(), &tls);
        assert!(stored.tabs[0].pinned);
        // `super::`, because `plan` is shadowed by the binding above.
        let plan = super::plan(&stored, |_| unreachable!());
        assert_eq!(
            plan.tabs,
            [Restore::Tool {
                id: TOOL,
                pinned: true,
                active: true,
            }]
        );
    }

    #[test]
    fn a_request_tab_round_trips_with_its_source_path_and_index() {
        let path = http_file("roundtrip", 3);
        let stored = snap(&requests(&path, &[2]), &tools(&[TOOL]));
        assert_eq!(
            stored.tabs,
            [
                active(record("request", &path.display().to_string(), Some("2"))),
                active(record("tool", TOOL, None)),
            ]
        );

        let mut loads = 0;
        let plan = plan(&stored, |p| {
            loads += 1;
            load_requests(p)
        });
        assert_eq!(loads, 1, "the file must be loaded once");
        assert_eq!(
            plan.tabs,
            [
                Restore::Request {
                    path: path.clone(),
                    index: 2,
                    pinned: false,
                    active: true,
                },
                Restore::Tool {
                    id: TOOL,
                    pinned: false,
                    active: true,
                },
            ]
        );
        assert_eq!(plan.sources, std::slice::from_ref(&path));
        assert_eq!(plan.note, None);
    }

    /// Every request tab carries its own file, so there is no longer such a thing
    /// as a request tab that cannot be written down. While a window had one source
    /// path, a tab opened before a file was loaded was silently dropped on save.
    #[test]
    fn a_request_tab_is_persisted_with_its_own_file() {
        let path = http_file("own-file", 1);
        let stored = snap(&requests(&path, &[0]), &tools(&[TOOL]));
        assert_eq!(stored.tabs.len(), 2, "{:?}", stored.tabs);
        assert_eq!(stored.tabs[0].kind, "request");
        assert_eq!(stored.tabs[0].key, path.display().to_string());
        // And the file is in the source list in its own right.
        assert_eq!(stored.sources, [path.display().to_string()]);
    }

    #[test]
    fn a_tool_id_that_no_longer_exists_is_skipped_silently() {
        let stored = WindowSession {
            sources: Vec::new(),
            tabs: vec![
                active(record("tool", "tool-that-was-renamed", None)),
                record("tool", TOOL, None),
                // An unknown kind is stale data too, not a crash.
                record("sandwich", "blt", None),
            ],
            active_tab: Some(0),
        };
        let plan = plan(&stored, |_| unreachable!());
        assert_eq!(
            plan.tabs,
            [Restore::Tool {
                id: TOOL,
                // The tab that was active is gone, so the mark falls to the first
                // survivor rather than leaving the strip with none.
                pinned: false,
                active: true,
            }]
        );
        assert_eq!(plan.note, None, "a dropped tool is not worth a message");
    }

    #[test]
    fn a_request_whose_file_is_gone_is_skipped_with_a_note() {
        let stored = WindowSession {
            sources: Vec::new(),
            tabs: vec![
                active(record("request", "definitely/not/here.http", Some("0"))),
                active(record("tool", TOOL, None)),
            ],
            active_tab: Some(0),
        };
        let plan = plan(&stored, load_requests);
        assert_eq!(
            plan.tabs,
            [Restore::Tool {
                id: TOOL,
                pinned: false,
                active: true,
            }]
        );
        assert!(plan.sources.is_empty());
        let note = plan.note.expect("a lost file must be visible, not silent");
        assert!(note.contains("here.http"), "{note}");
    }

    #[test]
    fn a_stored_index_past_the_end_of_the_file_is_skipped() {
        let path = http_file("shrunk", 1);
        let key = path.display().to_string();
        let stored = WindowSession {
            sources: Vec::new(),
            tabs: vec![
                record("request", &key, Some("0")),
                active(record("request", &key, Some("7"))),
                // Not a number at all.
                record("request", &key, Some("x")),
                record("request", &key, None),
            ],
            active_tab: Some(1),
        };
        let plan = plan(&stored, load_requests);
        assert_eq!(
            plan.tabs,
            [Restore::Request {
                path,
                index: 0,
                pinned: false,
                // The active tab was one of the dropped ones.
                active: true,
            }]
        );
    }

    /// A database written before migration v4 has no per-tab flag at all, only
    /// one index per window. It has to keep restoring onto that tab.
    #[test]
    fn a_session_from_before_the_split_still_restores_its_active_tab() {
        let stored = WindowSession {
            sources: Vec::new(),
            tabs: vec![
                record("tool", "qr-code-generator", None),
                record("tool", TOOL, None),
            ],
            active_tab: Some(1),
        };
        let plan = plan(&stored, |_| unreachable!());
        let marks: Vec<bool> = plan.tabs.iter().map(Restore::active).collect();
        assert_eq!(marks, vec![false, true]);

        // An index the stored tabs never had falls back to the first one, the way
        // a clamped index used to.
        let stored = WindowSession {
            sources: Vec::new(),
            tabs: vec![record("tool", TOOL, None)],
            active_tab: Some(9),
        };
        // `super::`, because `plan` is shadowed by the binding above.
        assert!(super::plan(&stored, |_| unreachable!()).tabs[0].active());

        // And nothing restorable means nothing to mark, rather than a panic.
        let empty = WindowSession {
            sources: Vec::new(),
            tabs: vec![record("tool", "gone", None)],
            active_tab: Some(0),
        };
        assert!(super::plan(&empty, |_| unreachable!()).tabs.is_empty());
    }

    /// Every section that came back has exactly one active tab — never two (which
    /// strip would win?) and never none (which would open on a blank pane).
    #[test]
    fn each_restored_section_has_exactly_one_active_tab() {
        let path = http_file("exactly-one", 2);
        let key = path.display().to_string();
        for active_at in [None, Some(0), Some(2), Some(9)] {
            let mut tabs = vec![
                record("request", &key, Some("0")),
                record("request", &key, Some("1")),
                record("tool", TOOL, None),
                record("tool", "qr-code-generator", None),
            ];
            if let Some(i) = active_at
                && let Some(r) = tabs.get_mut(i)
            {
                r.active = true;
            }
            let plan = plan(
                &WindowSession {
                    sources: Vec::new(),
                    tabs,
                    active_tab: None,
                },
                load_requests,
            );
            assert_eq!(plan.tabs.len(), 4, "{active_at:?}");
            for tools in [false, true] {
                let n = plan
                    .tabs
                    .iter()
                    .filter(|r| matches!(r, Restore::Tool { .. }) == tools && r.active())
                    .count();
                assert_eq!(n, 1, "{active_at:?}, tools={tools}");
            }
        }
    }

    /// The variables tab is workspace state, so it has nothing to resolve — but it
    /// still has to come back, and it belongs to the Requests strip.
    #[test]
    fn the_variables_tab_round_trips_without_a_source_file() {
        let mut reqs: Tabs<&str> = Tabs::default();
        reqs.open(tab(TabKey::Vars, "Variables"));
        let stored = snap(&reqs, &none());
        assert_eq!(stored.tabs, [active(record("vars", "", None))]);

        let plan = plan(&stored, |_| unreachable!("no request tabs to load"));
        assert_eq!(
            plan.tabs,
            [Restore::Vars {
                pinned: false,
                active: true
            }]
        );
        assert!(plan.sources.is_empty());

        // It sorts and pins like any other tab, and is not confused for a request:
        // the file rows are for tabs that have a file behind them.
        let mut reqs: Tabs<&str> = Tabs::default();
        reqs.open(tab(TabKey::Vars, "Variables"));
        reqs.open(tab(req(Path::new("api.http"), 0), "r0"));
        reqs.set_pinned(0, true);
        assert!(reqs.is_pinned(0));
        let rows = menu_rows(&reqs, reqs.find(&TabKey::Vars).expect("vars"));
        assert_eq!(find(&rows, "Copy Path"), None);
        // And opening it twice is one tab, like every other key.
        reqs.open(tab(TabKey::Vars, "Variables"));
        assert_eq!(
            reqs.iter().filter(|t| t.key == TabKey::Vars).count(),
            1,
            "the variables tab must not duplicate"
        );
    }

    /// Two files, both restored. This test used to assert the opposite — that a
    /// window owned one source and records naming a second were stale — which is
    /// exactly the restriction that had to go.
    #[test]
    fn tabs_from_several_files_all_come_back_and_each_file_is_read_once() {
        let first = http_file("two-a", 2);
        let second = http_file("two-b", 2);
        let stored = WindowSession {
            sources: Vec::new(),
            tabs: vec![
                active(record("request", &first.display().to_string(), Some("0"))),
                record("request", &second.display().to_string(), Some("1")),
                // A second tab into the first file must not read it again.
                record("request", &first.display().to_string(), Some("1")),
            ],
            active_tab: Some(0),
        };
        let mut loads: Vec<PathBuf> = Vec::new();
        let plan = plan(&stored, |p| {
            loads.push(p.to_path_buf());
            load_requests(p)
        });
        assert_eq!(
            loads,
            [first.clone(), second.clone()],
            "each file read once, in the order the tabs name it"
        );
        assert_eq!(plan.tabs.len(), 3, "{:?}", plan.tabs);
        assert_eq!(plan.sources, [first.clone(), second.clone()]);
        assert_eq!(
            plan.tabs[1],
            Restore::Request {
                path: second,
                index: 1,
                pinned: false,
                active: false,
            }
        );
    }

    /// A file with no tab open in it is still open, which is what the stored source
    /// list is for — inferring the list from the tabs loses it.
    #[test]
    fn a_file_with_no_open_tab_still_comes_back() {
        let quiet = http_file("quiet", 1);
        let busy = http_file("busy", 1);
        let stored = WindowSession {
            sources: vec![
                quiet.display().to_string(),
                busy.display().to_string(),
                // A duplicate is not two entries.
                quiet.display().to_string(),
            ],
            tabs: vec![active(record(
                "request",
                &busy.display().to_string(),
                Some("0"),
            ))],
            active_tab: Some(0),
        };
        let mut loads: Vec<PathBuf> = Vec::new();
        let plan = plan(&stored, |p| {
            loads.push(p.to_path_buf());
            load_requests(p)
        });
        assert_eq!(
            plan.sources,
            [quiet.clone(), busy.clone()],
            "the stored order wins, and the quiet file is in it"
        );
        assert_eq!(
            loads,
            [quiet, busy],
            "each file once, the stored list included"
        );
        assert_eq!(plan.tabs.len(), 1, "one tab was open, one comes back");
    }

    /// A file in the stored list that has since gone says so, and does not take the
    /// rest of the window with it.
    #[test]
    fn a_stored_file_that_is_gone_is_reported_and_the_others_still_open() {
        let there = http_file("still-there", 1);
        let stored = WindowSession {
            sources: vec![
                "definitely/not/here.http".into(),
                there.display().to_string(),
            ],
            tabs: vec![active(record("tool", TOOL, None))],
            active_tab: Some(0),
        };
        let plan = plan(&stored, load_requests);
        assert_eq!(plan.sources, [there]);
        let note = plan.note.expect("a lost file must be visible");
        assert!(note.contains("here.http"), "{note}");
        assert_eq!(plan.tabs.len(), 1, "the tool tab is unaffected");
    }
}
