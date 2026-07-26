//! Generic tool form. Every `Kind::Pure` tool renders from its `fields`
//! declaration, so porting a tool never means writing UI.
use crate::editor::Editor;
use crate::text_input::TextInput;
use crate::theme;
use crate::theme::{control, radius, space, text};
use gpui::{
    ClipboardItem, Context, Entity, IntoElement, ParentElement, SharedString, Styled, Window, div,
    prelude::*,
};
use rp_devtools::{Args, Field, FieldKind, Kind, Tool};
use std::collections::BTreeMap;

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
        };
        pane.recompute(cx);
        pane
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
            (self.tool.run)(&args).map_err(|e| format!("{e:#}"))
        };
        // Mirror it into the read-only editor so the result can be copied.
        let shown = match &self.output {
            Ok(s) => s.clone(),
            Err(e) => e.clone(),
        };
        self.output_view.update(cx, |v, cx| v.set_text(shown, cx));
    }

    /// The label above every field, so all 78 tool forms share one rhythm.
    fn field_label(label: &'static str) -> gpui::Div {
        div()
            .text_size(text::SMALL)
            .line_height(text::SMALL_LINE)
            .text_color(theme::TEXT_SECONDARY)
            .child(label)
    }

    fn field_row(&self, f: &'static Field, input: &Input) -> impl IntoElement + use<> {
        let (element, grow) = match input {
            Input::Line(i) => (i.clone().into_any_element(), false),
            Input::Area(e) => (e.clone().into_any_element(), true),
        };
        div()
            .flex()
            .flex_col()
            .gap(space::XXS)
            .when(grow, |d| d.flex_1().min_h(space::XXL * 2.5))
            .child(Self::field_label(f.label))
            .child(if grow {
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .font_family(theme::FONT_MONO)
                    .child(element)
            } else {
                div().flex().child(element)
            })
    }

    fn choice_row(&self, f: &'static Field, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let current = self.choices.get(f.key).cloned().unwrap_or_default();
        let options: Vec<&'static str> = match f.kind {
            FieldKind::Bool => vec!["false", "true"],
            _ => f.options.to_vec(),
        };
        let mut chips = Vec::with_capacity(options.len());
        for opt in options {
            let active = current == opt;
            chips.push(
                div()
                    .id(SharedString::from(format!("{}-{}", f.key, opt)))
                    .flex()
                    .items_center()
                    .h(control::SM)
                    .px(space::XS)
                    .rounded(radius::XS)
                    .border_1()
                    .text_size(text::SMALL)
                    .line_height(text::SMALL_LINE)
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .border_color(if active { theme::ACCENT } else { theme::BORDER })
                    .text_color(if active {
                        theme::ACCENT
                    } else {
                        theme::TEXT_SECONDARY
                    })
                    .when(active, |d| d.bg(theme::ACCENT_WASH))
                    .when(!active, |d| {
                        d.hover(|s| s.bg(theme::FILL).text_color(theme::TEXT))
                    })
                    .child(opt)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choices.insert(f.key, opt.to_string());
                        this.recompute(cx);
                        cx.notify();
                    })),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(space::XXS)
            .child(Self::field_label(f.label))
            .child(div().flex().flex_wrap().gap(space::XXS).children(chips))
    }

    /// Result or error, in a card with a copy affordance. An error is styled as
    /// one, so a failure never reads as output.
    fn result_panel(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let failed = self.output.is_err();
        let ink = if failed {
            theme::DANGER
        } else {
            theme::TEXT_SECONDARY
        };
        let mut title = div().flex().items_center().gap(space::XXS).text_color(ink);
        if failed {
            title = title.child(theme::icon_sized("alert", text::SMALL).text_color(ink));
        }
        title = title.child(
            div()
                .text_size(text::SMALL)
                .line_height(text::SMALL_LINE)
                .child(if failed { "Error" } else { "Result" }),
        );

        theme::card()
            .flex()
            .flex_col()
            .gap(space::XS)
            .flex_1()
            .min_h(space::XXL * 3.)
            .overflow_hidden()
            .when(failed, |d| d.border_color(theme::DANGER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(title)
                    .child(
                        div()
                            .id("copy-result")
                            .flex()
                            .items_center()
                            .gap(space::XXS)
                            .h(control::SM)
                            .px(space::XS)
                            .rounded(radius::XS)
                            .text_size(text::SMALL)
                            .line_height(text::SMALL_LINE)
                            .text_color(theme::TEXT_SECONDARY)
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::FILL).text_color(theme::TEXT))
                            .child(
                                theme::icon_sized("copy", text::SMALL)
                                    .text_color(theme::TEXT_SECONDARY),
                            )
                            .child("Copy")
                            .on_click(cx.listener(|this, _, _, cx| {
                                let text = match &this.output {
                                    Ok(s) => s.clone(),
                                    Err(e) => e.clone(),
                                };
                                cx.write_to_clipboard(ClipboardItem::new_string(text));
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .font_family(theme::FONT_MONO)
                    .child(self.output_view.clone()),
            )
    }
}

impl Render for ToolPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows = Vec::new();
        // Fields render in declaration order, so text and choice rows have to
        // be interleaved rather than grouped.
        for f in self.tool.fields {
            match f.kind {
                FieldKind::Select | FieldKind::Bool => {
                    rows.push(self.choice_row(f, cx).into_any_element())
                }
                _ => {
                    if let Some((_, input)) = self.inputs.iter().find(|(g, _)| g.key == f.key) {
                        rows.push(self.field_row(f, input).into_any_element());
                    }
                }
            }
        }

        div()
            .flex()
            .flex_col()
            .gap(space::MD)
            .size_full()
            .p(space::MD)
            .bg(theme::SURFACE)
            .text_color(theme::TEXT)
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(space::XXS)
                    .child(
                        div()
                            .text_size(text::H5)
                            .line_height(text::H5_LINE)
                            .text_color(theme::TEXT)
                            .child(self.tool.name),
                    )
                    .child(
                        div()
                            .text_size(text::SMALL)
                            .line_height(text::SMALL_LINE)
                            .text_color(theme::TEXT_SECONDARY)
                            .child(self.tool.description),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(space::SM)
                    .when(
                        self.inputs.iter().any(|(_, i)| matches!(i, Input::Area(_))),
                        |d| d.flex_1().overflow_hidden(),
                    )
                    .children(rows),
            )
            .child(self.result_panel(cx))
    }
}
