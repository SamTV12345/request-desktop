mod calendar;
// The design system. Everything about how this app looks lives in `rp-ui`.
use rp_ui::button::{Button, Tone, check_box};
use rp_ui::data::method_badge;
use rp_ui::fonts::fonts;
use rp_ui::fonts::resolve_fonts;
use rp_ui::icon::icon_tinted;
use rp_ui::input::TextInput;
use rp_ui::layout::{card, divider, empty_state, section_label, sidebar, sidebar_row};
use rp_ui::theme;
use rp_ui::tokens::{control, radius, size, space, text};
mod custom;
mod filters;
mod palette;
mod reference;
mod request_pane;
// `session` is the library target's, not the binary's, so `tests/` can use the
// same loader the app does. See `lib.rs`.
use request_pilot::session;
mod tabs;
mod tool_pane;
mod track;
mod year_strip;

use gpui::{
    App, Application, Bounds, Context, Entity, Global, IntoElement, ParentElement,
    PathPromptOptions, Pixels, Render, SharedString, Styled, WeakEntity, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, size,
};
use request_pane::RequestPane;
use rp_core::interp::Vars;
use rp_core::{Environment, Request, bru, httpfile, vars};
use rp_devtools::Tool;
use rp_store::{Store, WindowSession};
use std::path::Path;
use tabs::{Content, Restore, Tab, TabKey, Tabs};
use tool_pane::ToolPane;

/// Shell geometry the mockups fix and `theme` does not carry yet.
///
/// Every one of these belongs in `theme::size`; they are local only because
/// `theme.rs` has another owner. `size::RAIL` is 176, which is the *old* rail —
/// the design's is 104 — so using the token here would be wrong, not merely
/// untidy.
mod shell {
    use gpui::{Pixels, px};
    /// The rail. `size::RAIL` (176) contradicts the mockups.
    pub const RAIL: Pixels = px(104.);
    /// A rail entry: a 76×74 tile, not a full-width row.
    pub const TILE_W: Pixels = px(76.);
    pub const TILE_H: Pixels = px(74.);
    pub const TILE_RADIUS: Pixels = px(24.);
    /// The rail icon, a step above `text::H4`.
    pub const RAIL_ICON: Pixels = px(22.);
    /// The title bar and the app mark inside it. 44 in the redesign, down from 52:
    /// the row carries a 28-px control at most, so the rest was air.
    pub const TITLEBAR: Pixels = px(44.);
    pub const MARK: Pixels = px(22.);
    /// The title row's right inset. The design's own 6, with no step on the 4.4
    /// scale near it.
    pub const TITLE_INSET_R: Pixels = px(6.);
    /// The 1×20 rule between the mark and what follows it.
    pub const RULE: Pixels = px(20.);
    /// A title-bar icon button: 40×32, pill. The gear.
    pub const CONTROL_W: Pixels = px(40.);
    pub const CONTROL_H: Pixels = px(32.);
}

/// The name in the title bar, from the mockups.
/// The chord the "jump to anything" bar advertises. One definition, so the label
/// it shows and the binding it will get cannot drift apart.
const PALETTE: rp_ui::Chord = rp_ui::Chord::primary("K");

const APP_NAME: &str = "Toolbench";

/// The maker's mark at the right end of the title bar, where the mockups had
/// window controls the OS already draws.
const SIGNATURE: &str = "Designed by Samuel, excellence and innovation";

/// The kv key holding the ground the user picked.
const THEME_KEY: &str = "theme.dark";

/// Read the stored ground.
///
/// Dark when nothing was stored, when the value is junk, or when there is no
/// database at all — that is what the app shipped with, and a theme is never
/// worth failing a launch over.
fn stored_dark(store: Option<&Store>) -> bool {
    let Some(store) = store else { return true };
    match store.get(THEME_KEY) {
        Ok(value) => value.as_deref() != Some("light"),
        Err(e) => {
            eprintln!("request-pilot: cannot read the stored theme: {e:#}");
            true
        }
    }
}

fn save_dark(store: Option<&Store>, dark: bool) {
    if let Some(store) = store
        && let Err(e) = store.set(THEME_KEY, if dark { "dark" } else { "light" })
    {
        eprintln!("request-pilot: cannot store the theme: {e:#}");
    }
}

/// Switch the ground and remember it.
///
/// Every view reads `theme::theme(cx)` in `render`, so one refresh repaints all
/// windows — including detached ones, which is the point of a global.
fn set_ground(dark: bool, cx: &mut App) {
    theme::set_theme(dark, cx);
    save_dark(
        cx.try_global::<Persist>().and_then(|p| p.store.as_ref()),
        dark,
    );
    cx.refresh_windows();
}

/// The three faces the design names, resolved against what is installed.
///
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
            let paths: Vec<std::path::PathBuf> =
                ws.sources.iter().map(|s| s.path.clone()).collect();
            sessions.push(tabs::snapshot(&ws.request_tabs, &ws.tool_tabs, &paths));
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

/// The kv keys for the agent-facing HTTP server.
const MCP_ENABLED_KEY: &str = "mcp.enabled";
const MCP_PORT_KEY: &str = "mcp.port";
/// Whether the server listens on every interface instead of loopback only.
///
/// Needed for a client in a **container**: Claude Code in a Docker sandbox has its
/// own network namespace, so `127.0.0.1` there is the container, not the host. It
/// reaches the host as `host.docker.internal`, and that connection arrives on the
/// host's bridge interface — where a loopback-only listener is not.
const MCP_BIND_ALL_KEY: &str = "mcp.bind_all";

/// The embedded MCP server, off until the user turns it on.
///
/// A `Global` for the same reason as [`Persist`]: there is exactly one listener,
/// and every window has to show the same state.
///
/// The server opens its **own** connection to the same database file — see
/// `rp_mcp::http` for why it cannot share this process's `Store`.
struct Mcp {
    /// The port to bind. After a successful start this is the port actually bound.
    port: u16,
    bind: rp_mcp::http::Bind,
    /// Which database the server should open. `None` is the app's own; the tests
    /// point it at a temporary file so they never touch the real one.
    db: Option<std::path::PathBuf>,
    server: Option<rp_mcp::http::Handle>,
    error: Option<SharedString>,
}

impl Global for Mcp {}

impl Mcp {
    fn new(port: u16, bind: rp_mcp::http::Bind) -> Self {
        Self {
            port,
            bind,
            db: None,
            server: None,
            error: None,
        }
    }

    /// Start, unless it is already running — a second click must not leave a
    /// second listener behind.
    fn start(&mut self) {
        if self.server.is_some() {
            return;
        }
        match rp_mcp::http::start(self.port, self.bind, self.db.clone()) {
            Ok(handle) => {
                self.port = handle.port;
                self.server = Some(handle);
                self.error = None;
            }
            // A taken port or an unopenable database is shown, not fatal.
            Err(e) => self.error = Some(format!("{e:#}").into()),
        }
    }

    /// Stop. Inert when nothing is running. Dropping the handle blocks until the
    /// listener is gone, which is what actually frees the port.
    fn stop(&mut self) {
        self.server = None;
        self.error = None;
    }

    fn running(&self) -> bool {
        self.server.is_some()
    }

    /// Change the bind, restarting the listener if one is up — the address is
    /// fixed when the socket is created, so there is no way to apply it in place.
    fn set_bind(&mut self, bind: rp_mcp::http::Bind) {
        if self.bind == bind {
            return;
        }
        let was_running = self.running();
        self.stop();
        self.bind = bind;
        if was_running {
            self.start();
        }
    }
}

/// Was the server told to listen beyond loopback?
fn stored_bind(store: Option<&Store>) -> rp_mcp::http::Bind {
    match store
        .and_then(|s| s.get(MCP_BIND_ALL_KEY).ok().flatten())
        .as_deref()
    {
        Some("1") => rp_mcp::http::Bind::Any,
        // Anything else, including a value that no longer parses: loopback. This
        // setting opens a socket to the network, so it fails closed.
        _ => rp_mcp::http::Bind::Loopback,
    }
}

/// Flip the bind and remember it.
fn toggle_mcp_bind(cx: &mut App) {
    let next = cx.global_mut::<Mcp>().bind;
    let next = match next {
        rp_mcp::http::Bind::Loopback => rp_mcp::http::Bind::Any,
        rp_mcp::http::Bind::Any => rp_mcp::http::Bind::Loopback,
    };
    cx.global_mut::<Mcp>().set_bind(next);
    if let Some(store) = cx.try_global::<Persist>().and_then(|p| p.store.as_ref())
        && let Err(e) = store.set(
            MCP_BIND_ALL_KEY,
            if next == rp_mcp::http::Bind::Any {
                "1"
            } else {
                "0"
            },
        )
    {
        eprintln!("request-pilot: could not remember the MCP bind: {e:#}");
    }
    cx.refresh_windows();
}

/// Was the server on when the app last closed?
fn mcp_enabled(store: Option<&Store>) -> bool {
    // Off by default: this one opens a socket, so it waits to be asked.
    store
        .and_then(|s| s.get(MCP_ENABLED_KEY).ok().flatten())
        .as_deref()
        == Some("1")
}

/// The stored port, or the default when there is nothing usable stored.
fn stored_port(store: Option<&Store>) -> u16 {
    store
        .and_then(|s| s.get(MCP_PORT_KEY).ok().flatten())
        .and_then(|v| v.trim().parse().ok())
        .filter(|p| *p != 0)
        .unwrap_or(rp_mcp::http::DEFAULT_PORT)
}

/// Flip the server, and remember the choice for the next launch.
fn toggle_mcp(cx: &mut App) {
    let (running, port) = cx.update_global::<Mcp, _>(|mcp, _| {
        if mcp.running() {
            mcp.stop();
        } else {
            mcp.start();
        }
        (mcp.running(), mcp.port)
    });
    if let Some(store) = cx.try_global::<Persist>().and_then(|p| p.store.as_ref()) {
        for (key, value) in [
            (MCP_ENABLED_KEY, if running { "1" } else { "0" }),
            (MCP_PORT_KEY, &port.to_string()),
        ] {
            if let Err(e) = store.set(key, value) {
                eprintln!("request-pilot: cannot store {key}: {e:#}");
            }
        }
    }
    cx.refresh_windows();
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Section {
    Requests,
    Tools,
    Track,
}

/// One open request file, or one Bruno collection.
///
/// Several of these are open at once, which is why the requests, the declarations
/// and the reload stamp live here and not on the workspace: `@name = value`
/// belongs to a file, so two files are two sets of variables, and a file that
/// changed on disk is one file to reload.
#[derive(Clone)]
struct Source {
    path: std::path::PathBuf,
    /// File name, for the group header. Kept rather than derived per frame.
    label: SharedString,
    requests: Vec<Request>,
    declared: Vec<(String, String)>,
    /// Why this source did not load, if it did not. Held per source so one broken
    /// file does not take the others' requests off the screen with it.
    error: Option<SharedString>,
    /// mtime as of the last load. `None` for a path that cannot be stat'ed, which
    /// the watcher reads as "no news" rather than reloading forever.
    stamp: Option<std::time::SystemTime>,
    collapsed: bool,
}

impl Source {
    fn new(path: std::path::PathBuf) -> Self {
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let mut source = Self {
            label: label.into(),
            path,
            requests: Vec::new(),
            declared: Vec::new(),
            error: None,
            stamp: None,
            collapsed: false,
        };
        source.reload();
        source
    }

    /// Re-read the file. Keeps the old requests on a parse failure — a file being
    /// edited elsewhere is briefly invalid all the time, and blanking the list on
    /// every intermediate save would be worse than showing slightly stale entries
    /// with the reason next to them.
    fn reload(&mut self) {
        self.stamp = session::stamp(&self.path);
        match session::load(&self.path) {
            Ok(loaded) => {
                self.requests = loaded.requests;
                self.declared = loaded.declared;
                self.error = None;
            }
            Err(e) => self.error = Some(format!("{e:#}").into()),
        }
    }

    /// Whether the file on disk is newer than what is loaded.
    fn is_stale(&self) -> bool {
        match (session::stamp(&self.path), self.stamp) {
            (Some(now), Some(then)) => now > then,
            // A file that has just appeared where there was none is news too.
            (Some(_), None) => true,
            _ => false,
        }
    }
}

struct Workspace {
    section: Section,
    /// Every open request file, in the order they were opened.
    sources: Vec<Source>,
    /// Filters the request list across **every** source at once — with a dozen
    /// files open, scrolling to find one request is not a plan.
    request_query: Entity<TextInput>,
    error: Option<SharedString>,
    /// Environments found next to the opened files, merged by name: a directory's
    /// `http-client.env.json` is shared by every `.http` file in it, so two files
    /// from one directory offer one set of environments, not two identical ones.
    environments: Vec<Environment>,
    selected_env: Option<usize>,
    /// Editable variable rows: name plus a live value editor.
    var_rows: Vec<(String, Entity<TextInput>)>,
    /// Values captured from earlier responses via `client.global.set`. These
    /// win over the environment, which is how an auth token flows forward.
    captured: Vec<(String, String)>,
    // Tools
    tool_query: Entity<TextInput>,
    /// The command palette's query field. Built once and reused: it owns the focus
    /// the palette's key bindings hang off, and rebuilding it per open would mean
    /// re-focusing something the window has not laid out yet.
    palette_query: Entity<TextInput>,
    /// Whether the palette is open, and which row is on. The row list itself is
    /// derived per frame from the query — it is a function of state, not state.
    palette_open: bool,
    palette_selected: usize,
    /// The palette list's scroll offset. Held here rather than in the component,
    /// which is rebuilt every frame: walking the selection past the fold has to
    /// move the list, and only a handle that outlives one frame can do that.
    palette_scroll: gpui::ScrollHandle,
    /// Time tracking. Owns its own list and detail pane, and polls the database
    /// only while its section is on screen.
    track: Entity<track::Track>,
    /// Everything open in this window, **one list per section**. A tool tab has
    /// no business in the Requests strip, and requests need their own strip so
    /// several `.http` requests can be open at once; switching the rail switches
    /// which of these is shown. Each tab owns its view, so neither switching
    /// tabs nor switching sections rebuilds a pane.
    request_tabs: Tabs,
    tool_tabs: Tabs,
    /// The open tab context menu: which tab it belongs to, and where in window
    /// coordinates it was opened. The index is into the strip of whichever
    /// section is current, which is why `set_section` closes the menu.
    tab_menu: Option<(usize, gpui::Point<gpui::Pixels>)>,
    /// The open overflow list: the keys of the tabs the strip could not fit, and
    /// where it was opened. Keys rather than indices, so a list that changed
    /// underneath drops the rows that no longer resolve instead of activating
    /// whatever now sits at that position.
    tab_overflow: Option<(Vec<SharedString>, gpui::Point<gpui::Pixels>)>,
    /// Width the tab run reported during the last frame, so the strip can tell how
    /// many tabs fit. Written by the strip's measuring `canvas` — see
    /// `rp_ui::TabStrip::on_measure`.
    tab_strip_width: Pixels,
    /// The tab menu's focus, so Escape reaches it. Per window, like all focus.
    menu_focus: gpui::FocusHandle,
    /// Width of the list column, dragged by the handle beside it.
    ///
    /// A request named "Redirect bewusst nicht folgen" does not fit 264 px and was
    /// cut off mid-word, so the width had to stop being a constant. Kept in `kv`
    /// rather than per window: it is a preference about this app, not about one
    /// window, and a new window opening at someone else's width reads as a bug.
    sidebar_width: Pixels,
    /// A drag in progress: where it started, and the width it started from. Both,
    /// because tracking the delta from the *pointer* is what keeps the handle from
    /// jumping when the grab was not exactly on its centre.
    resizing: Option<(Pixels, Pixels)>,
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
        let request_query = cx.new(|cx| TextInput::new(cx, "Search all open files…"));
        cx.observe(&request_query, |_, _, cx| cx.notify()).detach();
        let palette_query = cx.new(|cx| TextInput::new(cx, "Jump to anything…"));
        // Typing narrows the list, so a shorter list must not leave the selection
        // pointing past its end — that is how Enter opens the wrong thing.
        cx.observe(&palette_query, |this, _, cx| {
            this.palette_selected = 0;
            // And back to the top with it, or a narrowed list shows its first row
            // scrolled out of sight.
            this.palette_scroll.scroll_to_item(0);
            cx.notify();
        })
        .detach();
        // The watcher. One timer for the window, not one per file: it only stats
        // the open paths, which is a handful of syscalls a second and needs no
        // platform notification API or extra dependency to be right.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(WATCH_INTERVAL_MS))
                    .await;
                if this.update(cx, |this, cx| this.reload_changed(cx)).is_err() {
                    // The window is gone.
                    return;
                }
            }
        })
        .detach();
        Self {
            section: Section::Requests,
            sources: Vec::new(),
            request_query,
            error: None,
            environments: Vec::new(),
            selected_env: None,
            var_rows: Vec::new(),
            captured: Vec::new(),
            tool_query,
            palette_query,
            palette_open: false,
            palette_selected: 0,
            palette_scroll: gpui::ScrollHandle::new(),
            track,
            request_tabs: Tabs::default(),
            tool_tabs: Tabs::default(),
            tab_menu: None,
            tab_overflow: None,
            tab_strip_width: px(0.),
            menu_focus: cx.focus_handle(),
            sidebar_width: stored_sidebar_width(cx),
            resizing: None,
        }
    }

    /// Follow a drag of the sidebar handle.
    fn resize_sidebar(&mut self, to: Pixels, cx: &mut Context<Self>) {
        let Some((from, started_at)) = self.resizing else {
            return;
        };
        self.sidebar_width = clamp_sidebar(started_at + (to - from));
        cx.notify();
    }

    /// Finish the drag and remember the width.
    ///
    /// Written once here rather than on every mouse move: a drag is a few hundred
    /// events and each would be its own transaction.
    fn end_sidebar_drag(&mut self, cx: &mut Context<Self>) {
        if self.resizing.take().is_none() {
            return;
        }
        let width = self.sidebar_width;
        if let Some(store) = cx.try_global::<Persist>().and_then(|p| p.store.as_ref())
            && let Err(e) = store.set(SIDEBAR_WIDTH_KEY, &f32::from(width).to_string())
        {
            // Not worth interrupting the user over: the width is right for this
            // session either way, it just will not come back.
            eprintln!("request-pilot: could not remember the sidebar width: {e:#}");
        }
        // Every other window shows the same list, so they follow along.
        cx.refresh_windows();
        cx.notify();
    }

    /// The handle between the list and the pane beside it.
    ///
    /// Its own element rather than a border on the sidebar: a 1-px border is not
    /// something a pointer can reliably hit, and the grab area has to be wider
    /// than the line it appears to be.
    fn sidebar_handle(&self, t: theme::Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let active = self.resizing.is_some();
        div()
            .id("sidebar-handle")
            .flex_none()
            .w(space::S2)
            .h_full()
            .cursor(gpui::CursorStyle::ResizeLeftRight)
            .when(active, |d| d.bg(t.accent))
            .when(!active, |d| d.hover(|s| s.bg(t.accent_wash_strong())))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, ev: &gpui::MouseDownEvent, _, cx| {
                    this.resizing = Some((ev.position.x, this.sidebar_width));
                    cx.notify();
                }),
            )
    }

    /// Switch the rail. The single place the section changes by hand, which is
    /// why the tracker's polling is started and stopped here.
    fn set_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        // The menu holds an index into the strip that is about to be replaced, and
        // the overflow list names tabs of the section being left.
        self.tab_menu = None;
        self.tab_overflow = None;
        self.track
            .update(cx, |t, cx| t.set_visible(section == Section::Track, cx));
        cx.notify();
    }

    /// The strip the current section shows.
    ///
    /// `Track` brings its own body and renders no strip at all, so what it maps
    /// to is never drawn; it shares the Requests list rather than needing a third
    /// empty one, or an `Option` every caller would have to unwrap.
    fn tabs(&self) -> &Tabs {
        match self.section {
            Section::Tools => &self.tool_tabs,
            Section::Requests | Section::Track => &self.request_tabs,
        }
    }

    fn tabs_mut(&mut self) -> &mut Tabs {
        match self.section {
            Section::Tools => &mut self.tool_tabs,
            Section::Requests | Section::Track => &mut self.request_tabs,
        }
    }

    /// The active tab, when it is a request: its file, its position in it, and the
    /// pane showing it.
    fn active_request(&self) -> Option<(std::path::PathBuf, usize, Entity<RequestPane>)> {
        match self.request_tabs.active() {
            Some(Tab {
                key: TabKey::Request { source, index },
                content: Content::Request(pane),
                ..
            }) => Some((source.clone(), *index, pane.clone())),
            _ => None,
        }
    }

    fn source_at(&self, path: &Path) -> Option<&Source> {
        self.sources.iter().find(|s| s.path == path)
    }

    /// Open `path` as a source, or bring the one already open forward.
    ///
    /// A directory is expanded: a folder of `.http` files becomes one source per
    /// file, because that is what they are — see [`session::http_files_in`]. A
    /// Bruno collection stays one source with its tree flattened.
    fn add_source(&mut self, path: &Path, cx: &mut Context<Self>) {
        let mut paths = vec![path.to_path_buf()];
        if path.is_dir() {
            let http = session::http_files_in(path);
            if !http.is_empty() {
                paths = http;
            }
        }
        let mut added = 0usize;
        for path in paths {
            if let Some(at) = self.sources.iter().position(|s| s.path == path) {
                // Already open: re-read it rather than opening a second copy, since
                // asking for a file again is how someone asks for it afresh.
                self.sources[at].reload();
                continue;
            }
            self.sources.push(Source::new(path));
            added += 1;
        }
        let _ = added;
        self.after_sources_changed(cx);
    }

    /// Close a source and every tab that pointed into it.
    fn close_source(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.sources.retain(|s| s.path != path);
        let gone = path.to_path_buf();
        self.request_tabs
            .retain(|k| !matches!(k, TabKey::Request { source, .. } if *source == gone));
        // The menu holds an index into the strip that just shrank.
        self.tab_menu = None;
        self.tab_overflow = None;
        self.after_sources_changed(cx);
    }

    /// Environments, variable rows and the status line, after the set of open
    /// files changed. One place, so no caller can update half of it.
    fn after_sources_changed(&mut self, cx: &mut Context<Self>) {
        // Environments come from `http-client.env.json` next to the files, so two
        // files in one directory yield the same set — merge by name rather than
        // listing `dev` twice.
        let mut merged: Vec<Environment> = Vec::new();
        for source in &self.sources {
            for env in session::load(&source.path)
                .map(|l| l.environments)
                .unwrap_or_default()
            {
                if !merged.iter().any(|e| e.name == env.name) {
                    merged.push(env);
                }
            }
        }
        // Keep the selection on the *same environment*, not the same index: the
        // list just changed under it.
        let was = self
            .selected_env
            .and_then(|i| self.environments.get(i))
            .map(|e| e.name.clone());
        self.environments = merged;
        self.selected_env = was
            .and_then(|name| self.environments.iter().position(|e| e.name == name))
            .or((!self.environments.is_empty()).then_some(0));

        self.error = self
            .sources
            .iter()
            .find_map(|s| s.error.clone())
            .or_else(|| {
                self.sources
                    .is_empty()
                    .then(|| SharedString::from("nothing open"))
            });
        self.rebuild_var_rows(cx);
        cx.defer(save_session);
        cx.notify();
    }

    /// Re-read whatever changed on disk. Driven by the timer in `new`.
    fn reload_changed(&mut self, cx: &mut Context<Self>) {
        let stale: Vec<std::path::PathBuf> = self
            .sources
            .iter()
            .filter(|s| s.is_stale())
            .map(|s| s.path.clone())
            .collect();
        if stale.is_empty() {
            return;
        }
        for path in &stale {
            if let Some(source) = self.sources.iter_mut().find(|s| s.path == *path) {
                source.reload();
            }
        }
        // A reloaded file may have fewer requests than it had, so a tab pointing
        // past the end has to go — otherwise it would render an empty pane and
        // send nothing. Tabs that still resolve are left alone, which is what
        // keeps a half-typed body alive across someone else's save.
        let counts: Vec<(std::path::PathBuf, usize)> = self
            .sources
            .iter()
            .map(|s| (s.path.clone(), s.requests.len()))
            .collect();
        self.request_tabs.retain(|k| match k {
            TabKey::Request { source, index } => counts
                .iter()
                .find(|(p, _)| p == source)
                .is_some_and(|(_, n)| index < n),
            _ => true,
        });
        self.after_sources_changed(cx);
    }

    /// Open request `i` from `self.requests` in a tab, or activate the tab it is
    /// already in. Shared by the sidebar and by session restore.
    fn open_request(&mut self, path: &Path, i: usize, cx: &mut Context<Self>) {
        let key = TabKey::Request {
            source: path.to_path_buf(),
            index: i,
        };
        // Already open here? Activate it, the way Zed does, instead of building
        // a second copy.
        if let Some(at) = self.request_tabs.find(&key) {
            self.request_tabs.activate(at);
            return;
        }
        let Some(req) = self
            .source_at(path)
            .and_then(|s| s.requests.get(i))
            .cloned()
        else {
            return;
        };
        let title = SharedString::from(req.name.clone());
        let vars = self.effective_vars(path, cx);
        // The directory the file came from, so a `>> file` redirect in it resolves
        // where the author meant.
        let base_dir = if path.is_dir() {
            Some(path.to_path_buf())
        } else {
            path.parent().map(Path::to_path_buf)
        };
        let pane = cx.new(|cx| RequestPane::new(req, vars, base_dir, cx));
        // A finished response may carry variables for the next request, so watch
        // the pane for them.
        cx.observe(&pane, |this, pane, cx| this.capture_from(&pane, cx))
            .detach();
        self.request_tabs.open(Tab {
            key,
            title,
            content: Content::Request(pane),
            pinned: false,
        });
    }

    /// Open the variables tab, or activate it if it is already open.
    fn open_vars(&mut self, cx: &mut Context<Self>) {
        self.request_tabs.open(Tab {
            key: TabKey::Vars,
            title: "Variables".into(),
            content: Content::Vars,
            pinned: false,
        });
        cx.notify();
    }

    /// Same for a tool: the bespoke pane if there is one, otherwise the generic
    /// form, otherwise the "no UI" marker and its reason.
    fn open_tool(&mut self, tool: &'static Tool, window: &mut Window, cx: &mut Context<Self>) {
        let key = TabKey::Tool(tool.id);
        if let Some(at) = self.tool_tabs.find(&key) {
            self.tool_tabs.activate(at);
            return;
        }
        let content = match custom::pane(tool.id, window, cx) {
            Some(view) => Content::Custom(view),
            None if unavailable(tool) => Content::Unavailable(custom::unavailable_reason(tool.id)),
            None => Content::Generic(cx.new(|cx| ToolPane::new(tool, window, cx))),
        };
        self.tool_tabs.open(Tab {
            key,
            title: tool.name.into(),
            content,
            pinned: false,
        });
    }

    // -- the command palette --

    /// Every row the palette currently offers, for the query that is typed.
    ///
    /// Derived per frame rather than cached: the sources change under a file
    /// watcher, and a cache that misses that offers rows that no longer exist.
    fn palette_hits(&self, cx: &App) -> Vec<palette::Hit> {
        let query = self.palette_query.read(cx).text().to_string();
        let mut candidates = Vec::new();
        for source in &self.sources {
            for (index, r) in source.requests.iter().enumerate() {
                candidates.push(palette::Candidate {
                    source: &source.path,
                    file: &source.label,
                    index,
                    name: &r.name,
                    method: &r.method,
                    url: &r.url,
                });
            }
        }
        let envs: Vec<&str> = self.environments.iter().map(|e| e.name.as_str()).collect();
        palette::hits(&query, &candidates, &envs)
    }

    fn toggle_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette_open {
            self.close_palette(window, cx);
            return;
        }
        self.palette_open = true;
        self.palette_selected = 0;
        // Cleared on open: re-opening onto the last search means a list the user
        // has already dealt with, and clearing it by hand is three keys.
        self.palette_query.update(cx, |q, cx| q.set_text("", cx));
        self.palette_scroll.scroll_to_item(0);
        window.focus(&gpui::Focusable::focus_handle(&self.palette_query, cx));
        cx.notify();
    }

    /// Close it, and give focus back. Without the blur, focus stays on a field
    /// that is no longer drawn and the next keystroke goes nowhere visible.
    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.palette_open {
            return;
        }
        self.palette_open = false;
        window.blur();
        cx.notify();
    }

    fn move_palette(&mut self, down: bool, cx: &mut Context<Self>) {
        let rows: Vec<rp_ui::palette::Row> =
            self.palette_hits(cx).into_iter().map(|h| h.row).collect();
        self.palette_selected =
            rp_ui::palette::step_selection(self.palette_selected, rows.len(), down);
        // Here and not in the palette's `render`: a wheel scroll also causes a
        // render, and asking to scroll to the selection there would snap the list
        // back on every notch. Only a *move* should move the list.
        self.palette_scroll
            .scroll_to_item(rp_ui::palette::child_index(&rows, self.palette_selected));
        cx.notify();
    }

    /// Take row `i`: close the palette, then do what the row said it would.
    ///
    /// The index is re-clamped against the list as it is *now* — the row list is
    /// rebuilt per frame, so an index from an older, longer one would otherwise
    /// open whatever has since moved into that slot.
    fn choose_palette(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let hits = self.palette_hits(cx);
        let Some(choice) = hits
            .get(rp_ui::palette::clamp_selection(i, hits.len()))
            .map(|h| h.choice.clone())
        else {
            // Nothing matched. Enter on an empty list leaves the palette open,
            // because closing it would look like something had happened.
            return;
        };
        self.close_palette(window, cx);
        match choice {
            // Switching section too: a request opened while Tools is on screen
            // would go into a strip nobody is looking at.
            palette::Choice::Request { source, index } => {
                self.set_section(Section::Requests, cx);
                self.open_request(&source, index, cx);
            }
            palette::Choice::Tool(id) => {
                if let Some(tool) = rp_devtools::find(id) {
                    self.set_section(Section::Tools, cx);
                    self.open_tool(tool, window, cx);
                }
            }
            palette::Choice::Section(section) => self.set_section(section, cx),
            // Rebuilding the rows is what actually applies it — the fields the
            // Variables view edits are built from the active environment.
            palette::Choice::Environment(i) => {
                if i < self.environments.len() {
                    self.selected_env = Some(i);
                    self.rebuild_var_rows(cx);
                }
            }
            palette::Choice::Vars => {
                self.set_section(Section::Requests, cx);
                self.open_vars(cx);
            }
        }
        cx.notify();
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
        let plan = tabs::plan(session, |path| {
            session::load(path).ok().map(|l| l.requests.len())
        });
        // Every file the plan could load becomes a source, in the order its tabs
        // first named it.
        for path in &plan.sources {
            if !self.sources.iter().any(|s| s.path == *path) {
                self.sources.push(Source::new(path.clone()));
            }
        }
        self.after_sources_changed(cx);
        // Each entry knows its section, and `open_tool`/`open_request` already
        // write into the matching list.
        let mut active = Vec::new();
        for restore in &plan.tabs {
            let list = match restore {
                Restore::Tool { id, .. } => {
                    // Already resolved by `plan`, but never unwrap on stored data.
                    match rp_devtools::find(id) {
                        Some(tool) => {
                            self.open_tool(tool, window, cx);
                            &mut self.tool_tabs
                        }
                        None => continue,
                    }
                }
                Restore::Request { path, index, .. } => {
                    self.open_request(&path.clone(), *index, cx);
                    &mut self.request_tabs
                }
                Restore::Vars { .. } => {
                    self.open_vars(cx);
                    &mut self.request_tabs
                }
            };
            let Some(at) = list.active_index() else {
                continue;
            };
            // Pin what was pinned, on the tab that was just opened. Applied here
            // rather than in a second pass so a tab that failed to open cannot
            // shift the pins onto its neighbours.
            if restore.pinned() {
                list.set_pinned(at, true);
            }
            // Remembered by key rather than index, because the pins still to come
            // reorder the list under it.
            if restore.active()
                && let Some(key) = list.active_key()
            {
                active.push(key.clone());
            }
        }
        // Last, because `set_pinned` reorders: the stored order is already
        // pinned-first so nothing moves, but activating after the pins are in
        // place means that stays true even if it ever is not.
        for key in active {
            let list = match key {
                TabKey::Tool(_) => &mut self.tool_tabs,
                TabKey::Request { .. } | TabKey::Vars => &mut self.request_tabs,
            };
            if let Some(at) = list.find(&key) {
                list.activate(at);
            }
        }
        // Open on the section the active tab belonged to, so a restored window
        // shows what the user was last looking at. A note has to be readable, and
        // the status line lives in the requests sidebar.
        self.section = match (&plan.note, plan.tabs.iter().find(|r| r.active())) {
            (None, Some(Restore::Tool { .. })) => Section::Tools,
            _ => Section::Requests,
        };
        // A note from the plan is about a file that is *gone*, which outranks the
        // per-source status `after_sources_changed` just worked out.
        if let Some(note) = plan.note {
            self.error = Some(note.into());
        }
    }

    /// Close everything and open `path` — what "Open file…" does.
    ///
    /// The additive door is [`Self::add_source`]; this one exists because opening a
    /// file from an empty window and adding a fifth one to four are different
    /// intentions, and the dialog offers both.
    fn load(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.sources.clear();
        self.request_tabs.close_all();
        // The menu points at an index into the strip that just emptied.
        self.tab_menu = None;
        self.tab_overflow = None;
        self.captured.clear();
        self.add_source(path, cx);
    }

    /// Rebuild the editable variable rows from the selected environment plus
    /// any in-file `@name = value` declarations.
    /// Rebuild the editable rows from the selected environment plus the loaded
    /// file's declarations. Takes no argument: the declarations live on `self`, so
    /// a caller cannot forget them the way the environment chip used to.
    fn rebuild_var_rows(&mut self, cx: &mut Context<Self>) {
        // Every open file's declarations, in file order, so the rows show what is
        // actually in play. A name in two files appears once with the later file's
        // value; sending still uses the request's own file — see `effective_vars`.
        let declared: Vec<(String, String)> = self
            .sources
            .iter()
            .flat_map(|s| s.declared.iter().cloned())
            .collect();
        let mut seeded: Vec<(String, String)> = Vec::new();
        if let Some(env) = self.selected_env.and_then(|i| self.environments.get(i)) {
            for (name, value, _secret) in &env.vars {
                seeded.push((name.clone(), value.clone()));
            }
        }
        // File declarations override the environment, as in IntelliJ.
        for (name, value) in &declared {
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
    /// The variables a request in `path` resolves against.
    ///
    /// Its **own** file's declarations, not every open file's: `@baseUrl` in one
    /// file has nothing to do with `@baseUrl` in the one next to it, and letting
    /// them collide would make a request's behaviour depend on which other files
    /// happen to be open. The edited rows and the captured values are shared,
    /// because the environment is shared and a captured token is the point.
    fn effective_vars(&self, path: &Path, cx: &gpui::App) -> Vars {
        let mut out = Vars::new();
        for (name, input) in &self.var_rows {
            out.insert(name.clone(), input.read(cx).text().to_string());
        }
        if let Some(source) = self.source_at(path) {
            for (name, value) in &source.declared {
                out.insert(name.clone(), value.clone());
            }
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
        let Some((path, i, pane)) = self.active_request() else {
            return;
        };
        if path.is_dir() {
            self.error = Some("saving into a collection folder is not wired up yet".into());
            cx.notify();
            return;
        }
        let edited = pane.read(cx).edited(cx);
        let Some(source) = self.sources.iter_mut().find(|s| s.path == path) else {
            return;
        };
        if i >= source.requests.len() {
            return;
        }
        source.requests[i] = edited;
        let text = if path.extension().is_some_and(|e| e == "bru") {
            bru::serialize(&source.requests[i])
        } else {
            httpfile::serialize(&source.requests)
        };
        match std::fs::write(&path, text) {
            Ok(()) => self.error = Some(format!("saved {}", path.display()).into()),
            Err(e) => self.error = Some(e.to_string().into()),
        }
        cx.notify();
    }

    /// The file dialog. `add` keeps what is already open and appends; without it
    /// the picked path replaces everything.
    ///
    /// Multi-select is on for the additive case only: picking three files to
    /// *replace* one would have to silently drop two of them, and a dialog that
    /// ignores two thirds of what you chose is worse than one that cannot offer it.
    fn open_dialog(&mut self, directories: bool, add: bool, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: !directories,
            directories,
            multiple: add,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && !paths.is_empty()
            {
                this.update(cx, |this, cx| {
                    for (n, path) in paths.iter().enumerate() {
                        // The first path replaces when this is not an add; the rest
                        // always append, or they would each undo the one before.
                        if n == 0 && !add {
                            this.load(path, cx);
                        } else {
                            this.add_source(path, cx);
                        }
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    fn rail(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = theme::theme(cx);
        let mut items = Vec::new();
        // ponytail: `folder` stands in for the mockup's arrow-into-bar glyph.
        // Adding an icon means adding an asset, and `assets/` has another owner.
        for (label, glyph, section) in [
            ("Requests", "folder", Section::Requests),
            ("Tools", "wrench", Section::Tools),
            ("Time", "clock", Section::Track),
        ] {
            let active = self.section == section;
            // Selected is a solid accent tile with ground-coloured ink — not a
            // wash and not a left bar. Unselected carries no fill at all and is
            // held back by opacity, which is how the mockups quiet it.
            let ink = if active { t.on_accent() } else { t.text };
            items.push(
                div()
                    .id(label)
                    .flex()
                    .flex_col()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .gap(space::S1)
                    .w(shell::TILE_W)
                    .h(shell::TILE_H)
                    .rounded(shell::TILE_RADIUS)
                    .text_size(text::TINY)
                    .text_color(ink)
                    .map(|d| {
                        if active {
                            d.bg(t.accent)
                        } else {
                            d.opacity(0.65).hover(|s| s.bg(t.hover()))
                        }
                    })
                    // svg does not inherit the container's text colour, so the
                    // glyph is tinted on the element itself or it paints nothing.
                    .child(icon_tinted(glyph, shell::RAIL_ICON, ink))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_section(section, cx))),
            );
        }
        div()
            .flex()
            .flex_col()
            .flex_none()
            .items_center()
            .gap(space::S1)
            .w(shell::RAIL)
            .h_full()
            .py(space::S3)
            .bg(t.surface)
            .children(items)
    }

    /// The title bar the mockups put above everything: the app mark, the name,
    /// the signature and the gear.
    ///
    /// Dragging it moves the window, so it behaves like the bar it replaces even
    /// though the OS frame is still the one being dragged.
    fn title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = theme::theme(cx);
        // ponytail: the mockups also put a global running-timer pill here —
        // accent-200 fill, accent-800 ink, a 7-px accent-600 dot, the task name
        // and the elapsed time in mono 700 — visible on *every* screen while a
        // timer runs. It is not built: the state lives in `track::Track::running`
        // with no accessor, and `Track` only polls while its own section is
        // visible, so the pill would be stale elsewhere. Needs
        // `Track::running(&self) -> Option<&Entry>` plus polling that does not
        // stop when the section is hidden — both in `track.rs`, another owner's
        // file.
        div()
            .id("title-bar")
            .flex()
            .flex_none()
            .items_center()
            .gap(space::S3)
            .h(shell::TITLEBAR)
            .w_full()
            // The redesign's asymmetric inset — 14 left, 6 right — because the
            // right end holds icon buttons that carry their own padding.
            .pl(space::S3)
            .pr(shell::TITLE_INSET_R)
            // Chrome, not surface: the title row and the tab strip share the one
            // recessed ground, so the body reads as raised out of both.
            .bg(t.chrome)
            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                window.start_window_move()
            })
            // The app mark.
            .child(
                div()
                    .flex_none()
                    .size(shell::MARK)
                    .rounded(radius::PILL)
                    .bg(t.accent),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(text::H5)
                    .font_family(fonts(cx).heading)
                    .text_color(t.text)
                    .child(APP_NAME),
            )
            .child(div().flex_none().w(px(1.)).h(shell::RULE).bg(t.divider))
            // "Jump to anything", with the shortcut spelled for whichever platform
            // this build is for — see `rp_ui::shortcut` for why that is not a
            // hard-coded string.
            .child(
                rp_ui::SearchBar::new("palette")
                    .placeholder("Jump to anything")
                    .chord(PALETTE)
                    .active(self.palette_open)
                    .render(
                        t,
                        cx.listener(|this, _: &gpui::ClickEvent, window, cx| {
                            this.toggle_palette(window, cx)
                        }),
                    ),
            )
            .child(div().flex_1())
            // Where the mockups had minimise/maximise/close pills. The app runs
            // with native decorations, so Windows already draws those a few
            // pixels higher; a second, differently-behaving set was a trap.
            .child(signature(cx, t))
            .child(gear(t))
    }

    /// One group per open file, and a box that searches across all of them.
    ///
    /// The search looks at the request name, its method and its URL, so "post" and
    /// "films" both find something — and it is over every open file at once, which
    /// is the point of having several open. A group with no match disappears
    /// entirely rather than sitting there as an empty header.
    fn requests_list(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = theme::theme(cx);
        let query = self.request_query.read(cx).text().trim().to_lowercase();
        let open = self.request_tabs.active_key().cloned();
        let mut groups = Vec::new();
        let mut shown = 0usize;
        for source in &self.sources {
            let path = source.path.clone();
            let mut rows = Vec::new();
            for (i, r) in source.requests.iter().enumerate() {
                if !query.is_empty()
                    && !r.name.to_lowercase().contains(&query)
                    && !r.method.to_lowercase().contains(&query)
                    && !r.url.to_lowercase().contains(&query)
                {
                    continue;
                }
                let active = open
                    == Some(TabKey::Request {
                        source: path.clone(),
                        index: i,
                    });
                let full = SharedString::from(r.name.clone());
                let at = path.clone();
                rows.push(
                    sidebar_row(t, active)
                        // Unique across files: two files both have a request 0.
                        .id(SharedString::from(format!("{}-{i}", path.display())))
                        .tooltip({
                            let full = full.clone();
                            move |_, cx| cx.new(|_| Tip(full.clone())).into()
                        })
                        .child(method_badge(t, &r.method))
                        .child(
                            div()
                                .flex_1()
                                .truncate()
                                .text_color(if active { t.accent } else { t.text })
                                .child(full),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_request(&at, i, cx);
                            cx.defer(save_session);
                            cx.notify();
                        })),
                );
            }
            shown += rows.len();
            // A search that matches nothing in this file hides the file; without a
            // search an empty file still shows, so "it opened and holds nothing" is
            // visible rather than looking like it failed to open.
            if rows.is_empty() && !query.is_empty() {
                continue;
            }
            let collapsed = source.collapsed;
            let count = source.requests.len();
            let closing = path.clone();
            let toggling = path.clone();
            groups.push(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id(SharedString::from(format!("group-{}", path.display())))
                            .flex()
                            .items_center()
                            .gap(space::S1)
                            .h(control::MD)
                            .px(space::S3)
                            .text_size(text::LABEL)
                            .text_color(t.text_label())
                            .cursor_pointer()
                            .hover(|s| s.bg(t.hover()))
                            .tooltip({
                                let full = SharedString::from(path.display().to_string());
                                move |_, cx| cx.new(|_| Tip(full.clone())).into()
                            })
                            .child(icon_tinted(
                                if collapsed {
                                    "chevron_right"
                                } else {
                                    "chevron_left"
                                },
                                text::SMALL,
                                t.text_muted(),
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(source.label.clone()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(text::TINY)
                                    .text_color(t.text_meta())
                                    .child(SharedString::from(count.to_string())),
                            )
                            // Closing one file must not be reachable by accident,
                            // so it is its own target rather than part of the row.
                            .child(
                                div()
                                    .id(SharedString::from(format!("close-{}", path.display())))
                                    .flex()
                                    .flex_none()
                                    .p(space::S1 * 0.5)
                                    .rounded(radius::PILL)
                                    .hover(|s| s.bg(t.active()))
                                    .child(icon_tinted("close", text::SMALL, t.text_muted()))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.close_source(&closing, cx)
                                    })),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(source) =
                                    this.sources.iter_mut().find(|s| s.path == toggling)
                                {
                                    source.collapsed = !source.collapsed;
                                }
                                cx.notify();
                            })),
                    )
                    .when(!collapsed, |d| d.children(rows))
                    .when_some(source.error.clone(), |d, why| {
                        d.child(
                            div()
                                .px(space::S3)
                                .pb(space::S1)
                                .text_size(text::TINY)
                                .text_color(t.danger())
                                .child(why),
                        )
                    }),
            );
        }
        let empty = groups.is_empty();
        let count: SharedString = if query.is_empty() {
            format!("{} files", self.sources.len()).into()
        } else {
            format!("{shown} of {} files", self.sources.len()).into()
        };
        sidebar(t, self.sidebar_width)
            .child(section(cx, t, "Collection"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(space::S1)
                    .px(space::S3)
                    .pb(space::S2)
                    .child(Button::new("open-file", "Open…").icon("file").render(
                        t,
                        cx.listener(|this, _, _, cx| this.open_dialog(false, false, cx)),
                    ))
                    // Adds instead of replacing, and takes several files at once —
                    // "open" and "also open this" are different intentions and now
                    // have different buttons.
                    .child(Button::new("add-file", "Add…").icon("plus").render(
                        t,
                        cx.listener(|this, _, _, cx| this.open_dialog(false, true, cx)),
                    ))
                    .child(Button::new("open-dir", "Folder…").icon("folder").render(
                        t,
                        cx.listener(|this, _, _, cx| this.open_dialog(true, true, cx)),
                    ))
                    .child(
                        Button::new("open-vars", "Variables")
                            .icon("settings")
                            .render(
                                t,
                                cx.listener(|this, _, _, cx| {
                                    this.open_vars(cx);
                                    cx.defer(save_session);
                                }),
                            ),
                    )
                    .child(
                        Button::new("save", "Save")
                            .icon("save")
                            .tone(Tone::Primary)
                            .render(t, cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            )
            .child(
                div()
                    .px(space::S3)
                    .pb(space::S2)
                    .text_size(text::SMALL)
                    .truncate()
                    .text_color(if self.error.is_some() {
                        t.text_label()
                    } else {
                        t.text_meta()
                    })
                    .child(self.error.clone().unwrap_or(count)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::S2)
                    .px(space::S3)
                    .pb(space::S2)
                    .child(icon_tinted("search", text::CONTROL, t.text_muted()))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .child(self.request_query.clone()),
                    ),
            )
            .child(divider(t))
            .child(
                div()
                    .id("request-list")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_scroll()
                    .when(empty, |d| {
                        d.child(empty_state(
                            cx,
                            t,
                            "folder",
                            if self.sources.is_empty() {
                                "Nothing loaded"
                            } else {
                                "No match"
                            },
                            if self.sources.is_empty() {
                                "Open a .http file or a Bruno collection, and add more with Add…"
                            } else {
                                "Nothing in the open files matches that search."
                            },
                        ))
                    })
                    .children(groups),
            )
    }

    /// The variables tab: environments, the editable values, and what earlier
    /// responses captured.
    ///
    /// A pane and no longer a strip under the request list. Two reasons, both
    /// from the same screenshot: the list of requests was down to about nine
    /// visible rows, and every value editor was as narrow as the sidebar. Here the
    /// fields wrap into as many columns as the width allows.
    fn vars_pane(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = theme::theme(cx);
        let mut env_chips = Vec::with_capacity(self.environments.len());
        for (i, env) in self.environments.iter().enumerate() {
            let active = self.selected_env == Some(i);
            // `.tag` — a pill on a wash of its own family, not an outline.
            let (fill, ink) = if active {
                (t.accent_wash_strong(), t.accent)
            } else {
                t.tag_neutral()
            };
            env_chips.push(
                div()
                    .id(SharedString::from(format!("env-{i}")))
                    .flex()
                    .items_center()
                    .px(space::S2)
                    .py(space::S1)
                    .rounded(radius::PILL)
                    .text_size(text::TINY)
                    .bg(fill)
                    .text_color(ink)
                    .hover(|s| s.bg(t.accent_wash()))
                    .child(SharedString::from(env.name.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_env = Some(i);
                        this.rebuild_var_rows(cx);
                        cx.notify();
                    })),
            );
        }

        // `.field`: the label sits above its input, which is how the system states
        // it. Each field claims a column of at least `GRID_CELL` and then shares
        // what is left, so a wide pane lays them out side by side instead of one
        // long column of short boxes.
        let mut rows = Vec::with_capacity(self.var_rows.len());
        for (name, input) in &self.var_rows {
            rows.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(space::S1)
                    .flex_basis(size::GRID_CELL)
                    .flex_grow()
                    .min_w(size::GRID_CELL_MIN)
                    .child(
                        div()
                            .truncate()
                            .text_size(text::LABEL)
                            .text_color(t.text_label())
                            .child(SharedString::from(name.clone())),
                    )
                    .child(input.clone()),
            );
        }

        // Captured values are shown separately: they are results, not settings.
        let mut caught = Vec::with_capacity(self.captured.len());
        for (name, value) in &self.captured {
            caught.push(
                div()
                    .truncate()
                    .text_size(text::SMALL)
                    .text_color(t.success())
                    .child(SharedString::from(format!("{name} = {value}"))),
            );
        }

        div()
            .id("vars-pane")
            .flex()
            .flex_col()
            .gap(space::S3)
            .size_full()
            .min_w_0()
            .p(space::S4)
            .overflow_y_scroll()
            .child(section(cx, t, "Environment").px_0().pt_0())
            .when(!env_chips.is_empty(), |d| {
                d.child(div().flex().flex_wrap().gap(space::S1).children(env_chips))
            })
            .child(section(cx, t, "Variables").px_0().pt_0())
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x(space::S4)
                    .gap_y(space::S3)
                    .min_w_0()
                    .children(rows),
            )
            .when(self.var_rows.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(text::SMALL)
                        .text_color(t.text_meta())
                        .child("none — add an http-client.env.json or `@name = value`"),
                )
            })
            .when(!caught.is_empty(), |d| {
                d.child(section(cx, t, "Captured from responses").px_0().pt_0())
                    .children(caught)
            })
    }

    fn tools_list(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let query = self.tool_query.read(cx).text().to_string();
        let matches = rp_devtools::search(&query);
        let count = matches.len();
        let selected = self.tool_tabs.active_key().cloned();
        // Rows are built eagerly: `cx` is a `&mut` borrow, so a lazy `map`
        // closure would let it escape.
        let mut rows = Vec::with_capacity(count);
        for tool in matches {
            rows.push(self.tool_row(tool, selected == Some(TabKey::Tool(tool.id)), cx));
        }
        let t = theme::theme(cx);
        sidebar(t, self.sidebar_width)
            .child(section(cx, t, "Tools"))
            .child(
                div()
                    .px(space::S3)
                    .pb(space::S2)
                    .child(self.tool_query.clone()),
            )
            .child(
                div()
                    .px(space::S3)
                    .pb(space::S2)
                    .text_size(text::SMALL)
                    .text_color(t.text_meta())
                    .child(SharedString::from(format!("{count} tools"))),
            )
            .child(divider(t))
            .child(
                div()
                    .id("tool-list")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_scroll()
                    .when(count == 0, |d| {
                        d.child(empty_state(
                            cx,
                            t,
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
        let t = theme::theme(cx);
        let (fill, ink) = t.tag_neutral();
        sidebar_row(t, active)
            .id(tool.id)
            // Tool names run long too — "Encode/decode url formatted strings".
            .tooltip(move |_, cx| cx.new(|_| Tip(tool.name.into())).into())
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_color(if active { t.accent } else { t.text })
                    .child(tool.name),
            )
            // Mark the tools this build cannot provide, so the disappointment
            // happens before the click rather than after it.
            .when(unavailable(tool), |d| {
                d.child(
                    div()
                        .flex_none()
                        .px(space::S2)
                        .rounded(radius::PILL)
                        .bg(fill)
                        .text_size(text::TINY)
                        .text_color(ink)
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
        let Some(tab) = self.tabs_mut().remove(i) else {
            return;
        };
        let section = match tab.key {
            TabKey::Request { .. } | TabKey::Vars => Section::Requests,
            TabKey::Tool(_) => Section::Tools,
        };
        // The new window shares the request/variable state, so a detached
        // request can still be sent with the same environment. `var_rows` holds
        // entities, so both windows edit the same value editors.
        // The new window shares the open files, so a detached request can still be
        // sent — and saved back to the file it came from.
        let sources = self.sources.clone();
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
                    ws.sources = sources;
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
                    match tab.key {
                        TabKey::Tool(_) => ws.tool_tabs.open(tab),
                        TabKey::Request { .. } | TabKey::Vars => ws.request_tabs.open(tab),
                    }
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

/// `h6` with the face that is actually installed.
///
/// `theme::section_label` asks for `FONT_HEADING` by name, and gpui ignores a
/// missing family without a word — so the family is re-set to whatever
/// `resolve_fonts` found.
fn section(cx: &App, t: theme::Theme, label: &'static str) -> gpui::Div {
    section_label(t, label).font_family(fonts(cx).heading)
}

/// The maker's mark, in whatever handwriting this machine has.
///
/// A maker's mark, not a heading: the accent carries it and the opacity keeps it
/// from competing with the app name at the other end of the bar. The line height
/// comes from the scale rather than a nudge, which is what keeps a script face
/// with tall ascenders centred in a 52-px bar.
fn signature(cx: &App, t: theme::Theme) -> impl IntoElement + use<> {
    div()
        .flex_none()
        .px(space::S2)
        .text_size(text::H5)
        .line_height(text::H5_LINE)
        .font_family(fonts(cx).script)
        .text_color(t.accent)
        .opacity(0.85)
        .child(SIGNATURE)
}

/// The gear: opens the settings window, or brings it forward if it is already up.
fn gear(t: theme::Theme) -> impl IntoElement {
    div()
        .id("settings")
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(shell::CONTROL_W)
        .h(shell::CONTROL_H)
        .rounded(radius::PILL)
        .hover(|s| s.bg(t.hover()))
        .child(icon_tinted("settings", text::CONTROL, t.text_muted()))
        .on_click(|_, _, cx| open_settings(cx))
}

/// `.seg` — the design's segmented control, choosing the ground.
///
/// Side by side now that it lives in the settings window rather than in the
/// 104-px rail. The machinery is unchanged: [`set_ground`] switches the theme and
/// persists it.
fn ground_switch(cx: &App) -> impl IntoElement + use<> {
    let t = theme::theme(cx);
    let mut opts = Vec::with_capacity(2);
    for (label, dark) in [("Light", false), ("Dark", true)] {
        let on = t.dark == dark;
        opts.push(
            div()
                .id(label)
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .h(control::MD)
                .rounded(radius::PILL)
                .text_size(text::CONTROL)
                .map(|d| {
                    if on {
                        d.bg(t.accent).text_color(t.on_accent())
                    } else {
                        d.text_color(t.text_muted()).hover(|s| s.bg(t.hover()))
                    }
                })
                .child(label)
                .on_click(move |_, _, cx| set_ground(dark, cx)),
        );
    }
    div()
        .flex()
        .gap(space::S1)
        .p(space::S1)
        .rounded(radius::PILL)
        .bg(t.bg)
        .children(opts)
}

/// The settings window, if one is open. Stored so the gear focuses it instead of
/// opening a second copy — the bug this button always ships with.
#[derive(Default)]
struct SettingsWindow(Option<gpui::WindowHandle<Settings>>);

impl Global for SettingsWindow {}

/// Open settings, or focus the window that is already open.
fn open_settings(cx: &mut App) {
    // `update` fails once the window is closed, which is how a stale handle is
    // told apart from a live one.
    if let Some(handle) = cx.try_global::<SettingsWindow>().and_then(|s| s.0)
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    // Sized for the MCP card's config block and token row, not for the two rows
    // it started with. The body scrolls, so a narrow drag or a future section
    // cannot cut content off.
    let bounds = Bounds::centered(None, size(px(680.), px(620.)), cx);
    match cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        },
        |_, cx| cx.new(|_| Settings::default()),
    ) {
        Ok(handle) => cx.set_global(SettingsWindow(Some(handle))),
        Err(e) => eprintln!("request-pilot: could not open settings: {e:#}"),
    }
}

/// The settings window's root: the ground, and the agent's way in.
#[derive(Default)]
struct Settings {
    /// What the last copy put on the clipboard, so a click has visible effect.
    /// Cleared by a timer, which is why it lives on the view and not in a global.
    copied: Option<SharedString>,
}

impl Settings {
    /// Put `payload` on the clipboard and say so for [`COPY_NOTICE`].
    /// Mint a fresh token and put it in force.
    ///
    /// The point of a *persisted* token is that the pasted config keeps working;
    /// the point of this button is that the user can still revoke it. Anything
    /// holding the old token stops working the moment this returns, so the
    /// confirmation has to say the config must be copied again.
    fn regenerate(&mut self, cx: &mut Context<Self>) {
        let outcome = cx
            .try_global::<Mcp>()
            .and_then(|m| m.server.as_ref())
            .map(|h| h.regenerate_token());
        match outcome {
            Some(Ok(_)) => self.note("New token in force — copy the config again.".into(), cx),
            // A failed store write leaves the old token in force, which is the
            // safe end of that failure; say so rather than implying it rotated.
            Some(Err(e)) => {
                eprintln!("request-pilot: could not regenerate the token: {e:#}");
                self.note(
                    "Could not regenerate — the old token still works.".into(),
                    cx,
                )
            }
            None => self.note("Start the server first.".into(), cx),
        }
    }

    /// Show a transient line under the buttons, the way a copy does.
    fn note(&mut self, what: SharedString, cx: &mut Context<Self>) {
        self.copied = Some(what);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPY_NOTICE).await;
            this.update(cx, |this, cx| {
                this.copied = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn copy(&mut self, what: SharedString, payload: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(payload));
        self.copied = Some(SharedString::from(format!("Copied the {what}.")));
        // The confirmation fades on its own; a label that never clears stops
        // meaning "just now".
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPY_NOTICE).await;
            this.update(cx, |this, cx| {
                this.copied = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
}

/// How long a "Copied …" confirmation stays up.
const COPY_NOTICE: std::time::Duration = std::time::Duration::from_secs(3);

impl Render for Settings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Read, never captured, so `cx.refresh_windows()` repaints this window on
        // the new ground like every other one.
        let t = theme::theme(cx);
        div()
            .flex()
            .flex_col()
            .gap(space::S3)
            .size_full()
            .p(space::S5)
            .bg(t.bg)
            .font_family(fonts(cx).body)
            .text_size(text::BODY)
            .line_height(text::BODY_LINE)
            .text_color(t.text)
            .child(
                // `overflow_y_scroll` lives on StatefulInteractiveElement, so the
                // scroller needs an id. `min_w_0` is what lets the cards shrink
                // below their content when the window is dragged narrow — a flex
                // child never goes under its own width without it.
                div()
                    .id("settings-body")
                    .flex()
                    .flex_col()
                    .gap(space::S3)
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_y_scroll()
                    .child(
                        card(t)
                            .min_w_0()
                            .child(section(cx, t, "Appearance"))
                            .child(ground_switch(cx)),
                    )
                    .child(mcp_card(self.copied.clone(), cx)),
            )
    }
}

/// The server name clients see, and the key it goes under in `mcpServers`.
const MCP_NAME: &str = "time-tracker";

/// The `mcpServers` block to paste into a client's configuration.
///
/// Identical for Claude Code (`.mcp.json` in a project, or `~/.claude.json`) and
/// for Claude Desktop (`claude_desktop_config.json`): the streamable-HTTP
/// transport is configured the same way in both, so no stdio bridge is needed.
/// `"type": "http"` (alias `"streamable-http"`), `headers` is an object, and the
/// header is spelled `Authorization`.
///
/// Built with `json!` rather than a format string so the output cannot be
/// invalid JSON, whatever the token contains.
fn mcp_config_json(url: &str, token: &str) -> String {
    let config = serde_json::json!({
        "mcpServers": {
            MCP_NAME: {
                "type": "http",
                "url": url,
                "headers": { "Authorization": format!("Bearer {token}") },
            }
        }
    });
    serde_json::to_string_pretty(&config).unwrap_or_default()
}

/// The Claude Code CLI equivalent, as one pasteable line. The long flags on
/// purpose: `-t`/`-H` read as noise when this lands in a chat window.
fn mcp_add_command(url: &str, token: &str) -> String {
    format!(
        "claude mcp add --transport http {MCP_NAME} {url} \
         --header \"Authorization: Bearer {token}\""
    )
}

/// The sentence the user pastes to the assistant.
///
/// This exists because the server **cannot** explain itself. `rp_mcp::proto`
/// returns an `instructions` string in its `initialize` result — the field that
/// is supposed to carry exactly this — and Claude Desktop stores it but never
/// reads it (anthropics/claude-code#43749: `getInstructions()` has no call
/// sites); Claude Code makes no promise to surface it either. So the purpose has
/// to travel by hand, and asking the user to invent the wording is the friction
/// this card is here to remove.
const MCP_PROMPT: &str = "Book your working time to my time tracker through the \
    `time-tracker` MCP: start a timer when you begin a task, stop it when you \
    finish or switch away, log time for work done earlier, and give me a summary \
    of the day when I ask.";

/// The MCP server's controls: state, address, and everything a client needs in
/// one paste.
fn mcp_card(copied: Option<SharedString>, cx: &mut Context<Settings>) -> impl IntoElement + use<> {
    let t = theme::theme(cx);
    let mcp = cx.try_global::<Mcp>();
    let running = mcp.is_some_and(Mcp::running);
    let url = mcp
        .and_then(|m| m.server.as_ref())
        .map(|h| SharedString::from(h.url()));
    let token = mcp
        .and_then(|m| m.server.as_ref())
        .map(|h| h.token())
        .unwrap_or_default();
    let error = mcp.and_then(|m| m.error.clone());
    let port = mcp.map_or(rp_mcp::http::DEFAULT_PORT, |m| m.port);
    let wide = mcp.map(|m| m.bind) == Some(rp_mcp::http::Bind::Any);
    // What the server has actually seen. The one thing that tells "the client
    // never reached me" apart from "it reached me and I turned it away" — and
    // without it the only place that answer existed was a stderr stream a
    // windowed app does not have.
    let activity: Vec<SharedString> = mcp
        .and_then(|m| m.server.as_ref())
        .map(|h| h.activity())
        .unwrap_or_default()
        .into_iter()
        .rev()
        .map(SharedString::from)
        .collect();
    let (fill, ink) = if running {
        (t.accent_wash_strong(), t.accent)
    } else {
        t.tag_neutral()
    };
    let mono = fonts(cx).mono;

    card(t)
        .child(
            div()
                .flex()
                .items_center()
                .gap(space::S2)
                .child(section(cx, t, "Agent access (MCP)").flex_1())
                .child(
                    div()
                        .flex_none()
                        .px(space::S2)
                        .rounded(radius::PILL)
                        .bg(fill)
                        .text_size(text::TINY)
                        .text_color(ink)
                        .child(if running { "running" } else { "stopped" }),
                ),
        )
        .child(
            div()
                .text_size(text::SMALL)
                .text_color(t.text_meta())
                .child(match (running, wide) {
                    (true, false) => {
                        "Claude can reach the nine time-tracking tools over HTTP. \
                         This machine only, and the token is required."
                    }
                    (true, true) => {
                        "Reachable from containers and from other machines on this \
                         network. The token is the only thing in the way."
                    }
                    (false, _) => {
                        "Off. Starting it lets Claude book time over HTTP instead of \
                         launching the rp-mcp binary."
                    }
                }),
        )
        // The one setting that takes the machine boundary away, so it says what it
        // costs rather than hiding behind a label.
        .child(
            div()
                .id("mcp-bind")
                .flex()
                .items_start()
                .gap(space::S2)
                .cursor_pointer()
                .child(
                    div()
                        .flex_none()
                        .mt(space::S1 * 0.5)
                        .child(check_box(t, wide)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .gap(space::S1 * 0.5)
                        .child(
                            div()
                                .text_size(text::CONTROL)
                                .text_color(t.text)
                                .child("Reachable from containers"),
                        )
                        .child(
                            div()
                                .text_size(text::SMALL)
                                .text_color(if wide { t.warning() } else { t.text_meta() })
                                .child(
                                    "A client in a Docker sandbox has its own network, so \
                                     127.0.0.1 there is the container and not this machine. \
                                     This listens on every interface so host.docker.internal \
                                     reaches it — which also means anything that can route \
                                     here can try, with only the token in the way.",
                                ),
                        ),
                )
                .on_click(cx.listener(|_, _: &gpui::ClickEvent, _, cx| toggle_mcp_bind(cx))),
        )
        // The address, or the port it will try.
        .child(
            div()
                .flex()
                .items_center()
                .gap(space::S2)
                .text_size(text::SMALL)
                .child(
                    div()
                        .flex_none()
                        .w(size::LABEL)
                        .text_color(t.text_label())
                        .child("URL"),
                )
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .font_family(mono.clone())
                        .text_color(t.text)
                        .child(url.clone().unwrap_or_else(|| {
                            SharedString::from(format!(
                                "http://{}:{port}{} (when started)",
                                if wide {
                                    "host.docker.internal"
                                } else {
                                    "127.0.0.1"
                                },
                                rp_mcp::http::PATH
                            ))
                        })),
                ),
        )
        // With a wide bind the config above is the container's address, which does
        // not work from this machine — so the local one is shown as well rather
        // than leaving the user to work out that they need a second URL.
        .when(running && wide, |d| {
            let local = mcp
                .and_then(|m| m.server.as_ref())
                .map(|h| SharedString::from(h.local_url()))
                .unwrap_or_default();
            d.child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::S2)
                    .text_size(text::SMALL)
                    .child(
                        div()
                            .flex_none()
                            .w(size::LABEL)
                            .text_color(t.text_label())
                            .child("From this machine"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .font_family(mono.clone())
                            .text_color(t.text_muted())
                            .child(local),
                    ),
            )
        })
        // The token is shown but no longer has a copy button of its own: on its
        // own it is not enough to configure anything, which was the complaint.
        // The three buttons below carry it inside something pasteable.
        .when(running, |d| {
            d.child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::S2)
                    .text_size(text::SMALL)
                    .child(
                        div()
                            .flex_none()
                            .w(size::LABEL)
                            .text_color(t.text_label())
                            .child("Token"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .font_family(mono)
                            .text_color(t.text)
                            .child(SharedString::from(token.clone())),
                    ),
            )
        })
        // Everything a client needs, in one paste each. Only while it runs: there
        // is no token to put in them otherwise.
        .when(running, |d| {
            let url = url.clone().unwrap_or_default();
            let config = mcp_config_json(&url, &token);
            let command = mcp_add_command(&url, &token);
            d.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(space::S2)
                    .child(
                        Button::new("mcp-copy-config", "Copy config")
                            .icon("copy")
                            .tone(Tone::Primary)
                            .render(
                                t,
                                cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                                    this.copy("config".into(), config.clone(), cx)
                                }),
                            ),
                    )
                    .child(
                        Button::new("mcp-copy-command", "Copy command")
                            .icon("copy")
                            .render(
                                t,
                                cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                                    this.copy("command".into(), command.clone(), cx)
                                }),
                            ),
                    )
                    .child(
                        Button::new("mcp-copy-prompt", "Copy prompt")
                            .icon("copy")
                            .render(
                                t,
                                cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                                    this.copy("prompt".into(), MCP_PROMPT.into(), cx)
                                }),
                            ),
                    )
                    .child(
                        Button::new("mcp-regenerate", "Regenerate token")
                            .icon("alert")
                            .render(
                                t,
                                cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                                    this.regenerate(cx)
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(text::SMALL)
                    .text_color(t.text_meta())
                    .child(
                        "The config block works as-is for Claude Code (.mcp.json) and for \
                         Claude Desktop (claude_desktop_config.json). The command is the \
                         Claude Code CLI equivalent. The prompt tells the assistant what the \
                         tools are for — the server sends that too, but no client reads it.",
                    ),
            )
            // The token is minted per start and deliberately never stored, so a
            // pasted config dies with the process. Saying so here is cheaper than
            // the user blaming the server for a 401.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::S2)
                    .text_size(text::SMALL)
                    .text_color(t.text_label())
                    .child(icon_tinted("alert", text::CONTROL, t.text_label()))
                    .child(
                        "The token is stored, so the config keeps working after a restart. \
                         Regenerate it to lock out anything still holding the old one — \
                         then copy the config again.",
                    ),
            )
        })
        // What the server has seen, newest first.
        //
        // This is the panel to look at when a client shows no tools. An empty list
        // means nothing ever reached the port — a wrong address, or a client in a
        // container while this is on loopback. A list of `refused:` lines means it
        // arrived and was turned away, and says why. A `tools/list` line means the
        // handshake worked and the problem is on the client's side, which is
        // usually that `claude mcp add` was run inside a running session: servers
        // are connected at startup, so the tools appear after a restart.
        .when(running, |d| {
            d.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(space::S1)
                    .child(section(cx, t, "Recent requests").px_0().pt_0())
                    .when(activity.is_empty(), |d| {
                        d.child(
                            div()
                                .text_size(text::SMALL)
                                .text_color(t.text_meta())
                                .child(
                                    "Nothing has reached the server yet. If a client shows no \
                                     tools, it never got here — check the address it is using, \
                                     and remember that a client in a container cannot reach \
                                     127.0.0.1 on this machine.",
                                ),
                        )
                    })
                    .child(
                        div()
                            .id("mcp-activity")
                            .flex()
                            .flex_col()
                            .max_h(space::S8 * 4.)
                            .overflow_y_scroll()
                            .font_family(fonts(cx).mono)
                            .text_size(text::TINY)
                            .children(activity.into_iter().map(|line| {
                                let refused = line.starts_with("refused:");
                                div()
                                    .truncate()
                                    .text_color(if refused { t.danger() } else { t.text_muted() })
                                    .child(line)
                            })),
                    ),
            )
        })
        .when_some(copied, |d, what| {
            d.child(
                div()
                    .text_size(text::SMALL)
                    .text_color(t.success())
                    .child(what),
            )
        })
        .when_some(error, |d, error| {
            d.child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::S2)
                    .text_size(text::SMALL)
                    .text_color(t.text_label())
                    .child(icon_tinted("alert", text::CONTROL, t.text_label()))
                    .child(error),
            )
        })
        .child(
            div().flex().child(
                Button::new(
                    "mcp-toggle",
                    if running {
                        "Stop server"
                    } else {
                        "Start server"
                    },
                )
                .icon(if running { "stop" } else { "play" })
                .tone(Tone::Primary)
                .render(t, |_, _, cx| toggle_mcp(cx)),
            ),
        )
}

/// The shared geometry of a sidebar list row, so requests and tools cannot
/// drift apart. Selected is an accent wash in the pill; hover is a plain ink
/// wash, which is visibly a different thing.
/// A one-line tooltip.
///
/// gpui takes an `AnyView` for a tooltip, so the smallest thing that works is a
/// view holding a string. It exists because a truncated row is unreadable even
/// after the sidebar has been widened — a long enough name still ends in an
/// ellipsis, and then the only way to read it is to hover.
struct Tip(SharedString);

impl Render for Tip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme(cx);
        div()
            .px(space::S2)
            .py(space::S1)
            .rounded(radius::MD)
            .bg(t.surface)
            .border_1()
            .border_color(t.divider)
            .shadow_md()
            .text_size(text::SMALL)
            .text_color(t.text)
            .child(self.0.clone())
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Variable edits are typed into their own editors, so push the current
        // set into the pane before it can be sent.
        if let Some((path, _, pane)) = self.active_request() {
            let vars = self.effective_vars(&path, cx);
            pane.update(cx, |pane, _| pane.set_vars(vars));
        }
        let t = theme::theme(cx);
        let rail = self.rail(cx);
        // Built here, not inside the tab strip: the strip clips its own children,
        // so an overlay drawn inside it would be cut off.
        let tab_menu = tabs::menu(self, cx);
        let tab_overflow = tabs::overflow(self, cx);
        // Same reason, one layer higher: the palette covers the whole window, so it
        // is built here and drawn deferred above everything including the menu.
        let command_palette = if self.palette_open {
            let hits = self.palette_hits(cx);
            let rows = hits.iter().map(|h| h.row.clone()).collect();
            // The entity, not `cx.listener`: the palette hands its handlers a plain
            // `&mut App`, which is all a click needs to reach back into the window.
            let choose = cx.entity();
            let dismiss = choose.clone();
            Some(
                rp_ui::Palette::new("command-palette")
                    .rows(rows)
                    .selected(self.palette_selected)
                    .empty("Nothing matches that")
                    .scroll(self.palette_scroll.clone())
                    .render(
                        t,
                        window,
                        self.palette_query.clone().into_any_element(),
                        move |i, window, cx| {
                            choose.update(cx, |this, cx| this.choose_palette(i, window, cx))
                        },
                        move |window, cx| {
                            dismiss.update(cx, |this, cx| this.close_palette(window, cx))
                        },
                    )
                    .into_any_element(),
            )
        } else {
            None
        };
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
            let main = match self.tabs().active().map(|t| &t.content) {
                Some(Content::Generic(pane)) => pane.clone().into_any_element(),
                Some(Content::Custom(view)) => view.clone().into_any_element(),
                Some(Content::Request(pane)) => pane.clone().into_any_element(),
                Some(Content::Vars) => self.vars_pane(cx).into_any_element(),
                Some(Content::Unavailable(reason)) => div()
                    .flex()
                    .flex_1()
                    .h_full()
                    .child(empty_state(cx, t, "alert", "No UI for this tool", reason))
                    .into_any_element(),
                None => div()
                    .flex()
                    .flex_1()
                    .h_full()
                    .child(empty_state(
                        cx,
                        t,
                        "file",
                        "Nothing open",
                        "Pick a request or a tool on the left.",
                    ))
                    .into_any_element(),
            };
            let strip = tabs::strip(self, cx);
            // `min_w_0` on every link of this chain, or the pane never shrinks
            // below its content and runs off the right edge of the window — a
            // flex child does not go under its intrinsic width unless allowed
            // to, in gpui as in CSS. The panes themselves were made flexible
            // earlier; this column is what was still holding them open.
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(list)
                .child(self.sidebar_handle(t, cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .bg(t.bg)
                        .when(!self.tabs().is_empty(), |d| d.child(strip))
                        .child(
                            div()
                                .flex()
                                .flex_1()
                                .min_w_0()
                                .min_h_0()
                                .overflow_hidden()
                                .child(main),
                        ),
                )
                .into_any_element()
        };
        // The mockups stack a title bar over the rail-plus-body row.
        //
        // ponytail: they also round the whole block by 22 and give it a shadow,
        // i.e. a client-drawn frame. Not done: that needs
        // `WindowBackgroundAppearance::Transparent` plus
        // `WindowDecorations::Client`, which hands resize, snap and move to the
        // app — and rounding the root *without* them leaves the OS frame's own
        // square corners showing through as notches. Windows 11 already rounds
        // the frame. See the report.
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(t.bg)
            .overflow_hidden()
            .font_family(fonts(cx).body)
            .text_size(text::BODY)
            .line_height(text::BODY_LINE)
            .text_color(t.text)
            // The palette's keys, on the root: the overlay is a child of this
            // element, so an action raised inside it bubbles up to here. Movement
            // and Enter are scoped to the palette's own key context and so only
            // exist while it is open; the chord that opens it is not.
            .on_action(cx.listener(|this, _: &rp_ui::palette::Toggle, window, cx| {
                this.toggle_palette(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &rp_ui::palette::Dismiss, window, cx| {
                    this.close_palette(window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &rp_ui::palette::SelectNext, _, cx| {
                this.move_palette(true, cx)
            }))
            .on_action(cx.listener(|this, _: &rp_ui::palette::SelectPrev, _, cx| {
                this.move_palette(false, cx)
            }))
            .on_action(
                cx.listener(|this, _: &rp_ui::palette::Confirm, window, cx| {
                    let at = this.palette_selected;
                    this.choose_palette(at, window, cx)
                }),
            )
            // The move and release listeners sit on the root, not on the handle:
            // a pointer dragging a 9-px strip leaves it immediately, and an
            // element only hears about moves over itself. Only while a drag is
            // live, so the rest of the time the root has no mouse listeners at
            // all.
            .when(self.resizing.is_some(), |d| {
                d.on_mouse_move(cx.listener(|this, ev: &gpui::MouseMoveEvent, _, cx| {
                    this.resize_sidebar(ev.position.x, cx)
                }))
                .on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.end_sidebar_drag(cx)),
                )
                // A release outside the window never arrives as `on_mouse_up`, so
                // without this the sidebar would still be following the pointer
                // after the button was let go.
                .on_mouse_up_out(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.end_sidebar_drag(cx)),
                )
            })
            .child(self.title_bar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(rail)
                    .child(body),
            )
            .when_some(tab_menu, |d, menu| d.child(menu))
            .when_some(tab_overflow, |d, list| d.child(list))
            .when_some(command_palette, |d, p| d.child(p))
    }
}

/// How often the open files are stat'ed for an outside change.
///
/// ponytail: polling, not a platform watcher. A handful of `stat` calls a second
/// is nothing, it behaves the same on every platform, and it needs no dependency.
/// The upgrade path if someone opens hundreds of files is the `notify` crate.
const WATCH_INTERVAL_MS: u64 = 900;

/// Where the dragged sidebar width is remembered, and the bounds it is held to.
///
/// The floor is the width of the two buttons in the list header; the ceiling is
/// generous but finite, because a sidebar that can eat the whole window is a way
/// to lose the pane behind it with no way back.
const SIDEBAR_WIDTH_KEY: &str = "ui.sidebar_width";
const SIDEBAR_MIN: Pixels = px(200.);
const SIDEBAR_MAX: Pixels = px(720.);

fn clamp_sidebar(width: Pixels) -> Pixels {
    width.max(SIDEBAR_MIN).min(SIDEBAR_MAX)
}

/// The remembered width, or the design's own. A stored value that no longer
/// parses, or that a smaller screen has made absurd, is clamped rather than
/// honoured — the alternative is a window that opens unusable.
fn stored_sidebar_width(cx: &App) -> Pixels {
    let stored = cx
        .try_global::<Persist>()
        .and_then(|p| p.store.as_ref())
        .and_then(|store| store.get(SIDEBAR_WIDTH_KEY).ok().flatten())
        .and_then(|v| v.parse::<f32>().ok());
    match stored {
        Some(w) => clamp_sidebar(px(w)),
        None => size::SIDEBAR,
    }
}

/// A `Kind::Custom` tool with neither a generic form nor a bespoke pane.
fn unavailable(tool: &'static Tool) -> bool {
    tool.kind == rp_devtools::Kind::Custom && !custom::HAS_PANE.contains(&tool.id)
}

fn main() {
    // Icons are compiled in; without an asset source `svg()` renders nothing.
    Application::new()
        .with_assets(rp_ui::Assets)
        .run(|cx: &mut App| {
            let resolved = resolve_fonts(cx);
            cx.set_global(resolved);
            // One call: the design system registers what its widgets need.
            rp_ui::bind_keys(cx, PALETTE);
            // Tabs can be pulled out into their own windows; the app lives until
            // the last one is gone.
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    // Release the port before the process goes, so a restart
                    // never meets its own listener. Closing the settings window
                    // while a workspace is open does not reach this branch.
                    if cx.has_global::<Mcp>() {
                        cx.update_global::<Mcp, _>(|mcp, _| mcp.stop());
                    }
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
            // Before the first window renders, so it opens on the right ground
            // instead of flashing dark and correcting itself.
            theme::set_theme(stored_dark(store.as_ref()), cx);
            // Off unless the user turned it on last time — it opens a socket, so
            // it waits to be asked.
            let start_mcp = mcp_enabled(store.as_ref());
            cx.set_global(Mcp::new(
                stored_port(store.as_ref()),
                stored_bind(store.as_ref()),
            ));
            cx.set_global(Persist {
                store,
                windows: Vec::new(),
            });
            if start_mcp {
                cx.update_global::<Mcp, _>(|mcp, _| mcp.start());
                if let Some(e) = cx.global::<Mcp>().error.clone() {
                    eprintln!("request-pilot: the MCP server did not start: {e}");
                }
                if let Some(h) = cx.global::<Mcp>().server.as_ref() {
                    // Never the token: stderr goes to logs and terminals.
                    eprintln!("request-pilot: the MCP server is listening on {}", h.url());
                }
            }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, TcpListener};

    /// A database of our own. Never the user's: these tests start a real server.
    fn temp_db(tag: &str) -> std::path::PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "request-pilot-mcp-{tag}-{}-{}.db",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn the_mcp_server_is_off_by_default_and_its_choice_survives_a_restart() {
        let store = Store::open_in_memory().expect("store");
        assert!(!mcp_enabled(Some(&store)), "it opens a socket; it waits");
        assert!(!mcp_enabled(None), "and without a database too");
        assert_eq!(stored_port(Some(&store)), rp_mcp::http::DEFAULT_PORT);

        store.set(MCP_ENABLED_KEY, "1").unwrap();
        store.set(MCP_PORT_KEY, "5123").unwrap();
        assert!(mcp_enabled(Some(&store)));
        assert_eq!(stored_port(Some(&store)), 5123);

        // A hand-edited or half-written value must not stop the app starting.
        for junk in ["nonsense", "0", "", "99999999"] {
            store.set(MCP_PORT_KEY, junk).unwrap();
            assert_eq!(
                stored_port(Some(&store)),
                rp_mcp::http::DEFAULT_PORT,
                "{junk:?}"
            );
        }
        store.set(MCP_ENABLED_KEY, "yes please").unwrap();
        assert!(!mcp_enabled(Some(&store)), "only \"1\" means on");
    }

    #[test]
    fn starting_twice_keeps_one_listener_and_stopping_frees_the_port() {
        let db = temp_db("toggle");
        let mut mcp = Mcp::new(0, rp_mcp::http::Bind::Loopback);
        mcp.db = Some(db.clone());

        mcp.start();
        assert!(mcp.running(), "{:?}", mcp.error);
        let port = mcp.port;
        let token = mcp.server.as_ref().unwrap().token();
        assert_ne!(port, 0, "the bound port must be reported back");

        // A second click is a no-op, not a second listener and not a new token.
        mcp.start();
        assert_eq!(mcp.port, port);
        assert_eq!(mcp.server.as_ref().unwrap().token(), token);
        assert!(
            TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_err(),
            "the port is in use while it runs"
        );

        mcp.stop();
        assert!(!mcp.running());
        mcp.stop(); // Stopping when nothing runs is inert.
        assert!(
            TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok(),
            "the port must be free again after a stop"
        );
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn a_taken_port_shows_an_error_instead_of_taking_the_app_down() {
        let squatter = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = squatter.local_addr().unwrap().port();
        let db = temp_db("taken");
        let mut mcp = Mcp::new(port, rp_mcp::http::Bind::Loopback);
        mcp.db = Some(db.clone());

        mcp.start();
        assert!(!mcp.running());
        let error = mcp.error.clone().expect("the UI needs something to show");
        assert!(error.contains(&port.to_string()), "{error}");

        // And the error clears when it is asked again successfully.
        mcp.port = 0;
        mcp.start();
        assert!(mcp.running(), "{:?}", mcp.error);
        assert!(mcp.error.is_none());
        mcp.stop();
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn an_unopenable_database_is_reported_rather_than_bound() {
        let mut mcp = Mcp::new(0, rp_mcp::http::Bind::Loopback);
        // A directory is not a database file.
        mcp.db = Some(std::env::temp_dir());
        mcp.start();
        assert!(!mcp.running());
        assert!(mcp.error.is_some());
    }

    /// Without a database the app still has to start, and on the ground it
    /// shipped with.
    #[test]
    fn the_ground_defaults_to_dark_when_there_is_no_store() {
        assert!(stored_dark(None));
        // Writing without a store is inert rather than a panic.
        save_dark(None, false);
    }

    #[test]
    fn a_chosen_ground_survives_a_restart() {
        let store = Store::open_in_memory().expect("store");
        // Nothing stored yet.
        assert!(stored_dark(Some(&store)));

        save_dark(Some(&store), false);
        assert!(!stored_dark(Some(&store)));
        // And back again, so the switch is not one-way.
        save_dark(Some(&store), true);
        assert!(stored_dark(Some(&store)));
    }

    /// A hand-edited or half-written value must not leave the app unreadable.
    #[test]
    fn a_junk_stored_value_falls_back_to_dark() {
        let store = Store::open_in_memory().expect("store");
        store.set(THEME_KEY, "chartreuse").expect("set");
        assert!(stored_dark(Some(&store)));
    }

    /// The copied block has to be valid JSON and carry the exact keys the clients
    /// read, or the paste fails in a config file with no error message.
    #[test]
    fn the_copied_config_is_json_a_client_can_actually_load() {
        let json = mcp_config_json("http://127.0.0.1:7717/mcp", "s3cret");
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let server = &v["mcpServers"][MCP_NAME];
        assert_eq!(server["type"], "http");
        assert_eq!(server["url"], "http://127.0.0.1:7717/mcp");
        // `headers` is an object, and the header is spelled exactly this way.
        assert!(server["headers"].is_object(), "{json}");
        assert_eq!(server["headers"]["Authorization"], "Bearer s3cret");
        // Pretty-printed: it is going into a config file a human will read.
        assert!(json.contains('\n'), "{json}");
    }

    /// A token with JSON-hostile characters must not produce a broken block —
    /// which is why the config is built with `json!` and not a format string.
    #[test]
    fn a_token_with_quotes_cannot_break_the_config() {
        let json = mcp_config_json("http://127.0.0.1:1/mcp", "a\"b\\c");
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(
            v["mcpServers"][MCP_NAME]["headers"]["Authorization"],
            "Bearer a\"b\\c"
        );
    }

    #[test]
    fn the_copied_command_is_one_pasteable_line_with_the_live_url_and_token() {
        let cmd = mcp_add_command("http://127.0.0.1:7717/mcp", "s3cret");
        assert_eq!(cmd.lines().count(), 1, "{cmd}");
        assert!(
            cmd.starts_with("claude mcp add --transport http time-tracker "),
            "{cmd}"
        );
        assert!(cmd.contains("http://127.0.0.1:7717/mcp"), "{cmd}");
        assert!(
            cmd.contains("--header \"Authorization: Bearer s3cret\""),
            "{cmd}"
        );
    }

    /// The prompt has to name the tools concretely, because the server's own
    /// `instructions` field is not read by any client.
    #[test]
    fn the_copied_prompt_names_the_server_and_what_it_is_for() {
        assert!(MCP_PROMPT.contains(MCP_NAME), "{MCP_PROMPT}");
        for word in ["start", "stop", "log", "summary"] {
            assert!(MCP_PROMPT.contains(word), "{word} is missing: {MCP_PROMPT}");
        }
        // One pasteable paragraph, not a list the user has to reflow.
        assert_eq!(MCP_PROMPT.lines().count(), 1);
    }
}
