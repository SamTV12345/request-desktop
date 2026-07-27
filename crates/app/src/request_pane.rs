//! Method + URL + headers + body + Send + response viewer.
use gpui::{
    Context, Entity, IntoElement, ParentElement, Rgba, SharedString, Styled, Window, div,
    prelude::*,
};
use rp_core::interp::Vars;
use rp_core::{Body, MultipartField, Request, Response, exec};
use rp_ui::button::{seg_option, seg_track};
use rp_ui::icon::icon_tinted;
use rp_ui::input::Editor;
use rp_ui::input::TextInput;
use rp_ui::layout::card;
use rp_ui::theme::{
    self, FONT_BODY, FONT_HEADING, FONT_MONO, Theme, alpha, method_color, status_color,
};
use rp_ui::tokens::{control, radius, size, space, text};
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
    /// Directory of the `.http` file this request came from, for a `>> file`
    /// response redirect: a relative path in the file means relative to the file,
    /// not to wherever the process happens to have been started.
    base_dir: Option<std::path::PathBuf>,
}

impl RequestPane {
    pub fn new(
        request: Request,
        vars: Vars,
        base_dir: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
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
            base_dir,
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
        let base = self.base_dir.clone();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let sent = runtime()
                        .block_on(exec::send(&req, &vars))
                        .map_err(|e| format!("{e:#}"));
                    // The `>> file` redirect, carried out here: once per send by
                    // construction, and on the background thread, where writing a
                    // response of any size belongs. Reported either way — a write
                    // that silently did not happen is worse than none at all.
                    let note = match (&sent, &base) {
                        (Ok(res), Some(base)) => {
                            match exec::write_redirect(&req, &res.body, base) {
                                Ok(Some(path)) => Some(format!(
                                    "wrote {} bytes to {}",
                                    res.body.len(),
                                    path.display()
                                )),
                                Ok(None) => None,
                                Err(e) => Some(format!("{e:#}")),
                            }
                        }
                        _ => None,
                    };
                    (sent, note)
                })
                .await;
            let (result, note) = result;
            this.update(cx, |this, cx| {
                this.sending = false;
                let mut shown = match &result {
                    Ok(res) => render_response(res),
                    Err(e) => e.clone(),
                };
                if let Some(note) = note {
                    shown = format!("# {note}\n\n{shown}");
                }
                this.response_view.update(cx, |v, cx| v.set_text(shown, cx));
                this.response = Some(result);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The seven methods as one segmented control. `.seg` in Organic is a pill
    /// track holding pill options, so there are no hairlines between cells —
    /// the selected option is marked by its own fill and the method's colour.
    fn method_control(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let current = self.request.method.to_ascii_uppercase();
        let mut cells = Vec::with_capacity(METHODS.len());
        for m in METHODS {
            let active = current == **m;
            cells.push(
                seg_option(t, active)
                    .id(*m)
                    // The selected cell is filled in the method's own colour —
                    // GET green, DELETE red — so the choice reads at a glance and
                    // says which method it is. `bg` after `seg_option`'s, which is
                    // what makes this an override rather than a second fill.
                    .when(active, |d| {
                        d.bg(method_color(t, m)).text_color(t.on_accent())
                    })
                    .child(*m)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.request.method = (*m).to_string();
                        cx.notify();
                    })),
            );
        }
        seg_track(t).children(cells)
    }

    fn toolbar(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let sending = self.sending;
        let send_ink = if sending {
            t.text_disabled()
        } else {
            t.on_accent()
        };
        div()
            .flex()
            .items_center()
            // The seven-cell method segment must not wrap — a segmented control
            // split over two lines stops reading as one track. So the *row*
            // wraps instead: at narrow widths the URL and Send drop to a second
            // line while the segment stays whole.
            .flex_wrap()
            .gap(space::S2)
            .child(self.method_control(t, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    // A URL box squeezed to a sliver is useless; below this the
                    // row breaks rather than shrinking further.
                    .min_w(size::URL_MIN)
                    .overflow_hidden()
                    // TextInput draws its own frame; nothing added around it.
                    .child(self.url.clone()),
            )
            .child(
                div()
                    .id("send")
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(space::S1)
                    .h(control::MD)
                    .px(space::S4)
                    .rounded(radius::PILL)
                    .font_family(FONT_BODY)
                    .text_size(text::CONTROL)
                    .bg(if sending { t.hover() } else { t.accent })
                    .text_color(send_ink)
                    // In flight the button is inert: no pointer, no hover wash.
                    .when(!sending, |d| {
                        d.cursor_pointer()
                            .hover(|s| s.bg(t.accent_ramp.s400))
                            .active(|s| s.bg(t.accent_ramp.s600))
                    })
                    .child(icon_tinted("send", text::CONTROL, send_ink))
                    .child(if sending { "Sending…" } else { "Send" })
                    .on_click(cx.listener(|this, _, _, cx| this.send(cx))),
            )
    }

    fn request_editor(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut tabs = Vec::new();
        for (label, tab) in [("Body", Tab::Body), ("Headers", Tab::Headers)] {
            let active = self.tab == tab;
            tabs.push(
                seg_option(t, active)
                    .id(label)
                    // No `text_color` here: `seg_option` sets the ink for both
                    // states, and overriding it after the fact put accent text on
                    // the accent fill — the selected tab's label vanished.
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
        card(t)
            .flex_1()
            // A floor, not a height: the request and response cards share what
            // is left after the toolbar, and on a short window they shrink
            // rather than pushing each other out of the pane.
            .min_h(size::CARD_MIN_H)
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap(space::S3)
                    .child(label_h6(t, "Request"))
                    .child(seg_track(t).children(tabs)),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .font_family(FONT_MONO)
                    .child(editor),
            )
    }

    /// `.tag` — a pill with a tint of its own ink, for the status code or the
    /// failure marker.
    fn badge(ink: Rgba, icon: Option<&'static str>, label: SharedString) -> gpui::Div {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(space::S1)
            .px(space::S2)
            .py(space::S1)
            .rounded(radius::PILL)
            .bg(alpha(ink, 0.16))
            .font_family(FONT_BODY)
            .text_size(text::TINY)
            .text_color(ink)
            .when_some(icon, |d, name| d.child(icon_tinted(name, text::TINY, ink)))
            .child(label)
    }

    fn response_header(&self, t: Theme) -> impl IntoElement + use<> {
        let mut row = div()
            .flex()
            .items_center()
            // Status, timing and size are separate chips: they may wrap onto a
            // second line rather than run out of the card.
            .flex_wrap()
            .gap(space::S3)
            .child(label_h6(t, "Response"));
        match &self.response {
            None => {
                let note = if self.sending {
                    "sending…"
                } else {
                    "no request sent yet"
                };
                row = row
                    .child(icon_tinted("clock", text::TINY, t.text_meta()))
                    .child(
                        div()
                            .text_size(text::SMALL)
                            .text_color(t.text_meta())
                            .child(note),
                    );
            }
            Some(Err(_)) => {
                row = row.child(Self::badge(
                    t.danger(),
                    Some("alert"),
                    "Request failed".into(),
                ));
            }
            Some(Ok(res)) => {
                let ink = status_color(t, res.status);
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
                            .gap(space::S3)
                            // Timing and size are digits: mono so they do not
                            // jitter as they change.
                            .font_family(FONT_MONO)
                            .text_size(text::TINY)
                            .text_color(t.text_meta())
                            .child(SharedString::from(format!("{} ms", res.duration_ms)))
                            .child(SharedString::from(format!("{} bytes", res.size_bytes))),
                    );
            }
        }
        row
    }

    fn response_section(&self, t: Theme) -> impl IntoElement + use<> {
        card(t)
            .flex_1()
            .min_h(size::CARD_MIN_H)
            .child(self.response_header(t))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    // Alignment carries meaning in a response body.
                    .font_family(FONT_MONO)
                    .child(self.response_view.clone()),
            )
    }
}

/// `h6` as the system defines it, without `section_label`'s sidebar padding —
/// these labels sit inline inside a card that already has its own.
fn label_h6(t: Theme, label: &'static str) -> gpui::Div {
    div()
        .flex_none()
        .font_family(FONT_HEADING)
        .text_size(text::H6)
        .text_color(t.text_muted())
        .child(label)
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
        let t = theme::theme(cx);
        div()
            .flex()
            .flex_col()
            .gap(space::S4)
            .size_full()
            .p(space::S4)
            .bg(t.bg)
            .font_family(FONT_BODY)
            .text_size(text::BODY)
            .line_height(text::BODY_LINE)
            .text_color(t.text)
            .overflow_hidden()
            .child(self.toolbar(t, cx))
            .child(self.request_editor(t, cx))
            .child(self.response_section(t))
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
