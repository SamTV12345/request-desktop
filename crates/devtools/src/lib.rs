//! Registry for the it-tools port.
//!
//! ponytail: every tool is `fields in -> text out`. That covers the ~80 of 90
//! it-tools that are pure text transforms. Tools needing a webcam, a real
//! canvas or a file picker (camera-recorder, qr-code, pdf-signature-checker,
//! html-wysiwyg-editor) do not fit and get bespoke UI later — they are marked
//! `Kind::Custom` so the UI can skip them instead of rendering a broken form.
//!
//! # Two rules learned the hard way
//!
//! The `fields in -> one text out` shape made 86 tools quick to port, and then
//! quietly mistranslated the ones it-tools renders as several panels at once.
//! A user typing text into the base64 tool while a `mode` select happened to
//! sit on `decode` got a raw Rust error, where it-tools would simply have shown
//! the encoding in the panel next to it. So:
//!
//! 1. **No direction toggle.** Check the tool's actual `.vue`, not its service
//!    module. An audit of all 86 tools found that *not one* of them is modal
//!    upstream — every apparent mode (`encryption`, OTP, base64, url-encoder,
//!    html-entities, text-to-binary, case-converter, …) is really several
//!    panels rendered at once. So compute every panel and label them, the way
//!    `hash-text` emits one line per algorithm. If you ever think you have
//!    found a genuinely modal tool, read its `.vue` twice before adding a
//!    select.
//! 2. **A failing panel must not take the whole output down.** Wrap each panel
//!    in [`section`] so one bad input degrades that panel only. Return `Err`
//!    from `run` only when nothing at all can be produced.
use std::collections::BTreeMap;

/// Render one labelled panel, keeping a failure local to it.
///
/// This is what stops a half-typed value in one panel from blanking the others.
pub fn section(label: &str, body: anyhow::Result<String>) -> String {
    match body {
        Ok(text) => format!("{label}:\n{text}"),
        Err(e) => format!("{label}:\n  ({e:#})"),
    }
}

pub type Args = BTreeMap<String, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    /// Multi-line input; the UI gives it a full editor pane.
    TextArea,
    Number,
    Bool,
    /// Fixed set of choices, offered in `options`.
    Select,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    pub default: &'static str,
    pub options: &'static [&'static str],
}

impl Field {
    pub const fn text(key: &'static str, label: &'static str) -> Self {
        Self {
            key,
            label,
            kind: FieldKind::Text,
            default: "",
            options: &[],
        }
    }
    pub const fn area(key: &'static str, label: &'static str) -> Self {
        Self {
            key,
            label,
            kind: FieldKind::TextArea,
            default: "",
            options: &[],
        }
    }
    pub const fn number(key: &'static str, label: &'static str, default: &'static str) -> Self {
        Self {
            key,
            label,
            kind: FieldKind::Number,
            default,
            options: &[],
        }
    }
    pub const fn bool(key: &'static str, label: &'static str, default: &'static str) -> Self {
        Self {
            key,
            label,
            kind: FieldKind::Bool,
            default,
            options: &[],
        }
    }
    pub const fn select(
        key: &'static str,
        label: &'static str,
        options: &'static [&'static str],
        default: &'static str,
    ) -> Self {
        Self {
            key,
            label,
            kind: FieldKind::Select,
            default,
            options,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Pure function: run on every keystroke, no side effects.
    Pure,
    /// Needs UI the generic form cannot express.
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Crypto,
    Converter,
    Web,
    Images,
    Development,
    Network,
    Math,
    Measurement,
    Text,
    Data,
}

pub struct Tool {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub category: Category,
    pub keywords: &'static [&'static str],
    pub fields: &'static [Field],
    pub kind: Kind,
    /// Missing keys must fall back to the field default, never panic —
    /// the UI calls this while the user is still typing.
    pub run: fn(&Args) -> anyhow::Result<String>,
}

/// Read `key`, falling back to the field default declared on `tool`.
pub fn arg<'a>(args: &'a Args, tool: &Tool, key: &str) -> &'a str {
    args.get(key).map(String::as_str).unwrap_or_else(|| {
        tool.fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.default)
            .unwrap_or("")
    })
}

pub mod tools;

pub fn all() -> &'static [&'static Tool] {
    static ALL: std::sync::OnceLock<Vec<&'static Tool>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut v: Vec<&'static Tool> = [
            tools::crypto::TOOLS,
            tools::data::TOOLS,
            tools::web::TOOLS,
            tools::text::TOOLS,
        ]
        .into_iter()
        .flatten()
        .collect();
        v.sort_by_key(|t| t.name);
        v
    })
}

pub fn find(id: &str) -> Option<&'static Tool> {
    all().iter().copied().find(|t| t.id == id)
}

/// Substring match over name and keywords. ponytail: no fuzzy index for 90
/// entries — add one when the list outgrows a linear scan, which it will not.
pub fn search(query: &str) -> Vec<&'static Tool> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return all().to_vec();
    }
    all()
        .iter()
        .copied()
        .filter(|t| {
            t.name.to_lowercase().contains(&q)
                || t.id.contains(&q)
                || t.keywords.iter().any(|k| k.contains(q.as_str()))
        })
        .collect()
}
