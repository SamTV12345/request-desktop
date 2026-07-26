mod assets;
mod custom;
mod editor;
mod request_pane;
mod session;
mod tabs;
mod text_input;
mod theme;
mod tool_pane;
mod track;

use gpui::{
    App, Application, Bounds, Context, Entity, Global, IntoElement, ParentElement,
    PathPromptOptions, Pixels, Render, Rgba, SharedString, Styled, WeakEntity, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, size,
};
use request_pane::RequestPane;
use rp_core::interp::Vars;
use rp_core::{Environment, Request, bru, httpfile, vars};
use rp_devtools::Tool;
use rp_store::{Store, WindowSession};
use std::path::Path;
use tabs::{Content, Restore, Tab, TabKey, Tabs};
use text_input::TextInput;
use tool_pane::ToolPane;

// Fixed widths the shell needs. A panel width is a layout decision, not a gap,
// so it is not in `theme::space` — but every one of them is a multiple of the
// 4-px grid, so the columns still line up with the padding inside them.
const RAIL_W: Pixels = px(160.);
const SIDEBAR_W: Pixels = px(260.);
/// Method badge width: wide enough for `OPTIONS` at `text::SMALL`, so the
/// request names line up into a column instead of stepping in and out.
const METHOD_W: Pixels = px(56.);
/// The name column in the variables panel.
const VAR_NAME_W: Pixels = px(96.);

/// The SQLite store plus the registry of live windows, so any window can write
/// the whole session.
///
/// A `Global` rather than something threaded through every `Workspace`: windows
/// are created from several places and there is exactly one database.
///
/// `store` is `None` when the database could not be opened. Persistence is never
/// load-bearing — a corrupt file or a read-only data directory costs the user
/// their restored tabs, never their ability to start the app.
struct Persist {
    store: Option<Store>,
    /// Weak, and pruned on every save, so a closed window's `Workspace` is
    /// dropped instead of leaking here. Push order is window order.
    windows: Vec<WeakEntity<Workspace>>,
}

impl Global for Persist {}

/// Write every live window's tabs to the database.
///
/// Called on tab-list mutations — open, close, activate, detach, window closed —
/// which is the middle ground between saving on every keystroke and saving only
/// at quit, where a crash would take the session with it. Never called from
/// `render`.
///
/// ponytail: a synchronous SQLite transaction on the UI thread. It is one
/// `DELETE` plus a handful of `INSERT`s for a handful of tabs; move it to a
/// background task if the session ever grows big enough to hitch a click.
fn save_session(cx: &mut App) {
    if cx.try_global::<Persist>().is_none_or(|p| p.store.is_none()) {
        return;
    }
    // A closed window's `Workspace` entity stays alive for a while afterwards, so
    // "the entity still upgrades" is not the same as "the window is open" — ask
    // the window list, or closing one window of two would keep storing both.
    let open: Vec<gpui::EntityId> = cx
        .windows()
        .into_iter()
        .filter_map(|w| w.downcast::<Workspace>())
        .filter_map(|w| w.entity(cx).ok())
        .map(|root| root.entity_id())
        .collect();

    // Cloned out so the global borrow ends before the entities are read.
    let handles = cx.global::<Persist>().windows.clone();
    let mut sessions = Vec::with_capacity(handles.len());
    let mut alive = Vec::with_capacity(handles.len());
    for handle in handles {
        if !open.contains(&handle.entity_id()) {
            continue;
        }
        if let Some(ws) = handle.upgrade() {
            let ws = ws.read(cx);
            sessions.push(tabs::snapshot(&ws.tabs, ws.source_path.as_deref()));
            alive.push(handle);
        }
    }
    let persist = cx.global_mut::<Persist>();
    persist.windows = alive;
    if let Some(store) = persist.store.as_mut()
        && let Err(e) = store.save_session(&sessions)
    {
        eprintln!("request-pilot: cannot save the session: {e:#}");
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Requests,
    Tools,
    Track,
}

struct Workspace {
    section: Section,
    // Requests
    requests: Vec<Request>,
    source: SharedString,
    /// Where `requests` came from, so edits can be written back.
    source_path: Option<std::path::PathBuf>,
    error: Option<SharedString>,
    /// Environments found next to the opened file.
    environments: Vec<Environment>,
    selected_env: Option<usize>,
    /// Editable variable rows: name plus a live value editor.
    var_rows: Vec<(String, Entity<TextInput>)>,
    /// Values captured from earlier responses via `client.global.set`. These
    /// win over the environment, which is how an auth token flows forward.
    captured: Vec<(String, String)>,
    // Tools
    tool_query: Entity<TextInput>,
    /// Time tracking. Owns its own list and detail pane, and polls the database
    /// only while its section is on screen.
    track: Entity<track::Track>,
    /// Everything open in this window. Tools and requests share one list, and
    /// each tab owns its view, so switching tabs never rebuilds a pane.
    tabs: Tabs,
}

impl Workspace {
    fn new(cx: &mut Context<Self>) -> Self {
        let tool_query = cx.new(|cx| TextInput::new(cx, "Search tools…"));
        cx.observe(&tool_query, |_, _, cx| cx.notify()).detach();
        let track = cx.new(track::Track::new);
        // Register for session snapshots. Weak, so closing the window still
        // drops the workspace.
        let weak = cx.weak_entity();
        if cx.has_global::<Persist>() {
            cx.global_mut::<Persist>().windows.push(weak);
        }
        Self {
            section: Section::Requests,
            requests: Vec::new(),
            source: "nothing open".into(),
            source_path: None,
            error: None,
            environments: Vec::new(),
            selected_env: None,
            var_rows: Vec::new(),
            captured: Vec::new(),
            tool_query,
            track,
            tabs: Tabs::default(),
        }
    }

    /// Switch the rail. The single place the section changes by hand, which is
    /// why the tracker's polling is started and stopped here.
    fn set_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        self.track
            .update(cx, |t, cx| t.set_visible(section == Section::Track, cx));
        cx.notify();
    }

    /// The active tab, when it is a request. `Request(usize)` indexes into
    /// `self.requests`, which is why `load` drops request tabs.
    fn active_request(&self) -> Option<(usize, Entity<RequestPane>)> {
        match self.tabs.active() {
            Some(Tab {
                key: TabKey::Request(i),
                content: Content::Request(pane),
                ..
            }) => Some((*i, pane.clone())),
            _ => None,
        }
    }

    /// Open request `i` from `self.requests` in a tab, or activate the tab it is
    /// already in. Shared by the sidebar and by session restore.
    fn open_request(&mut self, i: usize, cx: &mut Context<Self>) {
        let key = TabKey::Request(i);
        // Already open here? Activate it, the way Zed does, instead of building
        // a second copy.
        if let Some(at) = self.tabs.find(&key) {
            self.tabs.activate(at);
            return;
        }
        let Some(req) = self.requests.get(i).cloned() else {
            return;
        };
        let title = SharedString::from(req.name.clone());
        let vars = self.effective_vars(cx);
        let pane = cx.new(|cx| RequestPane::new(req, vars, cx));
        // A finished response may carry variables for the next request, so watch
        // the pane for them.
        cx.observe(&pane, |this, pane, cx| this.capture_from(&pane, cx))
            .detach();
        self.tabs.open(Tab {
            key,
            title,
            content: Content::Request(pane),
        });
    }

    /// Same for a tool: the bespoke pane if there is one, otherwise the generic
    /// form, otherwise the "no UI" marker and its reason.
    fn open_tool(&mut self, tool: &'static Tool, window: &mut Window, cx: &mut Context<Self>) {
        let key = TabKey::Tool(tool.id);
        if let Some(at) = self.tabs.find(&key) {
            self.tabs.activate(at);
            return;
        }
        let content = match custom::pane(tool.id, window, cx) {
            Some(view) => Content::Custom(view),
            None if unavailable(tool) => Content::Unavailable(custom::unavailable_reason(tool.id)),
            None => Content::Generic(cx.new(|cx| ToolPane::new(tool, window, cx))),
        };
        self.tabs.open(Tab {
            key,
            title: tool.name.into(),
            content,
        });
    }

    /// Rebuild the tabs a previous run had open.
    ///
    /// Only *which* tabs were open comes back. Half-typed input in a tool pane
    /// and fetched responses are deliberately not persisted, so a restored tab
    /// is a fresh one — that is by design, not a bug.
    ///
    /// Anything that no longer resolves is dropped: a renamed tool id silently,
    /// a request file that moved with a note in the status line.
    fn restore(&mut self, session: &WindowSession, window: &mut Window, cx: &mut Context<Self>) {
        let mut loaded = None;
        let plan = tabs::plan(session, |path| {
            let l = session::load(path).ok()?;
            let n = l.requests.len();
            loaded = Some(l);
            Some(n)
        });
        if let (Some(l), Some(path)) = (loaded, plan.source.clone()) {
            self.source = path.display().to_string().into();
            self.source_path = Some(path);
            self.requests = l.requests;
            self.environments = l.environments;
            self.selected_env = (!self.environments.is_empty()).then_some(0);
            self.rebuild_var_rows(&l.declared, cx);
        }
        for restore in &plan.tabs {
            match restore {
                Restore::Tool(id) => {
                    // Already resolved by `plan`, but never unwrap on stored data.
                    if let Some(tool) = rp_devtools::find(id) {
                        self.open_tool(tool, window, cx);
                    }
                }
                Restore::Request { index, .. } => self.open_request(*index, cx),
            }
        }
        self.tabs.activate(plan.active.unwrap_or(0));
        // Show the section the active tab belongs to, so a restored window opens
        // on what the user was last looking at. A note has to be readable, and
        // the status line lives in the requests sidebar.
        self.section = match (&plan.note, self.tabs.active_key()) {
            (None, Some(TabKey::Tool(_))) => Section::Tools,
            _ => Section::Requests,
        };
        self.error = plan.note.map(Into::into);
    }

    /// A `.http` file yields many requests; a directory is a Bruno collection.
    fn load(&mut self, path: &Path, cx: &mut Context<Self>) {
        // Request tabs point at indices into the old `requests`, so they cannot
        // survive a different file. Tool tabs are unaffected.
        self.tabs.retain(|k| !matches!(k, TabKey::Request(_)));
        self.error = None;
        self.captured.clear();
        self.environments.clear();
        self.selected_env = None;

        match session::load(path) {
            Ok(loaded) => {
                self.source = path.display().to_string().into();
                self.source_path = Some(path.to_path_buf());
                self.requests = loaded.requests;
                self.environments = loaded.environments;
                self.selected_env = (!self.environments.is_empty()).then_some(0);
                self.rebuild_var_rows(&loaded.declared, cx);
                if self.requests.is_empty() {
                    self.error = Some("no requests found in that file".into());
                }
            }
            // Errors from `session::load` already name the file.
            Err(e) => self.error = Some(e.to_string().into()),
        }
        cx.notify();
    }

    /// Rebuild the editable variable rows from the selected environment plus
    /// any in-file `@name = value` declarations.
    fn rebuild_var_rows(&mut self, declared: &[(String, String)], cx: &mut Context<Self>) {
        let mut seeded: Vec<(String, String)> = Vec::new();
        if let Some(env) = self.selected_env.and_then(|i| self.environments.get(i)) {
            for (name, value, _secret) in &env.vars {
                seeded.push((name.clone(), value.clone()));
            }
        }
        // File declarations override the environment, as in IntelliJ.
        for (name, value) in declared {
            match seeded.iter_mut().find(|(n, _)| n == name) {
                Some(slot) => slot.1 = value.clone(),
                None => seeded.push((name.clone(), value.clone())),
            }
        }
        self.var_rows.clear();
        for (name, value) in seeded {
            let input = cx.new(|cx| {
                let mut i = TextInput::new(cx, "value");
                i.set_text(value, cx);
                i
            });
            cx.observe(&input, |_, _, cx| cx.notify()).detach();
            self.var_rows.push((name, input));
        }
    }

    /// What the executor actually gets: rows as edited, then captured values.
    fn effective_vars(&self, cx: &gpui::App) -> Vars {
        let mut out = Vars::new();
        for (name, input) in &self.var_rows {
            out.insert(name.clone(), input.read(cx).text().to_string());
        }
        for (name, value) in &self.captured {
            out.insert(name.clone(), value.clone());
        }
        out
    }

    /// Pull `client.global.set(...)` values out of the finished response, so the
    /// next request can use them. This is the auth-token handoff.
    fn capture_from(&mut self, pane: &Entity<RequestPane>, cx: &mut Context<Self>) {
        let (Some(res), Some(script)) = ({
            let p = pane.read(cx);
            (p.last_response().cloned(), p.script().map(str::to_string))
        }) else {
            return;
        };
        let found = vars::capture(&script, &res.body, res.status, &res.headers);
        if found.is_empty() {
            return;
        }
        for (name, value) in found {
            match self.captured.iter_mut().find(|(n, _)| *n == name) {
                Some(slot) => slot.1 = value,
                None => self.captured.push((name, value)),
            }
        }
        cx.notify();
    }

    /// Write the pane's edits back to the file they came from.
    ///
    /// ponytail: only single-file sources are writable. A Bruno collection is
    /// many files and each request would have to be written to its own path —
    /// that needs `Item::Request`'s path threaded through the flat sidebar,
    /// which is the change to make when folder editing lands.
    fn save(&mut self, cx: &mut Context<Self>) {
        let (Some(path), Some((i, pane))) = (self.source_path.clone(), self.active_request())
        else {
            return;
        };
        if path.is_dir() {
            self.error = Some("saving into a collection folder is not wired up yet".into());
            cx.notify();
            return;
        }
        let edited = pane.read(cx).edited(cx);
        self.requests[i] = edited;
        let text = if path.extension().is_some_and(|e| e == "bru") {
            bru::serialize(&self.requests[i])
        } else {
            httpfile::serialize(&self.requests)
        };
        match std::fs::write(&path, text) {
            Ok(()) => self.error = Some(format!("saved {}", path.display()).into()),
            Err(e) => self.error = Some(e.to_string().into()),
        }
        cx.notify();
    }

    fn open_dialog(&mut self, directories: bool, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: !directories,
            directories,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(path) = paths.into_iter().next()
            {
                this.update(cx, |this, cx| this.load(&path, cx)).ok();
            }
        })
        .detach();
    }

    fn rail(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut items = Vec::new();
        for (label, glyph, section) in [
            ("Requests", "folder", Section::Requests),
            ("Tools", "wrench", Section::Tools),
            ("Time", "clock", Section::Track),
        ] {
            let active = self.section == section;
            items.push(
                div()
                    .id(label)
                    .flex()
                    .items_center()
                    .gap(theme::space::XS)
                    .h(theme::control::MD)
                    .px(theme::space::SM)
                    // The accent bar is what makes "selected" legible; the
                    // unselected rows carry the same border in the rail's own
                    // colour so the labels do not shift by two pixels.
                    .border_l_2()
                    .border_color(if active { theme::ACCENT } else { theme::SUNKEN })
                    .text_size(theme::text::BODY)
                    .line_height(theme::text::BODY_LINE)
                    .map(|d| {
                        if active {
                            d.bg(theme::ACCENT_WASH)
                                .text_color(theme::ACCENT)
                                .hover(|s| s.bg(theme::FILL_STRONG))
                        } else {
                            d.text_color(theme::TEXT_SECONDARY)
                                .hover(|s| s.bg(theme::FILL).text_color(theme::TEXT))
                        }
                    })
                    .child(theme::icon_sized(glyph, theme::text::H5))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_section(section, cx))),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(theme::space::XXS)
            .w(RAIL_W)
            .h_full()
            .py(theme::space::XS)
            .bg(theme::SUNKEN)
            .border_r_1()
            .border_color(theme::BORDER)
            .children(items)
    }

    fn requests_list(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut rows = Vec::with_capacity(self.requests.len());
        let open = self.tabs.active_key().cloned();
        for (i, r) in self.requests.iter().enumerate() {
            let active = open == Some(TabKey::Request(i));
            rows.push(
                sidebar_row(active)
                    .id(i)
                    .child(method_badge(&r.method))
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_color(if active { theme::ACCENT } else { theme::TEXT })
                            .child(SharedString::from(r.name.clone())),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_request(i, cx);
                        cx.defer(save_session);
                        cx.notify();
                    })),
            );
        }
        let empty = self.requests.is_empty();
        sidebar()
            .child(theme::section_label("Collection"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(theme::space::XXS)
                    .px(theme::space::SM)
                    .pb(theme::space::XS)
                    .child(button(
                        "open-file",
                        "file",
                        "Open file…",
                        cx.listener(|this, _, _, cx| this.open_dialog(false, cx)),
                    ))
                    .child(button(
                        "open-dir",
                        "folder",
                        "Collection…",
                        cx.listener(|this, _, _, cx| this.open_dialog(true, cx)),
                    ))
                    .child(primary_button(
                        "save",
                        "save",
                        "Save",
                        cx.listener(|this, _, _, cx| this.save(cx)),
                    )),
            )
            .child(
                div()
                    .px(theme::space::SM)
                    .pb(theme::space::XS)
                    .text_size(theme::text::SMALL)
                    .line_height(theme::text::SMALL_LINE)
                    .truncate()
                    .text_color(if self.error.is_some() {
                        theme::TEXT_SECONDARY
                    } else {
                        theme::TEXT_TERTIARY
                    })
                    .child(self.error.clone().unwrap_or_else(|| self.source.clone())),
            )
            .child(theme::divider())
            .child(
                div()
                    .id("request-list")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_scroll()
                    .when(empty, |d| {
                        d.child(empty_state(
                            "folder",
                            "Nothing loaded",
                            "Open a .http file or a Bruno collection.",
                        ))
                    })
                    .children(rows),
            )
            .child(self.vars_panel(cx))
    }

    fn vars_panel(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut env_chips = Vec::with_capacity(self.environments.len());
        for (i, env) in self.environments.iter().enumerate() {
            let active = self.selected_env == Some(i);
            env_chips.push(
                div()
                    .id(SharedString::from(format!("env-{i}")))
                    .flex()
                    .items_center()
                    .h(theme::control::SM)
                    .px(theme::space::XS)
                    .rounded(theme::radius::XS)
                    .text_size(theme::text::SMALL)
                    .line_height(theme::text::SMALL_LINE)
                    .border_1()
                    .border_color(if active { theme::ACCENT } else { theme::BORDER })
                    .text_color(if active { theme::ACCENT } else { theme::TEXT_SECONDARY })
                    .when(active, |d| d.bg(theme::ACCENT_WASH))
                    .hover(|s| s.bg(theme::FILL))
                    .child(SharedString::from(env.name.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_env = Some(i);
                        this.rebuild_var_rows(&[], cx);
                        cx.notify();
                    })),
            );
        }

        let mut rows = Vec::with_capacity(self.var_rows.len());
        for (name, input) in &self.var_rows {
            rows.push(
                div()
                    .flex()
                    .gap(theme::space::XS)
                    .items_center()
                    .child(
                        div()
                            .w(VAR_NAME_W)
                            .truncate()
                            .text_size(theme::text::SMALL)
                            .line_height(theme::text::SMALL_LINE)
                            .text_color(theme::TEXT_SECONDARY)
                            .child(SharedString::from(name.clone())),
                    )
                    .child(div().flex_1().child(input.clone())),
            );
        }

        // Captured values are shown separately: they are results, not settings.
        let mut caught = Vec::with_capacity(self.captured.len());
        for (name, value) in &self.captured {
            caught.push(
                div()
                    .truncate()
                    .text_size(theme::text::SMALL)
                    .line_height(theme::text::SMALL_LINE)
                    .text_color(theme::SUCCESS)
                    .child(SharedString::from(format!("{name} = {value}"))),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap(theme::space::XXS)
            .pb(theme::space::SM)
            .px(theme::space::SM)
            .border_t_1()
            .border_color(theme::BORDER)
            .child(theme::section_label("Variables").px_0())
            .when(!env_chips.is_empty(), |d| {
                d.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(theme::space::XXS)
                        .children(env_chips),
                )
            })
            .children(rows)
            .when(self.var_rows.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(theme::text::SMALL)
                        .line_height(theme::text::SMALL_LINE)
                        .text_color(theme::TEXT_TERTIARY)
                        .child("none — add an http-client.env.json or `@name = value`"),
                )
            })
            .when(!caught.is_empty(), |d| {
                d.child(theme::section_label("Captured").px_0())
                    .children(caught)
            })
    }

    fn tools_list(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let query = self.tool_query.read(cx).text().to_string();
        let matches = rp_devtools::search(&query);
        let count = matches.len();
        let selected = self.tabs.active_key().cloned();
        // Rows are built eagerly: `cx` is a `&mut` borrow, so a lazy `map`
        // closure would let it escape.
        let mut rows = Vec::with_capacity(count);
        for t in matches {
            rows.push(self.tool_row(t, selected == Some(TabKey::Tool(t.id)), cx));
        }
        sidebar()
            .child(theme::section_label("Tools"))
            .child(
                div()
                    .px(theme::space::SM)
                    .pb(theme::space::XS)
                    .child(self.tool_query.clone()),
            )
            .child(
                div()
                    .px(theme::space::SM)
                    .pb(theme::space::XS)
                    .text_size(theme::text::SMALL)
                    .line_height(theme::text::SMALL_LINE)
                    .text_color(theme::TEXT_TERTIARY)
                    .child(SharedString::from(format!("{count} tools"))),
            )
            .child(theme::divider())
            .child(
                div()
                    .id("tool-list")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_scroll()
                    .when(count == 0, |d| {
                        d.child(empty_state(
                            "search",
                            "No tools match",
                            "Try a shorter search term.",
                        ))
                    })
                    .children(rows),
            )
    }

    fn tool_row(
        &self,
        tool: &'static Tool,
        active: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        sidebar_row(active)
            .id(tool.id)
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_color(if active { theme::ACCENT } else { theme::TEXT })
                    .child(tool.name),
            )
            // Mark the tools this build cannot provide, so the disappointment
            // happens before the click rather than after it.
            .when(unavailable(tool), |d| {
                d.child(
                    div()
                        .flex_none()
                        .px(theme::space::XXS)
                        .rounded(theme::radius::XS)
                        .bg(theme::FILL)
                        .text_size(theme::text::SMALL)
                        .line_height(theme::text::SMALL_LINE)
                        .text_color(theme::TEXT_TERTIARY)
                        .child("no UI"),
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_tool(tool, window, cx);
                cx.defer(save_session);
                cx.notify();
            }))
    }

    /// Move tab `i` into a brand-new window.
    ///
    /// gpui entities are app-global, so the view moves as-is: the same
    /// `Entity`/`AnyView` is simply rendered by the new window's `Workspace`.
    /// Nothing is rebuilt, so a running chronometer keeps running and typed
    /// input stays typed. Element ids are scoped per window and focus is
    /// per-window state, so the moved pane just needs a click to regain focus.
    fn detach_tab(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.remove(i) else {
            return;
        };
        let section = match tab.key {
            TabKey::Request(_) => Section::Requests,
            TabKey::Tool(_) => Section::Tools,
        };
        // The new window shares the request/variable state, so a detached
        // request can still be sent with the same environment. `var_rows` holds
        // entities, so both windows edit the same value editors.
        let requests = self.requests.clone();
        let source = self.source.clone();
        let source_path = self.source_path.clone();
        let environments = self.environments.clone();
        let selected_env = self.selected_env;
        let var_rows = self.var_rows.clone();
        let captured = self.captured.clone();
        let bounds = Bounds::centered(None, size(px(980.), px(700.)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |_, cx| {
                cx.new(move |cx| {
                    let mut ws = Workspace::new(cx);
                    ws.section = section;
                    ws.requests = requests;
                    ws.source = source;
                    ws.source_path = source_path;
                    ws.environments = environments;
                    ws.selected_env = selected_env;
                    ws.captured = captured;
                    for (_, input) in &var_rows {
                        cx.observe(input, |_, _, cx| cx.notify()).detach();
                    }
                    ws.var_rows = var_rows;
                    if let Content::Request(pane) = &tab.content {
                        cx.observe(pane, |this, pane, cx| this.capture_from(&pane, cx))
                            .detach();
                    }
                    ws.tabs.open(tab);
                    ws
                })
            },
        );
        if opened.is_err() {
            self.error = Some("could not open a new window".into());
        }
        cx.defer(save_session);
        cx.notify();
    }
}

/// Open a window, restoring `session` into it when there is one.
///
/// A window that fails to open is logged, not fatal: with a stored session of
/// several windows, one failure must not take the others down.
fn open_workspace(session: Option<WindowSession>, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1180.), px(760.)), cx);
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        },
        |window, cx| {
            cx.new(|cx| {
                let mut ws = Workspace::new(cx);
                if let Some(session) = session {
                    ws.restore(&session, window, cx);
                }
                ws
            })
        },
    );
    if let Err(e) = opened {
        eprintln!("request-pilot: could not open a window: {e:#}");
    }
}

/// Ant's default button: an outline, no fill until it is hovered.
fn button(
    id: &'static str,
    glyph: &'static str,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    button_of(id, glyph, label, false, on_click)
}

/// Ant's primary button: an `ACCENT` fill. One per group of controls.
fn primary_button(
    id: &'static str,
    glyph: &'static str,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    button_of(id, glyph, label, true, on_click)
}

/// The one button in the shell. Both variants are the same box — `control::MD`
/// high, 2-px corners, icon then label — so a row of mixed buttons still lines
/// up; only the fill and the hover/active pair differ.
fn button_of(
    id: &'static str,
    glyph: &'static str,
    label: &'static str,
    primary: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(theme::space::XXS)
        .h(theme::control::MD)
        .px(theme::space::XS)
        .rounded(theme::radius::XS)
        .text_size(theme::text::SMALL)
        .line_height(theme::text::SMALL_LINE)
        .map(|d| {
            if primary {
                d.bg(theme::ACCENT)
                    .text_color(theme::TEXT_ON_ACCENT)
                    .hover(|s| s.bg(theme::ACCENT_HOVER))
                    .active(|s| s.bg(theme::ACCENT_ACTIVE))
            } else {
                d.border_1()
                    .border_color(theme::BORDER)
                    .text_color(theme::TEXT_SECONDARY)
                    .hover(|s| s.border_color(theme::ACCENT).text_color(theme::ACCENT))
                    .active(|s| s.bg(theme::FILL_STRONG))
            }
        })
        .child(theme::icon_sized(glyph, theme::text::BODY))
        .child(label)
        .on_click(on_click)
}

/// A method badge: the method's own hue on a wash of itself, at a fixed width
/// so the names to its right form a column.
fn method_badge(method: &str) -> impl IntoElement + use<> {
    let color = theme::method_color(method);
    div()
        .flex_none()
        .w(METHOD_W)
        .rounded(theme::radius::XS)
        .bg(wash(color))
        .text_size(theme::text::SMALL)
        .line_height(theme::text::SMALL_LINE)
        .text_color(color)
        .text_center()
        .child(SharedString::from(method.to_ascii_uppercase()))
}

/// Any colour at the strength `ACCENT_WASH` uses.
///
/// `theme` has the accent wash as a token but no general "tint this hue"
/// helper, and the method badges need one per method — so the alpha is taken
/// from the token rather than a second set of hard-coded colours.
fn wash(color: Rgba) -> Rgba {
    Rgba {
        a: theme::ACCENT_WASH.a,
        ..color
    }
}

/// The shared geometry of a sidebar list row, so requests and tools cannot
/// drift apart. Selected is an accent bar plus an accent wash; hover is a
/// plain fill, which is visibly a different thing.
fn sidebar_row(selected: bool) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(theme::space::XS)
        .h(theme::control::MD)
        .px(theme::space::SM)
        .text_size(theme::text::BODY)
        .line_height(theme::text::BODY_LINE)
        .border_l_2()
        .border_color(if selected { theme::ACCENT } else { theme::SURFACE })
        .when(selected, |d| d.bg(theme::ACCENT_WASH))
        .hover(|s| s.bg(theme::FILL))
}

/// The sidebar column itself: both lists are the same box.
fn sidebar() -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .w(SIDEBAR_W)
        .h_full()
        .bg(theme::SURFACE)
        .border_r_1()
        .border_color(theme::BORDER)
}

/// A centred icon, a one-line heading and a quieter hint — for every "there is
/// nothing here yet" in the shell.
fn empty_state(
    glyph: &'static str,
    heading: &'static str,
    hint: &'static str,
) -> impl IntoElement {
    div()
        .flex()
        .flex_1()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(theme::space::XS)
        .p(theme::space::XL)
        .child(theme::icon_sized(glyph, theme::text::H3).text_color(theme::TEXT_DISABLED))
        .child(
            div()
                .text_size(theme::text::H5)
                .line_height(theme::text::H5_LINE)
                .text_color(theme::TEXT_SECONDARY)
                .child(heading),
        )
        .child(
            div()
                .text_size(theme::text::SMALL)
                .line_height(theme::text::SMALL_LINE)
                .text_color(theme::TEXT_TERTIARY)
                .text_center()
                .child(hint),
        )
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Variable edits are typed into their own editors, so push the current
        // set into the pane before it can be sent.
        if let Some((_, pane)) = self.active_request() {
            let vars = self.effective_vars(cx);
            pane.update(cx, |pane, _| pane.set_vars(vars));
        }
        let rail = self.rail(cx);
        // Time tracking brings its own list *and* detail pane, so it replaces the
        // whole body instead of borrowing the tab area — tabs are untouched and
        // come back exactly as they were.
        let body = if self.section == Section::Track {
            self.track.clone().into_any_element()
        } else {
            let list = match self.section {
                Section::Requests => self.requests_list(cx).into_any_element(),
                _ => self.tools_list(cx).into_any_element(),
            };
            // The active tab's view is cloned, never rebuilt: that is what keeps
            // half-typed input and running timers alive across a tab switch.
            let main = match self.tabs.active().map(|t| &t.content) {
                Some(Content::Generic(pane)) => pane.clone().into_any_element(),
                Some(Content::Custom(view)) => view.clone().into_any_element(),
                Some(Content::Request(pane)) => pane.clone().into_any_element(),
                Some(Content::Unavailable(reason)) => div()
                    .flex()
                    .flex_1()
                    .h_full()
                    .child(empty_state("alert", "No UI for this tool", *reason))
                    .into_any_element(),
                None => div()
                    .flex()
                    .flex_1()
                    .h_full()
                    .child(empty_state(
                        "file",
                        "Nothing open",
                        "Pick a request or a tool on the left.",
                    ))
                    .into_any_element(),
            };
            let strip = tabs::strip(self, cx);
            div()
                .flex()
                .flex_1()
                .h_full()
                .child(list)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .h_full()
                        .bg(theme::SURFACE)
                        .when(!self.tabs.is_empty(), |d| d.child(strip))
                        .child(div().flex().flex_1().overflow_hidden().child(main)),
                )
                .into_any_element()
        };
        div()
            .flex()
            .size_full()
            .bg(theme::LAYOUT)
            .font_family(theme::FONT_UI)
            .text_size(theme::text::BODY)
            .line_height(theme::text::BODY_LINE)
            .text_color(theme::TEXT)
            .child(rail)
            .child(body)
    }
}

/// A `Kind::Custom` tool with neither a generic form nor a bespoke pane.
fn unavailable(tool: &'static Tool) -> bool {
    tool.kind == rp_devtools::Kind::Custom && !custom::HAS_PANE.contains(&tool.id)
}

fn main() {
    // Icons are compiled in; without an asset source `svg()` renders nothing.
    Application::new().with_assets(assets::Assets).run(|cx: &mut App| {
        text_input::bind_keys(cx);
        editor::bind_keys(cx);
        // Tabs can be pulled out into their own windows; the app lives until
        // the last one is gone.
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                // Do not save here: the last snapshot still has this window in
                // it, which is what should come back. Saving now would store an
                // empty session and quitting would wipe the tabs.
                cx.quit();
            } else {
                save_session(cx);
            }
        })
        .detach();

        // Opening the database must never stop the app from starting.
        let store = match Store::open_default() {
            Ok(store) => Some(store),
            Err(e) => {
                eprintln!("request-pilot: persistence disabled: {e:#}");
                None
            }
        };
        let session = store
            .as_ref()
            .map(|s| s.load_session())
            .transpose()
            .unwrap_or_else(|e| {
                eprintln!("request-pilot: cannot read the stored session: {e:#}");
                None
            })
            .unwrap_or_default();
        cx.set_global(Persist {
            store,
            windows: Vec::new(),
        });

        if session.is_empty() {
            open_workspace(None, cx);
        } else {
            for window in session {
                open_workspace(Some(window), cx);
            }
        }
        cx.activate(true);
    });
}
