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
//!
//! At the end of the strip the design puts a **tail**, divided off by a hairline:
//! a `+6`-style count of the tabs that did not fit, with a chevron, and a `+` for
//! a new tab. The tab run itself is `overflow:hidden` and does *not* scroll —
//! which is why the count has to be right, and why [`visible_range`] is the one
//! piece of this module worth testing.
use crate::icon::icon_tinted;
use crate::theme::{Theme, alpha};
use crate::tokens::{radius, space, text};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, Pixels, Point, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, canvas, div, prelude::*, px,
};
use std::ops::Range;
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

/// A handler for the overflow chip's chevron.
pub type TabOverflowHandler = Rc<dyn Fn(&TabOverflowRequest, &mut Window, &mut App)>;

/// A handler for something that names no tab at all — the `+`.
pub type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A handler for the measured width of the tab run.
///
/// Takes `&Pixels` for the same reason [`TabHandler`] takes `&SharedString`: it is
/// what `Context::listener` yields, so the call site is one `cx.listener`.
pub type WidthHandler = Rc<dyn Fn(&Pixels, &mut Window, &mut App)>;

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

/// A click on the overflow chip: which tabs are hidden, and where the pointer was.
///
/// The keys travel with it because only the strip knows which tabs it dropped, and
/// the position because the list it opens is an `anchored()` surface the caller has
/// to draw — the tab run is `overflow:hidden` and would clip one of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct TabOverflowRequest {
    /// In strip order, so the list reads the way the strip would have.
    pub hidden: Vec<SharedString>,
    pub at: Point<Pixels>,
}

#[derive(Clone)]
pub struct TabStrip {
    pub id: ElementId,
    pub items: Vec<TabItem>,
    pub pinned_label: SharedString,
    /// Width the tab run had during the last frame, `0` when nothing has been
    /// measured yet — an unmeasured strip draws every tab rather than guessing.
    /// See [`TabStrip::on_measure`].
    pub width: Pixels,
    pub on_activate: Option<TabHandler>,
    pub on_close: Option<TabHandler>,
    /// Wired to the pinned marker, which is the only place a pin can be undone
    /// without opening a menu.
    pub on_toggle_pin: Option<TabHandler>,
    pub on_detach: Option<TabHandler>,
    pub on_menu: Option<TabMenuHandler>,
    /// Wired to the chevron on the overflow chip. Without it the count is still
    /// drawn — it is information — but the chevron is not.
    pub on_overflow: Option<TabOverflowHandler>,
    pub on_new_tab: Option<ClickHandler>,
    /// Called with the tab run's width, once per width. Feed it back through
    /// [`TabStrip::width`]; there is no other way for the strip to learn it, since
    /// [`TabStrip::render`] gets no bounds.
    pub on_measure: Option<WidthHandler>,
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
    /// The tail's two controls are both 26-px: `height:26px` on the count chip and
    /// `width:26px;height:26px` on the `+`.
    pub const TAIL: Pixels = px(26.);
    /// `gap:2px` between them — tighter than `space::S1`, and deliberately so:
    /// the design reads them as one control, not two.
    pub const TAIL_GAP: Pixels = px(2.);
    /// The chip's own `padding:0 10px`, which is not a `--space-*` step.
    pub const CHIP_PAD: Pixels = px(10.);
    /// `gap:6px` inside the chip, between the count and the chevron.
    pub const CHIP_GAP: Pixels = px(6.);
    /// The chip's `font-size:11.5px`, half a pixel off `text::TINY`.
    pub const CHIP_TEXT: Pixels = px(11.5);
    /// Average advance of one character at [`crate::tokens::text::SMALL`].
    ///
    /// ponytail: an estimate, and the one calibration knob in [`super::wanted_width`].
    /// gpui can only measure text through a `Window`, which the fit decision does
    /// not get and must not need, or it stops being testable. Half an em is the
    /// usual average for a humanist sans; turn this if titles start being
    /// mis-priced, rather than reworking the arithmetic around it.
    pub const CHAR: Pixels = px(6.5);
}

/// Uppercase in the literal: the design applies `text-transform`, and gpui has no
/// equivalent.
const PINNED_LABEL: &str = "PINNED";

// -- the fit decision: pure, because nothing inside `render` can be tested -----

/// Roughly how wide one tab wants to be, `actions` being how many of the hover
/// icons the caller wired up.
///
/// Everything but the title is exact — the design's `padding:0 14px`, its `gap:9px`
/// and this module's own icon boxes. The title is priced at [`design::CHAR`] per
/// character; see that constant for why it cannot be measured here.
pub fn wanted_width(item: &TabItem, actions: usize) -> Pixels {
    // One `gap` plus one padded icon box, which is what both the pinned dot and
    // each hover action costs.
    let marker = space::S2 + space::S1 * 2.;
    let mut w = space::S3 * 2. + design::CHAR * item.title.chars().count() as f32;
    if item.pinned {
        w += marker + design::DOT;
    }
    w += (marker + text::SMALL) * actions as f32;
    // The tab is capped, so a very long title costs no more than the cap.
    if w > design::TAB_MAX {
        design::TAB_MAX
    } else {
        w
    }
}

/// What the pinned run's header and its trailing hairline take before a single tab
/// is drawn.
///
/// Priced with [`design::CHAR`] too, which over-estimates it — the label is set at
/// `text::KICKER`, not `text::SMALL`. Erring high is the safe direction: it hides
/// one tab too early rather than clipping one.
fn header_width() -> Pixels {
    space::S3 + space::S2 + text::KICKER + space::S1
        + design::CHAR * PINNED_LABEL.len() as f32
        // The hairline between the runs.
        + px(1.)
}

/// Which slice of the strip is drawn, given what each tab wants and which one is
/// active. Everything outside it is what the overflow chip counts.
///
/// Two rules beyond "fill until full":
///
/// * **At least one tab**, even one that does not fit. A strip that is nothing but
///   a count does not say what you are looking at.
/// * **The active tab is always in the range.** Without that, choosing a tab out of
///   the overflow list would switch the pane and leave the strip unchanged, which
///   reads as a click that did nothing.
pub fn visible_range(available: Pixels, widths: &[Pixels], active: Option<usize>) -> Range<usize> {
    if widths.is_empty() {
        return 0..0;
    }
    // Nothing measured yet — the first frame draws everything and the canvas
    // corrects it on the next one.
    if available <= px(0.) {
        return 0..widths.len();
    }
    let mut end = 0;
    let mut used = px(0.);
    while end < widths.len() && used + widths[end] <= available {
        used += widths[end];
        end += 1;
    }
    let end = end.max(1);
    let Some(active) = active.filter(|a| *a >= end && *a < widths.len()) else {
        return 0..end;
    };
    // The active tab is past the fold, so fill backwards from it instead.
    let mut start = active;
    let mut used = widths[active];
    while start > 0 && used + widths[start - 1] <= available {
        start -= 1;
        used += widths[start];
    }
    start..active + 1
}

impl TabStrip {
    pub fn new(id: impl Into<ElementId>, items: Vec<TabItem>) -> Self {
        Self {
            id: id.into(),
            items,
            pinned_label: PINNED_LABEL.into(),
            width: px(0.),
            on_activate: None,
            on_close: None,
            on_toggle_pin: None,
            on_detach: None,
            on_menu: None,
            on_overflow: None,
            on_new_tab: None,
            on_measure: None,
        }
    }

    pub fn pinned_label(mut self, label: impl Into<SharedString>) -> Self {
        self.pinned_label = label.into();
        self
    }

    pub fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
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

    pub fn on_overflow(
        mut self,
        f: impl Fn(&TabOverflowRequest, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_overflow = Some(Rc::new(f));
        self
    }

    pub fn on_new_tab(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_new_tab = Some(Rc::new(f));
        self
    }

    pub fn on_measure(mut self, f: impl Fn(&Pixels, &mut Window, &mut App) + 'static) -> Self {
        self.on_measure = Some(Rc::new(f));
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
        let id = self.id.clone();
        let pinned = self.pinned_count();
        let ordered = self.ordered();
        let actions = self.on_detach.is_some() as usize + self.on_close.is_some() as usize;
        let widths: Vec<Pixels> = ordered.iter().map(|i| wanted_width(i, actions)).collect();
        let header = if pinned > 0 { header_width() } else { px(0.) };
        let shown = visible_range(
            (self.width - header).max(px(0.)),
            &widths,
            ordered.iter().position(|i| i.active),
        );
        let hidden: Vec<SharedString> = ordered
            .iter()
            .enumerate()
            .filter(|(n, _)| !shown.contains(n))
            .map(|(_, i)| i.key.clone())
            .collect();

        let mut children: Vec<AnyElement> = Vec::with_capacity(shown.len() + 2);
        // Only while the run it heads is actually on screen: with the window
        // shifted right past the pins, a "PINNED" label over unpinned tabs lies.
        if pinned > 0 && shown.start < pinned {
            children.push(pinned_label(t, self.pinned_label.clone()).into_any_element());
        }
        // A `for` loop, not `map`: the handlers are cloned per tab, and a lazy
        // adaptor would keep borrowing `self` past the point the vector is built.
        for (n, item) in ordered.iter().enumerate() {
            if !shown.contains(&n) {
                continue;
            }
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
        // The tab run: `flex_1` so the tail keeps its width, `overflow_hidden`
        // because the design does not scroll this — the count is what says a tab
        // is missing. The `canvas` is absolute so it measures this box without
        // becoming a third item in the row.
        let run = div()
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .children(self.measure())
            .children(children);
        let tail = self.tail(t, hidden);
        div()
            .id(id)
            .flex()
            .flex_none()
            // `min_w_0` here as well as on the run and on the tabs: a row that is
            // itself allowed to grow past its parent takes the whole layout with
            // it, which is the "it hangs off the right edge" bug this app has had
            // four times.
            .min_w_0()
            .w_full()
            .h(design::HEIGHT)
            .bg(t.chrome)
            .border_b_1()
            .border_color(t.divider)
            .child(run)
            .children(tail)
    }

    /// A zero-height `canvas` that reports the tab run's width for the next frame.
    ///
    /// The established trick in this codebase (see `custom/emoji_picker.rs`, which
    /// sizes a grid the same way): a `render` gets no bounds, so the one element
    /// that does is asked instead. One frame behind during a resize drag, which is
    /// not visible. Reports only a width that changed, or every frame would ask for
    /// another one.
    fn measure(&self) -> Option<AnyElement> {
        let f = self.on_measure.clone()?;
        let known = self.width;
        Some(
            canvas(
                move |bounds, window, cx| {
                    let width = bounds.size.width;
                    if width != known {
                        f(&width, window, cx);
                    }
                },
                |_, _: (), _, _| {},
            )
            .absolute()
            .w_full()
            .h(px(0.))
            .into_any_element(),
        )
    }

    /// The strip's tail: what did not fit, and the `+`. Absent entirely when it
    /// would hold neither.
    fn tail(&self, t: Theme, hidden: Vec<SharedString>) -> Option<AnyElement> {
        if hidden.is_empty() && self.on_new_tab.is_none() {
            return None;
        }
        Some(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(design::TAIL_GAP)
                .px(space::S2)
                .border_l_1()
                .border_color(t.divider)
                .when(!hidden.is_empty(), |d| d.child(self.chip(t, hidden)))
                .when_some(self.on_new_tab.clone(), |d, f| d.child(new_tab(t, f)))
                .into_any_element(),
        )
    }

    /// The `+6` count of the tabs that did not fit.
    ///
    /// The number is drawn whether or not anything is wired up, because it is
    /// information and a tab silently missing is worse than a tab you cannot reach
    /// from here. The **chevron** is the affordance, and follows this module's rule:
    /// no handler, not drawn.
    fn chip(&self, t: Theme, hidden: Vec<SharedString>) -> AnyElement {
        // The design's own `opacity:0.8` — quieter than a tab title, louder than
        // `text_label`, and no colour role in the theme sits at 0.8.
        let ink = alpha(t.text, 0.8);
        let chip = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(design::CHIP_GAP)
            .h(design::TAIL)
            .px(design::CHIP_PAD)
            .rounded(radius::SM)
            .bg(t.hover())
            .text_size(design::CHIP_TEXT)
            .text_color(ink)
            .child(SharedString::from(format!("+{}", hidden.len())));
        let Some(f) = self.on_overflow.clone() else {
            return chip.into_any_element();
        };
        chip.id("tab-overflow")
            .cursor_pointer()
            .hover(|s| s.bg(t.active()))
            .child(icon_tinted("chevron_down", text::TINY, ink))
            // Mouse-down rather than click, for the position: the list it opens is
            // anchored at the pointer, as the tab menu is.
            .on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, window, cx| {
                f(
                    &TabOverflowRequest {
                        hidden: hidden.clone(),
                        at: ev.position,
                    },
                    window,
                    cx,
                )
            })
            .into_any_element()
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

/// The `+` at the end of the strip.
///
/// Hand-rolled rather than a [`crate::button::Button`]: the design's square is
/// 26 px on an 8-px corner, and `Size::Sm` is 28 px on a pill — overriding the
/// height, the radius and the padding is more code than the box itself.
fn new_tab(t: Theme, f: ClickHandler) -> impl IntoElement {
    div()
        .id("tab-new")
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(design::TAIL)
        .rounded(radius::SM)
        .cursor_pointer()
        .hover(|s| s.bg(t.hover()))
        // The design's `opacity:0.55` is exactly `text_muted`.
        .child(icon_tinted("plus", text::SMALL, t.text_muted()))
        .on_click(move |ev, window, cx| f(ev, window, cx))
}

/// The run header, in the voice the design gives its section labels: uppercase,
/// quiet, and small enough to sit under the tab titles.
///
/// The mark is a **star**, filled, which is what the design puts over both this run
/// and the sidebar's `Pinned` group. It was a `pin` only because there was no star
/// in the icon set.
fn pinned_label(t: Theme, label: SharedString) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(space::S1)
        .pl(space::S3)
        .pr(space::S2)
        .text_size(text::KICKER)
        .text_color(t.text_meta())
        .child(icon_tinted("star", text::KICKER, t.text_meta()))
        .child(label)
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
        assert!(s.on_overflow.is_none() && s.on_new_tab.is_none());
        // Unmeasured, which is what makes the first frame draw everything.
        assert_eq!(s.width, px(0.));
    }

    // --- what fits, and what the overflow chip counts ----------------------

    fn w(n: &[f32]) -> Vec<Pixels> {
        n.iter().copied().map(px).collect()
    }

    #[test]
    fn an_unmeasured_strip_draws_every_tab_rather_than_guessing() {
        let widths = w(&[100., 100., 100.]);
        assert_eq!(visible_range(px(0.), &widths, Some(0)), 0..3);
        // A negative width is the same non-answer.
        assert_eq!(visible_range(px(-5.), &widths, None), 0..3);
        // And an empty strip is an empty range, not a range of one.
        assert_eq!(visible_range(px(500.), &[], None), 0..0);
    }

    #[test]
    fn tabs_are_dropped_from_the_end_once_the_run_is_full() {
        let widths = w(&[100., 100., 100., 100.]);
        assert_eq!(visible_range(px(400.), &widths, Some(0)), 0..4, "exact fit");
        assert_eq!(visible_range(px(399.), &widths, Some(0)), 0..3);
        assert_eq!(visible_range(px(250.), &widths, Some(0)), 0..2);
        // Tabs are not all the same width, and the accumulation has to respect that.
        let uneven = w(&[60., 200., 60.]);
        assert_eq!(visible_range(px(260.), &uneven, Some(0)), 0..2);
        assert_eq!(visible_range(px(259.), &uneven, Some(0)), 0..1);
    }

    #[test]
    fn one_tab_is_always_drawn_even_when_it_does_not_fit() {
        let widths = w(&[210., 210.]);
        assert_eq!(visible_range(px(30.), &widths, Some(0)), 0..1);
        assert_eq!(visible_range(px(1.), &widths, None), 0..1);
    }

    /// The rule that makes the overflow list worth having: picking a tab out of it
    /// has to move the window onto that tab, or the click looks like it did nothing.
    #[test]
    fn the_window_shifts_to_keep_the_active_tab_in_view() {
        let widths = w(&[100., 100., 100., 100., 100.]);
        // Room for two. The active tab is the last one, so the window ends there.
        assert_eq!(visible_range(px(200.), &widths, Some(4)), 3..5);
        assert_eq!(visible_range(px(200.), &widths, Some(2)), 1..3);
        // Already inside the leading window: nothing shifts.
        assert_eq!(visible_range(px(200.), &widths, Some(1)), 0..2);
        // Room for one only, so the window is exactly the active tab.
        assert_eq!(visible_range(px(100.), &widths, Some(3)), 3..4);
        // A stale index — the list changed under a click — is not a panic.
        assert_eq!(visible_range(px(200.), &widths, Some(9)), 0..2);
    }

    #[test]
    fn a_title_costs_more_the_longer_it_is_and_stops_at_the_cap() {
        let short = wanted_width(&TabItem::new("a", "A"), 0);
        let long = wanted_width(&TabItem::new("a", "A much longer tab title"), 0);
        assert!(long > short, "{long:?} vs {short:?}");
        // Nothing is ever priced above the width a tab is allowed to reach.
        let huge = TabItem::new("a", "x".repeat(400));
        assert_eq!(wanted_width(&huge, 2), design::TAB_MAX);
        // The pin marker and each wired-up action cost width of their own.
        let item = TabItem::new("a", "A");
        assert!(wanted_width(&item.clone().pinned(true), 0) > wanted_width(&item, 0));
        assert!(wanted_width(&item, 2) > wanted_width(&item, 1));
        assert!(wanted_width(&item, 1) > wanted_width(&item, 0));
    }

    #[test]
    fn an_overflow_request_carries_the_hidden_tabs_and_the_pointer() {
        let at = gpui::point(px(12.), px(34.));
        let req = TabOverflowRequest {
            hidden: vec!["b".into(), "c".into()],
            at,
        };
        assert_eq!(req.hidden.len(), 2);
        assert_eq!(req.at, at);
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
