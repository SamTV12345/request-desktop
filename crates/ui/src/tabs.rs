//! The tab strip from the redesign's detail column.
//!
//! Props, in the shape [`crate::search_bar`] documents: the items plus a handler
//! per affordance, each set by name. What a tab *contains* is not this crate's
//! business — an item is a key, a title and two flags, and every handler is given
//! back the key it was handed. That is what lets the strip live beside `button`
//! and `chip` with gpui as its only dependency, while the app keeps its `TabKey`,
//! its `Workspace` and its context menu to itself.
//!
//! ```ignore
//! TabStrip::new("tab-strip", items)
//!     .on_activate(cx.listener(|this, key: &SharedString, _, cx| this.activate(key, cx)))
//!     .on_close(cx.listener(|this, key: &SharedString, _, cx| this.close(key, cx)))
//!     .render(t)
//! ```
//!
//! A handler left unset means the affordance is **not drawn**, rather than drawn
//! and inert — an icon that looks alive and does nothing is a failure this app has
//! already shipped twice.
//!
//! # What the design says
//!
//! The strip sits on the window's recessed chrome ([`Theme::chrome`]), not on the
//! surface: the active tab is `bg`, so the tab reads as lifted out of the chrome
//! rather than as a pill laid on top of it. Pinned tabs are their own run, headed
//! by the same quiet uppercase label the design puts over the sidebar's `Pinned`
//! group, and separated from the rest by a hairline.
//!
//! There is no dirty/unsaved state here: the design draws none, and a tab this app
//! cannot mark dirty has nothing to show.
use crate::icon::icon_tinted;
use crate::theme::Theme;
use crate::tokens::{radius, space, text};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, Pixels, Point, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::*, px,
};
use std::rc::Rc;

/// One tab, as the strip needs to draw it.
///
/// `key` is the caller's identity and opaque here: the strip uses it for gpui's
/// element ids and hands it straight back to the handlers. A `TabKey`, a path, a
/// row id — whatever it is, it must be unique within the strip, because two tabs
/// sharing an element id share a hover group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabItem {
    pub key: SharedString,
    pub title: SharedString,
    /// Pinned tabs form the leading run. [`TabStrip::ordered`] enforces that, so
    /// a caller that keeps its own order does not have to.
    pub pinned: bool,
    pub active: bool,
}

impl TabItem {
    pub fn new(key: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            pinned: false,
            active: false,
        }
    }

    pub fn pinned(mut self, pinned: bool) -> Self {
        self.pinned = pinned;
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

/// A handler for the tab it names.
///
/// Takes `&SharedString` because that is exactly what `Context::listener` yields
/// for an event type of `SharedString`, so a call site is one `cx.listener` and no
/// adapter.
pub type TabHandler = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

/// A handler for the right-click that opens a menu. Its own type, because the
/// event carries more than a key.
pub type TabMenuHandler = Rc<dyn Fn(&TabMenuRequest, &mut Window, &mut App)>;

/// A right-click on a tab: which tab, and where the pointer was.
///
/// The position travels with it because a context menu is anchored at the pointer
/// and only the caller can draw one — the strip scrolls horizontally and would
/// clip a menu of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct TabMenuRequest {
    pub key: SharedString,
    /// Window coordinates, which is what `anchored()` wants.
    pub at: Point<Pixels>,
}

#[derive(Clone)]
pub struct TabStrip {
    pub id: ElementId,
    pub items: Vec<TabItem>,
    pub on_activate: Option<TabHandler>,
    pub on_close: Option<TabHandler>,
    /// Wired to the pinned marker, which is the only place a pin can be undone
    /// without opening a menu.
    pub on_toggle_pin: Option<TabHandler>,
    pub on_detach: Option<TabHandler>,
    pub on_menu: Option<TabMenuHandler>,
}

/// The design's own numbers, which the token scale does not carry.
mod design {
    use gpui::{Pixels, px};
    /// Strip height. Shorter than `control::MD`, because a tab is chrome and not
    /// a control.
    pub const HEIGHT: Pixels = px(40.);
    /// How wide one tab may get before its title ellipsizes.
    pub const TAB_MAX: Pixels = px(210.);
    /// The pinned marker: a dot, not a glyph — the design uses one in the strip
    /// and in the sidebar's pinned group.
    pub const DOT: Pixels = px(6.);
    /// The accent edge along the top of the active tab.
    pub const ACTIVE_EDGE: Pixels = px(2.);
}

/// Uppercase in the literal: the design applies `text-transform`, and gpui has no
/// equivalent.
const PINNED_LABEL: &str = "PINNED";

impl TabStrip {
    pub fn new(id: impl Into<ElementId>, items: Vec<TabItem>) -> Self {
        Self {
            id: id.into(),
            items,
            on_activate: None,
            on_close: None,
            on_toggle_pin: None,
            on_detach: None,
            on_menu: None,
        }
    }

    pub fn on_activate(
        mut self,
        f: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_activate = Some(Rc::new(f));
        self
    }

    pub fn on_close(mut self, f: impl Fn(&SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    pub fn on_toggle_pin(
        mut self,
        f: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_toggle_pin = Some(Rc::new(f));
        self
    }

    pub fn on_detach(mut self, f: impl Fn(&SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_detach = Some(Rc::new(f));
        self
    }

    pub fn on_menu(mut self, f: impl Fn(&TabMenuRequest, &mut Window, &mut App) + 'static) -> Self {
        self.on_menu = Some(Rc::new(f));
        self
    }

    // -- the pure part: which items land in which run, and in what order -----

    /// The items in drawing order: the pinned run, then the rest.
    ///
    /// Stable within each run, so a caller's own order survives — and a tab just
    /// pinned joins the *end* of the pinned run rather than jumping the queue.
    pub fn ordered(&self) -> Vec<&TabItem> {
        let mut out: Vec<&TabItem> = self.items.iter().collect();
        out.sort_by_key(|i| !i.pinned);
        out
    }

    pub fn pinned_count(&self) -> usize {
        self.items.iter().filter(|i| i.pinned).count()
    }

    /// The active tab's key.
    ///
    /// Active is a flag on the item and not an index into the list, which is what
    /// makes [`Self::ordered`] safe: reordering cannot move the highlight onto a
    /// different tab, because there is no index for it to follow.
    pub fn active_key(&self) -> Option<&SharedString> {
        self.items.iter().find(|i| i.active).map(|i| &i.key)
    }

    pub fn render(self, t: Theme) -> impl IntoElement {
        let pinned = self.pinned_count();
        let mut children: Vec<AnyElement> = Vec::with_capacity(self.items.len() + 2);
        if pinned > 0 {
            children.push(pinned_label(t).into_any_element());
        }
        // A `for` loop, not `map`: the handlers are cloned per tab, and a lazy
        // adaptor would keep borrowing `self` past the point the vector is built.
        for (n, item) in self.ordered().into_iter().enumerate() {
            // The hairline the design draws between the two runs. Only between:
            // one at either end would look like a missing tab.
            if n == pinned && pinned > 0 {
                children.push(
                    div()
                        .flex_none()
                        .w(px(1.))
                        .my(space::S2)
                        .bg(t.divider)
                        .into_any_element(),
                );
            }
            children.push(self.tab(t, item));
        }
        div()
            .id(self.id)
            .flex()
            .flex_none()
            // `min_w_0` here as well as on the tabs: a horizontal scroller that is
            // itself allowed to grow past its parent takes the whole row with it,
            // which is the "it hangs off the right edge" bug this app has had four
            // times.
            .min_w_0()
            .w_full()
            .h(design::HEIGHT)
            .overflow_x_scroll()
            .bg(t.chrome)
            .border_b_1()
            .border_color(t.divider)
            .children(children)
    }

    /// One tab. Its own hover group, so hovering it reveals only its own actions:
    /// `GroupHitboxes` resolves a group name to the innermost element that claimed
    /// it, and a shared name would light up every tab's close button at once.
    fn tab(&self, t: Theme, item: &TabItem) -> AnyElement {
        let key = item.key.clone();
        let group = SharedString::from(format!("{key}-group"));
        div()
            .id(key.clone())
            .group(group.clone())
            .flex()
            .flex_none()
            .items_center()
            .gap(space::S2)
            .px(space::S3)
            .max_w(design::TAB_MAX)
            .min_w_0()
            .text_size(text::SMALL)
            .map(|d| {
                if item.active {
                    // Lifted out of the chrome onto the window ground, with the
                    // accent edge along the top. The active tab carries no right
                    // border — the raised ground already separates it.
                    d.bg(t.bg)
                        .border_t(design::ACTIVE_EDGE)
                        .border_color(t.accent)
                        .text_color(t.text)
                } else {
                    d.border_r_1()
                        .border_color(t.divider)
                        .text_color(t.text_muted())
                        .hover(|s| s.bg(t.hover()).text_color(t.text))
                }
            })
            .when(item.pinned, |d| d.child(self.pin_marker(t, item)))
            .child(
                div()
                    // Without all three the title is clipped mid-glyph instead of
                    // ellipsized: `truncate` needs the box to be allowed to shrink
                    // and to hide its overflow.
                    .min_w_0()
                    .overflow_hidden()
                    .truncate()
                    .child(item.title.clone()),
            )
            .when_some(self.on_detach.clone(), |d, f| {
                let key = key.clone();
                d.child(action(
                    t,
                    format!("{key}-detach").into(),
                    group.clone(),
                    "window",
                    move |_, window, cx| f(&key, window, cx),
                ))
            })
            .when_some(self.on_close.clone(), |d, f| {
                let key = key.clone();
                d.child(action(
                    t,
                    format!("{key}-close").into(),
                    group.clone(),
                    "close",
                    move |_, window, cx| f(&key, window, cx),
                ))
            })
            .when_some(self.on_activate.clone(), |d, f| {
                let key = key.clone();
                d.cursor_pointer()
                    .on_click(move |_, window, cx| f(&key, window, cx))
            })
            .when_some(self.on_menu.clone(), |d, f| {
                let key = key.clone();
                // `ev.position` is already in window coordinates.
                d.on_mouse_down(
                    MouseButton::Right,
                    move |ev: &MouseDownEvent, window, cx| {
                        f(
                            &TabMenuRequest {
                                key: key.clone(),
                                at: ev.position,
                            },
                            window,
                            cx,
                        )
                    },
                )
            })
            .into_any_element()
    }

    /// The pinned dot. Clickable when the caller offers a pin handler, which is
    /// the one way to unpin without going through a menu; a bare marker otherwise.
    fn pin_marker(&self, t: Theme, item: &TabItem) -> AnyElement {
        // On the active tab the marker takes the accent, elsewhere the olive
        // secondary — read off the theme rather than a constant, so the meaning
        // survives a ground switch.
        let dot = div()
            .flex_none()
            .size(design::DOT)
            .rounded(radius::PILL)
            .bg(if item.active { t.accent } else { t.accent_2 });
        let Some(f) = self.on_toggle_pin.clone() else {
            return dot.into_any_element();
        };
        let key = item.key.clone();
        div()
            .id(SharedString::from(format!("{key}-pin")))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            // Padding, not a bigger dot: six pixels is the design's mark and also
            // far too small to aim at.
            .p(space::S1)
            .rounded(radius::PILL)
            .cursor_pointer()
            .hover(|s| s.bg(t.active()))
            .child(dot)
            .on_click(move |_, window, cx| f(&key, window, cx))
            .into_any_element()
    }
}

/// The run header, in the voice the design gives its section labels: uppercase,
/// quiet, and small enough to sit under the tab titles.
fn pinned_label(t: Theme) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(space::S1)
        .pl(space::S3)
        .pr(space::S2)
        .text_size(text::KICKER)
        .text_color(t.text_meta())
        .child(icon_tinted("pin", text::KICKER, t.text_meta()))
        .child(PINNED_LABEL)
}

/// One tab action — detach, close. Reveals itself only while its own tab is
/// hovered, which is what keeps the strip quiet when nothing is being aimed at.
fn action(
    t: Theme,
    id: SharedString,
    group: SharedString,
    glyph: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .p(space::S1)
        .rounded(radius::PILL)
        // gpui has no `invisible()`, so the reveal is an opacity swap driven by
        // the tab's group.
        .opacity(0.)
        .group_hover(group, |s| s.opacity(1.))
        .hover(|s| s.bg(t.active()))
        // svg takes its colour from its own element, never from the container.
        .child(icon_tinted(glyph, text::SMALL, t.text_muted()))
        .on_click(on_click)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(items: &[(&str, bool)], active: &str) -> TabStrip {
        let items = items
            .iter()
            .map(|(key, pinned)| {
                TabItem::new(key.to_string(), key.to_string())
                    .pinned(*pinned)
                    .active(*key == active)
            })
            .collect();
        TabStrip::new("s", items)
    }

    fn order(s: &TabStrip) -> Vec<&str> {
        s.ordered().iter().map(|i| i.key.as_ref()).collect()
    }

    #[test]
    fn the_defaults_are_the_quiet_ones() {
        let s = TabStrip::new("s", vec![TabItem::new("a", "A")]);
        assert!(!s.items[0].pinned);
        assert!(!s.items[0].active);
        assert_eq!(s.active_key(), None, "no active tab is a real state");
        // No handler means no affordance, rather than an inert one.
        assert!(s.on_close.is_none() && s.on_detach.is_none());
        assert!(s.on_activate.is_none() && s.on_toggle_pin.is_none());
    }

    #[test]
    fn the_pinned_run_comes_first_and_each_run_keeps_its_order() {
        let s = strip(&[("a", false), ("b", true), ("c", false), ("d", true)], "c");
        assert_eq!(order(&s), vec!["b", "d", "a", "c"]);
        assert_eq!(s.pinned_count(), 2);
        // Stable: the two pinned tabs kept the order they were given, and so did
        // the two unpinned ones.
        assert!(s.ordered()[..2].iter().all(|i| i.pinned));
        assert!(s.ordered()[2..].iter().all(|i| !i.pinned));
    }

    #[test]
    fn nothing_pinned_is_one_run_in_the_order_it_came() {
        let s = strip(&[("a", false), ("b", false)], "a");
        assert_eq!(order(&s), vec!["a", "b"]);
        assert_eq!(s.pinned_count(), 0);

        let all = strip(&[("a", true), ("b", true)], "a");
        assert_eq!(order(&all), vec!["a", "b"]);
        assert_eq!(all.pinned_count(), 2);

        let empty = TabStrip::new("s", Vec::new());
        assert!(empty.ordered().is_empty());
        assert_eq!(empty.pinned_count(), 0);
    }

    /// The reorder is where a strip breaks: the highlight has to stay on the same
    /// *tab*, not on the same position. It cannot follow an index here because
    /// there is no index — `active` rides on the item.
    #[test]
    fn a_reorder_keeps_the_same_tab_active_not_the_same_index() {
        // The active tab is the one being pinned: it moves, and takes the
        // highlight with it.
        let mut s = strip(&[("a", false), ("b", false), ("c", false)], "c");
        assert_eq!(order(&s), vec!["a", "b", "c"]);
        s.items[2].pinned = true;
        assert_eq!(order(&s), vec!["c", "a", "b"]);
        assert_eq!(s.active_key().map(|k| k.as_ref()), Some("c"));
        assert!(s.ordered()[0].active, "and it is the one drawn as active");

        // The active tab is a bystander, pushed right by someone else's pin.
        let mut s = strip(&[("a", false), ("b", false), ("c", false)], "a");
        s.items[2].pinned = true;
        assert_eq!(s.active_key().map(|k| k.as_ref()), Some("a"));
        assert!(s.ordered()[1].active, "a is second now, and still active");

        // And unpinning, which moves the tab the other way.
        let mut s = strip(&[("a", true), ("b", false)], "b");
        s.items[0].pinned = false;
        assert_eq!(order(&s), vec!["a", "b"]);
        assert_eq!(s.active_key().map(|k| k.as_ref()), Some("b"));
    }

    #[test]
    fn a_menu_request_carries_the_tab_and_the_pointer() {
        let at = gpui::point(px(12.), px(34.));
        let req = TabMenuRequest {
            key: "a".into(),
            at,
        };
        assert_eq!(req.key.as_ref(), "a");
        assert_eq!(req.at, at);
    }
}
