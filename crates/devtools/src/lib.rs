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

/// The inverse of [`section`]: split a tool's output back into `(label, value)`.
///
/// This exists because the labels are *in the text*, which is right for reading
/// and wrong for copying — a user pressing Copy on the base64 tool got
/// `Base64 of string:\nSGVsbG8=` when they wanted `SGVsbG8=`. The parser lives
/// next to the formatter so the two cannot drift.
///
/// A header is a line ending in `:` that either starts the output or follows a
/// blank line — exactly what `section` plus the usual `join("\n\n")` produces.
/// Output with no such header is one unlabelled section, so a single-value tool
/// needs no special case at the call site.
///
/// Reassembling the result with `section` must reproduce the input byte for
/// byte; `split_sections_are_lossless_for_every_tool` asserts that across the
/// whole registry, which is what stops a body line that happens to end in `:`
/// from being silently mistaken for a header.
pub fn split_sections(output: &str) -> Vec<(&str, &str)> {
    // Header positions first, then slice between them. The parser only ever
    // *cuts* — it never trims — so every byte of the output ends up inside some
    // section. That is the property `split_sections_loses_nothing` asserts, and
    // it is what makes a mistaken header harmless: the text is still all there,
    // just labelled oddly. Trimming for display or copying is the caller's job.
    let mut heads: Vec<(usize, usize, &str)> = Vec::new(); // (line start, line end, label)
    let mut at_boundary = true;
    let mut offset = 0usize;
    for line in output.split_inclusive('\n') {
        let text = line.trim_end_matches(['\n', '\r']);
        if at_boundary && text.len() > 1 && text.ends_with(':') {
            heads.push((offset, offset + line.len(), &text[..text.len() - 1]));
        }
        at_boundary = text.is_empty();
        offset += line.len();
    }

    let mut out = Vec::new();
    // Anything before the first header is a section of its own. Dropping it was
    // the bug the registry test caught on three tools.
    let first = heads.first().map_or(output.len(), |&(s, _, _)| s);
    if first > 0 {
        out.push(("", &output[..first]));
    }
    for (i, &(_, value_start, label)) in heads.iter().enumerate() {
        let end = heads.get(i + 1).map_or(output.len(), |&(next, _, _)| next);
        out.push((label, &output[value_start..end]));
    }
    if out.is_empty() {
        out.push(("", output));
    }
    out
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

impl Category {
    /// The heading the sidebar groups this category under.
    ///
    /// Three of these are the redesign's own words — `Tools Improved.dc.html`
    /// writes "Convert & format", "Crypto & auth" and "Network" — and the rest
    /// follow the same shape: what you would *say*, not the variant's name.
    pub const fn label(self) -> &'static str {
        match self {
            Category::Converter => "Convert & format",
            Category::Crypto => "Crypto & auth",
            Category::Network => "Network",
            Category::Web => "Web & URLs",
            Category::Images => "Images & colour",
            Category::Development => "Development",
            Category::Math => "Maths",
            Category::Measurement => "Measurement",
            Category::Text => "Text",
            Category::Data => "Data",
        }
    }

    /// The one-word form for a filter tag, where the full heading would not fit.
    ///
    /// `Tools Improved.dc.html` writes "Convert", "Crypto" and "Web" in that row
    /// while the headings below say "Convert & format" and "Crypto & auth", so the
    /// two forms are the design's, not a shortening of convenience.
    pub const fn short(self) -> &'static str {
        match self {
            Category::Converter => "Convert",
            Category::Crypto => "Crypto",
            Category::Web => "Web",
            Category::Images => "Images",
            Category::Development => "Dev",
            Category::Network => "Network",
            Category::Math => "Maths",
            Category::Measurement => "Units",
            Category::Text => "Text",
            Category::Data => "Data",
        }
    }

    /// Every category, for a filter row that should offer them all.
    pub const ALL: [Category; 10] = [
        Category::Crypto,
        Category::Converter,
        Category::Web,
        Category::Images,
        Category::Development,
        Category::Network,
        Category::Math,
        Category::Measurement,
        Category::Text,
        Category::Data,
    ];
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
