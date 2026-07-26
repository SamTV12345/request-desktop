//! Method + URL + headers + body + Send + response viewer.
use crate::editor::Editor;
use crate::text_input::TextInput;
use crate::theme;
use crate::theme::{control, radius, space, text};
use gpui::{
    Context, Entity, Hsla, IntoElement, ParentElement, SharedString, Styled, Window, div,
    prelude::*, transparent_black,
};
use rp_core::interp::Vars;
use rp_core::{Body, MultipartField, Request, Response, exec};
use std::sync::OnceLock;

const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/// reqwest needs a tokio reactor; gpui's executors are not one. One runtime for
/// the process, driven from gpui's background thread pool.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Runtime::new().expect("tokio runtime"))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Body,
    Headers,
}

pub struct RequestPane {
    request: Request,
    url: Entity<TextInput>,
    headers: Entity<Editor>,
    body: Entity<Editor>,
    /// Read-only, but focusable and selectable — that is what makes the
    /// response copyable.
    response_view: Entity<Editor>,
    tab: Tab,
    vars: Vars,
    response: Option<Result<Response, String>>,
    sending: bool,
}

impl RequestPane {
    pub fn new(request: Request, vars: Vars, cx: &mut Context<Self>) -> Self {
        let url = cx.new(|cx| {
            let mut i = TextInput::new(cx, "https://example.com/api");
            i.set_text(request.url.clone(), cx);
            i
        });
        cx.observe(&url, |_, _, cx| cx.notify()).detach();

        let header_text = render_headers(&request);
        let headers = cx.new(|cx| {
            let mut e = Editor::new(cx, "Name: value");
            e.set_text(header_text, cx);
            e
        });

        let body_text = render_body(&request);
        let body = cx.new(|cx| {
            let mut e = Editor::new(cx, "request body");
            e.set_text(body_text, cx);
            e
        });

        let response_view = cx.new(|cx| Editor::new(cx, "no response yet").read_only());

        Self {
            request,
            url,
            headers,
            body,
            response_view,
            tab: Tab::Body,
            vars,
            response: None,
            sending: false,
        }
    }

    pub fn last_response(&self) -> Option<&Response> {
        self.response.as_ref().and_then(|r| r.as_ref().ok())
    }

    pub fn script(&self) -> Option<&str> {
        self.request.post_response_script.as_deref()
    }

    pub fn set_vars(&mut self, vars: Vars) {
        self.vars = vars;
    }

    /// The request with the pane's edits applied, for sending or saving.
    pub fn edited(&self, cx: &gpui::App) -> Request {
        let mut req = self.request.clone();
        req.url = self.url.read(cx).text().to_string();
        req.headers = parse_headers(self.headers.read(cx).text());
        apply_body(&mut req, self.body.read(cx).text());
        req
    }

    fn send(&mut self, cx: &mut Context<Self>) {
        if self.sending {
            return;
        }
        self.sending = true;
        self.response = None;
        cx.notify();

        let req = self.edited(cx);
        let vars = self.vars.clone();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    runtime()
                        .block_on(exec::send(&req, &vars))
                        .map_err(|e| format!("{e:#}"))
                })
                .await;
            this.update(cx, |this, cx| {
                this.sending = false;
                let shown = match &result {
                    Ok(res) => render_response(res),
                    Err(e) => e.clone(),
                };
                this.response_view.update(cx, |v, cx| v.set_text(shown, cx));
                this.response = Some(result);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The seven methods as one segmented control: adjoining cells inside a
    /// single border, only the outer corners rounded.
    fn method_control(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let current = self.request.method.to_ascii_uppercase();
        let mut cells = Vec::with_capacity(METHODS.len());
        for (i, m) in METHODS.iter().enumerate() {
            let active = current == **m;
            cells.push(
                div()
                    .id(*m)
                    .flex()
                    .items_center()
                    .px(space::XS)
                    .text_size(text::SMALL)
                    .line_height(text::SMALL_LINE)
                    .whitespace_nowrap()
                    .cursor_pointer()
                    // Adjoining cells: one hairline between neighbours, none on
                    // the first, so the group reads as a single control.
                    .when(i > 0, |d| d.border_l_1().border_color(theme::BORDER))
                    .text_color(if active {
                        theme::method_color(m)
                    } else {
                        theme::TEXT_SECONDARY
                    })
                    .when(active, |d| d.bg(theme::FILL_STRONG))
                    .when(!active, |d| d.hover(|s| s.bg(theme::FILL)))
                    .child(*m)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.request.method = (*m).to_string();
                        cx.notify();
                    })),
            );
        }
        div()
            .flex()
            .flex_none()
            .h(control::MD)
            .bg(theme::SUNKEN)
            .border_1()
            .border_color(theme::BORDER)
            .rounded(radius::XS)
            .overflow_hidden()
            .children(cells)
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let sending = self.sending;
        let send_ink = if sending {
            theme::TEXT_DISABLED
        } else {
            theme::TEXT_ON_ACCENT
        };
        div()
            .flex()
            .items_center()
            .gap(space::XS)
            .child(self.method_control(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(self.url.clone()),
            )
            .child(
                div()
                    .id("send")
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(space::XXS)
                    .h(control::MD)
                    .px(space::SM)
                    .rounded(radius::XS)
                    .text_size(text::BODY)
                    .line_height(text::BODY_LINE)
                    .bg(if sending { theme::FILL } else { theme::ACCENT })
                    .text_color(send_ink)
                    // In flight the button is inert: no pointer, no hover wash.
                    .when(!sending, |d| {
                        d.cursor_pointer().hover(|s| s.bg(theme::ACCENT_HOVER))
                    })
                    .child(theme::icon_sized("send", text::BODY).text_color(send_ink))
                    .child(if sending { "Sending…" } else { "Send" })
                    .on_click(cx.listener(|this, _, _, cx| this.send(cx))),
            )
    }

    fn request_editor(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut tabs = Vec::new();
        for (label, tab) in [("Body", Tab::Body), ("Headers", Tab::Headers)] {
            let active = self.tab == tab;
            tabs.push(
                div()
                    .id(label)
                    .flex()
                    .items_center()
                    .h(control::MD)
                    .px(space::SM)
                    .text_size(text::BODY)
                    .line_height(text::BODY_LINE)
                    .cursor_pointer()
                    // The selected tab is marked by the accent underline, the
                    // rest keep an invisible one so nothing shifts.
                    .border_b_2()
                    .border_color(if active {
                        Hsla::from(theme::ACCENT)
                    } else {
                        transparent_black()
                    })
                    .text_color(if active {
                        theme::TEXT
                    } else {
                        theme::TEXT_SECONDARY
                    })
                    .when(!active, |d| d.hover(|s| s.bg(theme::FILL)))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tab = tab;
                        cx.notify();
                    })),
            );
        }
        // Only the selected editor is mounted, so the other one keeps its state
        // but costs no layout.
        let editor = match self.tab {
            Tab::Body => self.body.clone().into_any_element(),
            Tab::Headers => self.headers.clone().into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .gap(space::XS)
            .flex_1()
            .min_h(space::XXL * 3.)
            .child(
                div()
                    .flex()
                    .border_b_1()
                    .border_color(theme::DIVIDER)
                    .children(tabs),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .font_family(theme::FONT_MONO)
                    .child(editor),
            )
    }

    /// A coloured tag: the status code, or the failure marker.
    fn badge(ink: gpui::Rgba, icon: Option<&'static str>, label: SharedString) -> gpui::Div {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(space::XXS)
            .h(control::SM)
            .px(space::XS)
            .rounded(radius::XS)
            .bg(theme::FILL_STRONG)
            .border_1()
            .border_color(ink)
            .text_size(text::SMALL)
            .line_height(text::SMALL_LINE)
            .text_color(ink)
            .when_some(icon, |d, name| {
                d.child(theme::icon_sized(name, text::SMALL).text_color(ink))
            })
            .child(label)
    }

    fn response_header(&self) -> impl IntoElement + use<> {
        let mut row = div()
            .flex()
            .items_center()
            .gap(space::XS)
            .h(control::MD)
            .child(
                div()
                    .text_size(text::SMALL)
                    .line_height(text::SMALL_LINE)
                    .text_color(theme::TEXT_SECONDARY)
                    .child("Response"),
            );
        match &self.response {
            None => {
                let note = if self.sending {
                    "sending…"
                } else {
                    "no request sent yet"
                };
                row = row
                    .child(theme::icon_sized("clock", text::SMALL).text_color(theme::TEXT_TERTIARY))
                    .child(
                        div()
                            .text_size(text::SMALL)
                            .line_height(text::SMALL_LINE)
                            .text_color(theme::TEXT_TERTIARY)
                            .child(note),
                    );
            }
            Some(Err(_)) => {
                row = row.child(Self::badge(
                    theme::DANGER,
                    Some("alert"),
                    "Request failed".into(),
                ));
            }
            Some(Ok(res)) => {
                let ink = theme::status_color(res.status);
                row = row
                    .child(Self::badge(
                        ink,
                        None,
                        format!("{} {}", res.status, res.status_text).into(),
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(space::XS)
                            .text_size(text::SMALL)
                            .line_height(text::SMALL_LINE)
                            .text_color(theme::TEXT_TERTIARY)
                            .child(SharedString::from(format!("{} ms", res.duration_ms)))
                            .child(SharedString::from(format!("{} bytes", res.size_bytes))),
                    );
            }
        }
        row
    }

    fn response_section(&self) -> impl IntoElement + use<> {
        div()
            .flex()
            .flex_col()
            .gap(space::XS)
            .flex_1()
            .min_h(space::XXL * 4.)
            .child(self.response_header())
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    // Alignment carries meaning in a response body.
                    .font_family(theme::FONT_MONO)
                    .child(self.response_view.clone()),
            )
    }
}

// ------------------------------------------------------------------ text <-> model

fn render_headers(req: &Request) -> String {
    let mut lines: Vec<String> = req
        .headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect();
    // Query params are shown too, so a user can see the whole picture, but they
    // round-trip through the URL rather than this editor.
    for (k, v) in &req.query {
        lines.push(format!("# ?{k}={v}"));
    }
    lines.join("\n")
}

/// `Name: value` per line. `#` comments and blank lines are ignored, so the
/// query-param hints written by `render_headers` do not become headers.
fn parse_headers(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = t.split_once(':') {
            out.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    out
}

fn render_body(req: &Request) -> String {
    match &req.body {
        Body::None => String::new(),
        Body::Json(s) | Body::Text(s) | Body::Xml(s) | Body::Sparql(s) => s.clone(),
        Body::GraphQl { query, variables } => format!("{query}\n\n---variables---\n{variables}"),
        Body::Form(f) => f
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n"),
        Body::MultipartForm(f) => f
            .iter()
            .map(|m| match &m.file_path {
                Some(p) => format!("{}=@{p}", m.name),
                None => format!("{}={}", m.name, m.value),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Body::File { path, .. } => format!("< {path}"),
    }
}

/// Write the edited text back into the body, preserving its variant.
fn apply_body(req: &mut Request, text: &str) {
    req.body = match &req.body {
        Body::None if text.trim().is_empty() => Body::None,
        // A body typed into an empty request is treated as text; choosing JSON
        // for it would guess at a Content-Type the user did not ask for.
        Body::None => Body::Text(text.to_string()),
        Body::Json(_) => Body::Json(text.to_string()),
        Body::Text(_) => Body::Text(text.to_string()),
        Body::Xml(_) => Body::Xml(text.to_string()),
        Body::Sparql(_) => Body::Sparql(text.to_string()),
        Body::GraphQl { .. } => match text.split_once("\n\n---variables---\n") {
            Some((q, v)) => Body::GraphQl {
                query: q.to_string(),
                variables: v.trim().to_string(),
            },
            None => Body::GraphQl {
                query: text.to_string(),
                variables: String::new(),
            },
        },
        Body::Form(_) => Body::Form(parse_kv_lines(text)),
        Body::MultipartForm(_) => Body::MultipartForm(
            parse_kv_lines(text)
                .into_iter()
                .map(|(name, value)| match value.strip_prefix('@') {
                    Some(path) => MultipartField {
                        name,
                        value: String::new(),
                        file_path: Some(path.to_string()),
                        content_type: None,
                    },
                    None => MultipartField {
                        name,
                        value,
                        file_path: None,
                        content_type: None,
                    },
                })
                .collect(),
        ),
        // ponytail: a file body is a path, not content. Editing the path text
        // would need a picker to be useful, so it is left alone.
        other @ Body::File { .. } => other.clone(),
    };
}

fn parse_kv_lines(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            l.split_once('=')
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}

fn render_response(res: &Response) -> String {
    let headers = res
        .headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join("\n");
    let body = String::from_utf8_lossy(&res.body);
    let body = match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|_| body.to_string()),
        Err(_) => body.to_string(),
    };
    format!("{headers}\n\n{body}")
}

impl Render for RequestPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(space::MD)
            .size_full()
            .p(space::MD)
            .bg(theme::SURFACE)
            .text_color(theme::TEXT)
            .overflow_hidden()
            .child(self.toolbar(cx))
            .child(self.request_editor(cx))
            .child(self.response_section())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req_with(body: Body) -> Request {
        Request {
            body,
            ..Default::default()
        }
    }

    #[test]
    fn headers_round_trip_through_the_editor_text() {
        let r = Request {
            headers: vec![
                ("Accept".into(), "application/json".into()),
                ("X-Trace".into(), "abc".into()),
            ],
            query: vec![("page".into(), "1".into())],
            ..Default::default()
        };
        let text = render_headers(&r);
        // Query hints are comments and must not come back as headers.
        assert_eq!(parse_headers(&text), r.headers);
    }

    #[test]
    fn header_editing_tolerates_junk_lines() {
        let parsed = parse_headers("Accept: a\n\n# note\nnocolon\n  X-Pad : b \n");
        assert_eq!(
            parsed,
            [
                ("Accept".to_string(), "a".to_string()),
                ("X-Pad".to_string(), "b".to_string())
            ]
        );
    }

    #[test]
    fn body_editing_preserves_the_variant() {
        for original in [
            Body::Json("{}".into()),
            Body::Text("x".into()),
            Body::Xml("<a/>".into()),
            Body::Sparql("SELECT".into()),
        ] {
            let mut r = req_with(original.clone());
            apply_body(&mut r, "edited");
            assert_eq!(
                std::mem::discriminant(&r.body),
                std::mem::discriminant(&original),
                "variant changed for {original:?}"
            );
        }
    }

    #[test]
    fn graphql_body_splits_query_from_variables() {
        let r = req_with(Body::GraphQl {
            query: "query { me }".into(),
            variables: "{\"a\":1}".into(),
        });
        let text = render_body(&r);
        let mut back = r.clone();
        apply_body(&mut back, &text);
        assert_eq!(back.body, r.body);

        // Without the separator everything is the query, not a parse error.
        let mut back = r.clone();
        apply_body(&mut back, "query { only }");
        assert_eq!(
            back.body,
            Body::GraphQl {
                query: "query { only }".into(),
                variables: String::new()
            }
        );
    }

    #[test]
    fn form_and_multipart_bodies_round_trip() {
        let form = req_with(Body::Form(vec![
            ("a".into(), "1".into()),
            ("b".into(), "2".into()),
        ]));
        let mut back = form.clone();
        apply_body(&mut back, &render_body(&form));
        assert_eq!(back.body, form.body);

        let multi = req_with(Body::MultipartForm(vec![
            MultipartField {
                name: "field".into(),
                value: "v".into(),
                file_path: None,
                content_type: None,
            },
            MultipartField {
                name: "upload".into(),
                value: String::new(),
                file_path: Some("./a.bin".into()),
                content_type: None,
            },
        ]));
        let mut back = multi.clone();
        apply_body(&mut back, &render_body(&multi));
        assert_eq!(back.body, multi.body);
    }

    #[test]
    fn an_empty_body_stays_empty_but_typing_creates_text() {
        let mut r = req_with(Body::None);
        apply_body(&mut r, "   ");
        assert_eq!(r.body, Body::None);
        apply_body(&mut r, "hello");
        assert_eq!(r.body, Body::Text("hello".into()));
    }

    #[test]
    fn a_file_body_is_not_clobbered_by_its_own_display_text() {
        let original = Body::File {
            path: "./p.json".into(),
            content_type: "application/json".into(),
        };
        let mut r = req_with(original.clone());
        let shown = render_body(&r);
        apply_body(&mut r, &shown);
        assert_eq!(r.body, original);
    }

    #[test]
    fn response_rendering_pretty_prints_json_and_survives_binary() {
        let res = Response {
            status: 200,
            status_text: "OK".into(),
            headers: vec![("content-type".into(), "application/json".into())],
            body: br#"{"b":1,"a":2}"#.to_vec(),
            duration_ms: 5,
            size_bytes: 13,
        };
        let shown = render_response(&res);
        assert!(shown.contains("content-type: application/json"));
        assert!(shown.contains("\"b\": 1"), "{shown}");

        let binary = Response {
            body: vec![0xff, 0x00, 0xfe],
            ..res
        };
        // Must not panic on non-UTF-8 payloads.
        assert!(!render_response(&binary).is_empty());
    }
}
