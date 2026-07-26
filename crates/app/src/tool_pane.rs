//! Generic tool form. Every `Kind::Pure` tool renders from its `fields`
//! declaration, so porting a tool never means writing UI.
//!
//! The design's "Tool Patterns" sheet reduces all 86 tools to four body shapes,
//! and each `FieldKind` maps onto one of them:
//!
//! * **Line** — a labelled pill input. `Text` and `Number`.
//! * **Document** — a labelled full-height mono editor. `TextArea`.
//! * **Segmented** — one pill border with adjoining cells, the checked one
//!   filled with the accent. `Bool`, and a `Select` short enough to fit on a
//!   row (`SEG_MAX` options).
//! * **Radio list** — a wrapped set of dot-and-label choices, for a `Select`
//!   with too many options to segment.
use crate::editor::Editor;
use crate::reference;
use crate::text_input::TextInput;
use crate::theme::{self, Theme, control, radius, size, space, text};
use gpui::{
    ClipboardItem, Context, Entity, IntoElement, ParentElement, SharedString, Styled, Window, div,
    prelude::*,
};
use rp_devtools::{Args, Field, FieldKind, Kind, Tool};
use std::collections::BTreeMap;

/// Above this many options a segmented control stops being readable and the
/// choice becomes a radio list instead.
const SEG_MAX: usize = 4;

/// The registry's key for a field that *filters* the output rather than being an
/// operand of it. Drives both the search-box shape and [`ToolPane::is_reference`],
/// so the two cannot disagree about which fields those are.
const FILTER_KEY: &str = "query";

/// A single-line field gets a compact input; a `TextArea` gets the multi-line
/// editor, because pasting a JSON document into a one-line box is useless.
enum Input {
    Line(Entity<TextInput>),
    Area(Entity<Editor>),
}

impl Input {
    fn text(&self, cx: &gpui::App) -> String {
        match self {
            Input::Line(i) => i.read(cx).text().to_string(),
            Input::Area(e) => e.read(cx).text().to_string(),
        }
    }
}

pub struct ToolPane {
    tool: &'static Tool,
    /// Text/number/area fields, in declaration order.
    inputs: Vec<(&'static Field, Input)>,
    /// Select and bool fields, which have no text editor.
    choices: BTreeMap<&'static str, String>,
    /// Read-only editor so results can be selected and copied.
    output_view: Entity<Editor>,
    output: Result<String, String>,
    /// The args `output` was produced from.
    ///
    /// `cx.observe` fires on *any* notify from an input — a click into the field,
    /// a cursor move, a selection — not only on an edit. Re-running the tool on
    /// each of those is invisible for the 80 deterministic tools and wrong for
    /// the handful that are not: bcrypt salts randomly, so the hash changed every
    /// time you clicked anywhere in the form. Same answer for the same input, and
    /// the work is skipped as a bonus.
    computed_from: Option<Args>,
}

impl ToolPane {
    pub fn new(tool: &'static Tool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut inputs = Vec::new();
        let mut choices = BTreeMap::new();
        for f in tool.fields {
            match f.kind {
                FieldKind::Select | FieldKind::Bool => {
                    choices.insert(f.key, f.default.to_string());
                }
                FieldKind::TextArea => {
                    let editor = cx.new(|cx| {
                        let mut e = Editor::new(cx, f.label);
                        if !f.default.is_empty() {
                            e.set_text(f.default, cx);
                        }
                        e
                    });
                    cx.observe(&editor, |this, _, cx| {
                        this.recompute(cx);
                        cx.notify();
                    })
                    .detach();
                    inputs.push((f, Input::Area(editor)));
                }
                _ => {
                    let input = cx.new(|cx| {
                        let mut i = TextInput::new(cx, f.label);
                        if !f.default.is_empty() {
                            i.set_text(f.default, cx);
                        }
                        i
                    });
                    // Re-run the tool whenever any field changes.
                    cx.observe(&input, |this, _, cx| {
                        this.recompute(cx);
                        cx.notify();
                    })
                    .detach();
                    inputs.push((f, Input::Line(input)));
                }
            }
        }
        let _ = window;
        let output_view = cx.new(|cx| Editor::new(cx, "result").read_only());
        let mut pane = Self {
            tool,
            inputs,
            choices,
            output_view,
            output: Ok(String::new()),
            computed_from: None,
        };
        pane.recompute(cx);
        pane
    }

    /// Whether this tool only *reports* something — a reference table or a cheat
    /// sheet, with at most a box to filter it.
    ///
    /// Those pages had the full form treatment: a labelled input under a heading,
    /// and the content in a panel titled "Result", as though something were being
    /// computed from what you typed. Nothing is — `http-status-codes` is a list of
    /// status codes, `regex-memo` and `git-memo` are cheat sheets. So the filter
    /// becomes a search box and the panel says what it holds.
    ///
    /// Derived from the registry rather than a hand-kept list of ids here, so a
    /// reference tool added later gets the right shape without this file knowing
    /// about it. The convention it relies on: a field keyed [`FILTER_KEY`] filters
    /// output, it is not an operand.
    fn is_reference(&self) -> bool {
        match self.tool.fields {
            [] => true,
            [only] => only.key == FILTER_KEY,
            _ => false,
        }
    }

    fn args(&self, cx: &Context<Self>) -> Args {
        let mut args = Args::new();
        for (f, input) in &self.inputs {
            args.insert(f.key.to_string(), input.text(cx));
        }
        for (k, v) in &self.choices {
            args.insert(k.to_string(), v.clone());
        }
        args
    }

    fn recompute(&mut self, cx: &mut Context<Self>) {
        self.output = if self.tool.kind == Kind::Custom {
            Err("This tool needs a dedicated UI — not wired up yet.".into())
        } else {
            let args = self.args(cx);
            if self.computed_from.as_ref() == Some(&args) {
                return;
            }
            let out = (self.tool.run)(&args).map_err(|e| format!("{e:#}"));
            self.computed_from = Some(args);
            out
        };
        // Mirror it into the read-only editor so the result can be copied.
        let shown = match &self.output {
            Ok(s) => s.clone(),
            Err(e) => e.clone(),
        };
        self.output_view.update(cx, |v, cx| v.set_text(shown, cx));
    }

    /// `.field > label`: the label above every control, so all 78 tool forms
    /// share one rhythm.
    fn field_label(t: &Theme, label: &'static str) -> gpui::Div {
        div()
            .text_size(text::LABEL)
            .text_color(t.text_label())
            .child(label)
    }

    /// `.field` — the wrapper both the line and document shapes sit in.
    fn field(t: &Theme, label: &'static str) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .gap(space::S1)
            .child(Self::field_label(t, label))
    }

    /// Shapes 1 and 2: a labelled control, full height when it is a document.
    /// A filter over the output: magnifier and field, no heading above it. The
    /// same shape the emoji picker uses, because it is the same job.
    fn search_row(input: &Input, t: &Theme) -> impl IntoElement + use<> {
        let field = match input {
            Input::Line(i) => i.clone().into_any_element(),
            // A filter is never a document field, but rendering one as the editor
            // is still better than rendering nothing.
            Input::Area(e) => e.clone().into_any_element(),
        };
        div()
            .flex()
            .items_center()
            .gap(space::S2)
            .child(theme::icon_tinted("search", text::CONTROL, t.text_muted()))
            // TextInput asks for `w_full`, so it needs a flexible box of its own
            // or it sizes against the whole row.
            .child(div().flex().flex_1().min_w_0().child(field))
    }

    fn field_row(&self, f: &'static Field, input: &Input, t: &Theme) -> impl IntoElement + use<> {
        match input {
            Input::Line(i) => {
                Self::field(t, f.label).child(div().flex().min_w_0().child(i.clone()))
            }
            Input::Area(e) => Self::field(t, f.label)
                .flex_1()
                // A floor: several document fields plus the result card no
                // longer add up to more than the window.
                .min_h(size::CARD_MIN_H)
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .font_family(theme::FONT_MONO)
                        .child(e.clone()),
                ),
        }
    }

    /// Shape 3, `.seg` / `.seg-opt`: adjoining cells inside one pill-shaped
    /// border, the checked cell filled with the accent and the ground as ink.
    fn segmented(
        &self,
        f: &'static Field,
        options: &[&'static str],
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let current = self.choices.get(f.key).cloned().unwrap_or_default();
        let mut seg = div()
            .flex()
            .items_center()
            .h(control::MD)
            .p(space::S1 * 0.5)
            .rounded(radius::PILL)
            .border_1()
            .border_color(t.divider);
        for &opt in options {
            let active = current == opt;
            seg = seg.child(
                div()
                    .id(SharedString::from(format!("{}-{}", f.key, opt)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .h_full()
                    .px(space::S3)
                    .rounded(radius::PILL)
                    .text_size(text::SMALL)
                    // A segmented control never wraps; a cell whose label does
                    // not fit ellipsizes so the track keeps its shape.
                    .min_w_0()
                    .truncate()
                    .cursor_pointer()
                    .text_color(if active {
                        t.on_accent()
                    } else {
                        t.text_muted()
                    })
                    .when(active, |d| d.bg(t.accent))
                    .when(!active, |d| d.hover(|s| s.bg(t.hover()).text_color(t.text)))
                    .child(opt)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choices.insert(f.key, opt.to_string());
                        this.recompute(cx);
                        cx.notify();
                    })),
            );
        }
        seg
    }

    /// Shape 4, `.radio`: a dot and a label per option, for a choice set too
    /// wide to segment.
    fn radio_list(
        &self,
        f: &'static Field,
        options: &[&'static str],
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let current = self.choices.get(f.key).cloned().unwrap_or_default();
        let mut list = div().flex().flex_wrap().gap_x(space::S4).gap_y(space::S2);
        for &opt in options {
            let active = current == opt;
            list = list.child(
                div()
                    .id(SharedString::from(format!("{}-{}", f.key, opt)))
                    .flex()
                    .items_center()
                    .gap(space::S2)
                    .text_size(text::SMALL)
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .text_color(if active { t.text } else { t.text_muted() })
                    .hover(|s| s.text_color(t.text))
                    .child(
                        // The ring, with the accent filling its centre when
                        // checked — a nested pill, not a checkmark glyph.
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(space::S4)
                            .flex_none()
                            .rounded(radius::PILL)
                            .border_1()
                            .border_color(if active { t.accent } else { t.divider })
                            .child(
                                div()
                                    .size(space::S2)
                                    .rounded(radius::PILL)
                                    .when(active, |d| d.bg(t.accent)),
                            ),
                    )
                    .child(opt)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choices.insert(f.key, opt.to_string());
                        this.recompute(cx);
                        cx.notify();
                    })),
            );
        }
        list
    }

    /// Shape 5, `.checkbox`: a box and its label, for the one case where the
    /// choice is yes-or-no.
    ///
    /// Was a `false | true` segmented pill, which made the reader compare two
    /// words to learn one bit — and stacked nine of them down the page for the
    /// chmod calculator, whose nine bits are a 3×3 matrix. That tool now has its
    /// own panel; every other boolean field is at least a tick now.
    fn checkbox_row(
        &self,
        f: &'static Field,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let checked = self.choices.get(f.key).is_some_and(|v| v == "true");
        div()
            .id(f.key)
            .flex()
            .items_center()
            .gap(space::S2)
            .h(control::MD)
            .text_size(text::CONTROL)
            .cursor_pointer()
            .text_color(if checked { t.text } else { t.text_muted() })
            .hover(|s| s.text_color(t.text))
            .child(theme::check_box(t, checked))
            .child(div().min_w_0().truncate().child(f.label))
            .on_click(cx.listener(move |this, _, _, cx| {
                let next = if checked { "false" } else { "true" };
                this.choices.insert(f.key, next.to_string());
                this.recompute(cx);
                cx.notify();
            }))
    }

    fn choice_row(&self, f: &'static Field, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        if f.kind == FieldKind::Bool {
            // The label belongs beside the box, so this shape brings its own row
            // rather than going through `field`. Wrapped so every arm here still
            // hands back a plain `Div`.
            return div().flex().child(self.checkbox_row(f, t, cx));
        }
        let options: Vec<&'static str> = f.options.to_vec();
        let control = if options.len() <= SEG_MAX {
            self.segmented(f, &options, t, cx)
        } else {
            self.radio_list(f, &options, t, cx)
        };
        // A row wrapper, so the pill keeps its content width instead of being
        // stretched by the column it sits in.
        Self::field(t, f.label).child(div().flex().child(control))
    }

    /// `.btn-ghost` — a pill with no fill until hovered.
    fn ghost_button(
        // Not `&'static str`: the per-panel copy rows need one id each, derived
        // from their label.
        id: impl Into<gpui::ElementId>,
        icon: &'static str,
        label: &'static str,
        t: &Theme,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .flex()
            .flex_none()
            .whitespace_nowrap()
            .items_center()
            .gap(space::S1)
            .h(control::MD)
            .px(space::S3)
            .rounded(radius::PILL)
            .text_size(text::SMALL)
            .text_color(t.text_muted())
            .cursor_pointer()
            .hover(|s| s.bg(t.accent_wash()).text_color(t.accent))
            .child(theme::icon_tinted(icon, text::SMALL, t.text_muted()))
            .child(label)
    }

    /// One copyable row per labelled panel the tool produced.
    ///
    /// The labels live *in* the output text, because a tool returns one string —
    /// so pressing a single Copy handed back the label and the value together
    /// when the user wanted only the value. `split_sections` cuts it apart and
    /// each row copies only its own value. Rows appear only when the tool
    /// actually labelled something: a single-value tool has nothing to
    /// disambiguate, and "Copy all" already covers it.
    fn copyable_rows(&self, t: &Theme, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let Ok(output) = &self.output else {
            return Vec::new();
        };
        let parts = rp_devtools::split_sections(output);
        if parts.len() < 2 && parts.first().is_some_and(|(l, _)| l.is_empty()) {
            return Vec::new();
        }
        let mut rows = Vec::with_capacity(parts.len());
        for (label, value) in parts {
            // The parser deliberately does not trim; the copy does.
            let value = value.trim().to_string();
            if value.is_empty() {
                continue;
            }
            // Owned: the click closure must outlive this borrow of `self.output`.
            let shown = SharedString::from(if label.is_empty() {
                "value".to_string()
            } else {
                label.to_string()
            });
            rows.push(
                div()
                    .flex()
                    .items_center()
                    .gap(space::S2)
                    .min_w_0()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(text::SMALL)
                            .text_color(t.text_meta())
                            .child(shown.clone()),
                    )
                    .child(
                        Self::ghost_button(
                            gpui::SharedString::from(format!("copy-{shown}")),
                            "copy",
                            "Copy",
                            t,
                        )
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(value.clone()));
                        }),
                    )
                    .into_any_element(),
            );
        }
        let _ = cx;
        rows
    }

    /// A reference page, set as structured text.
    ///
    /// The tool's own filter field has already narrowed `text` if it has one;
    /// filtering the parsed blocks as well is what keeps a hit's heading with it
    /// and a matched table row under its header, which no amount of filtering
    /// lines of text can do.
    fn reference_page(
        &self,
        text: &str,
        t: &Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let blocks = reference::parse(text);
        let query = self
            .tool
            .fields
            .first()
            .filter(|f| f.key == FILTER_KEY)
            .and_then(|f| self.inputs.iter().find(|(g, _)| g.key == f.key))
            .map(|(_, input)| input.text(cx))
            .unwrap_or_default();
        let blocks = reference::filter(&blocks, &query);
        let empty = blocks.is_empty();
        div()
            .id("reference-page")
            .flex()
            .flex_col()
            .gap(space::S2)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .when(empty, |d| {
                d.child(
                    div()
                        .text_size(text::BODY)
                        .text_color(t.text_muted())
                        .child("Nothing on this page matches that search."),
                )
            })
            .children(reference::page(&blocks, t))
    }

    /// Result or error, in a card with a copy affordance. An error is styled as
    /// one, so a failure never reads as output.
    fn result_panel(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let failed = self.output.is_err();
        let ink = if failed { t.danger() } else { t.text_muted() };
        let mut title = div().flex().items_center().gap(space::S1).text_color(ink);
        if failed {
            title = title.child(theme::icon_tinted("alert", text::SMALL, ink));
        }
        title = title.child(
            div()
                .text_size(text::H6)
                .font_family(theme::FONT_HEADING)
                // "Result" claims something was computed from what you typed. On
                // a lookup table nothing was — see `is_reference`.
                .child(match (failed, self.is_reference()) {
                    (true, _) => "Error",
                    (false, true) => "Reference",
                    (false, false) => "Result",
                }),
        );

        theme::card(t)
            .flex_1()
            .min_h(size::CARD_MIN_H)
            .overflow_hidden()
            .when(failed, |d| d.bg(t.danger_wash()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .child(title)
                    .child(
                        Self::ghost_button("copy-result", "copy", "Copy all", t).on_click(
                            cx.listener(|this, _, _, cx| {
                                let text = match &this.output {
                                    Ok(s) => s.clone(),
                                    Err(e) => e.clone(),
                                };
                                cx.write_to_clipboard(ClipboardItem::new_string(text));
                            }),
                        ),
                    ),
            )
            .children(self.copyable_rows(t, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .font_family(theme::FONT_MONO)
                    .child(self.output_view.clone()),
            )
    }
}

impl Render for ToolPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme(cx);
        let mut rows = Vec::new();
        // Fields render in declaration order, so text and choice rows have to
        // be interleaved rather than grouped.
        for f in self.tool.fields {
            match f.kind {
                FieldKind::Select | FieldKind::Bool => {
                    rows.push(self.choice_row(f, &t, cx).into_any_element())
                }
                _ => {
                    if let Some((_, input)) = self.inputs.iter().find(|(g, _)| g.key == f.key) {
                        // A filter is a search box, not an operand under a
                        // heading: the label already says "search…", so repeating
                        // it above the field was chrome twice over.
                        if f.key == FILTER_KEY {
                            rows.push(Self::search_row(input, &t).into_any_element());
                        } else {
                            rows.push(self.field_row(f, input, &t).into_any_element());
                        }
                    }
                }
            }
        }

        div()
            .flex()
            .flex_col()
            .gap(space::S4)
            .size_full()
            .p(space::S6)
            .bg(t.bg)
            .text_color(t.text)
            .font_family(theme::FONT_BODY)
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(space::S1)
                    .child(
                        div()
                            .text_size(text::H4)
                            .font_family(theme::FONT_HEADING)
                            .text_color(t.text)
                            .child(self.tool.name),
                    )
                    .child(
                        div()
                            .text_size(text::BODY)
                            .line_height(text::BODY_LINE)
                            .text_color(t.text_muted())
                            .child(self.tool.description),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(space::S3)
                    .when(
                        self.inputs.iter().any(|(_, i)| matches!(i, Input::Area(_))),
                        |d| d.flex_1().min_h_0().overflow_hidden(),
                    )
                    .children(rows),
            )
            // A reference page is text to read, so it is set as one: headings,
            // prose, code and tables. Only computed output goes in the result
            // card, where "Result" and a copy button mean something.
            .map(|d| match (self.is_reference(), &self.output) {
                (true, Ok(text)) => d.child(self.reference_page(text, &t, cx)),
                _ => d.child(self.result_panel(&t, cx)),
            })
    }
}
