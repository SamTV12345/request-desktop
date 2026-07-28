//! `mime-types`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg, section};
use anyhow::{Context, Result, bail};

pub(crate) fn mime_to_extensions(mime: &str) -> Result<String> {
    if mime.is_empty() {
        bail!("nothing to look up");
    }
    let exts = mime_guess::get_mime_extensions_str(mime).context("unknown mime type")?;
    let list = exts
        .iter()
        .map(|e| format!(".{e}"))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(format!("  Extensions of {mime} files: {list}\n"))
}

pub(crate) fn extension_to_mime(ext: &str) -> Result<String> {
    if ext.is_empty() {
        bail!("nothing to look up");
    }
    let m = mime_guess::from_ext(ext)
        .first()
        .context("unknown extension")?;
    Ok(format!("  Mime type of .{ext} files: {m}\n"))
}

/// Mime type for a bare file extension (no leading dot), or `None` if
/// `mime_guess` does not know it.
///
/// Thin accessor over the same table the `mime-types` tool formats. The app's
/// base64-file pane consumes it so it does not need its own mime list.
pub fn mime_for_extension(ext: &str) -> Option<&'static str> {
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    // `.ts` is ambiguous and `mime_guess` resolves it to the MPEG transport
    // stream. In a developer tool it is TypeScript essentially always, so this
    // one deliberately overrides the table rather than falling back to it.
    if ext == "ts" {
        return Some("text/typescript");
    }
    mime_guess::from_ext(&ext)
        .first_raw()
        .or(match ext.as_str() {
            // Extensions the table has no entry for at all. Without these they
            // would be labelled opaque binary, which mislabels plain text.
            "lock" | "bru" | "http" | "tsx" | "cjs" => Some("text/plain"),
            "jsonc" => Some("application/json"),
            _ => None,
        })
}

/// Extensions (no leading dot) known for a mime type, most canonical first.
///
/// Same table, other direction. The app's base64-file pane uses it to suggest a
/// file name when saving a decoded `data:` URI.
pub fn extensions_for_mime(mime: &str) -> Vec<&'static str> {
    mime_guess::get_mime_extensions_str(mime)
        .unwrap_or(&[])
        .to_vec()
}

/// it-tools has one card per direction, both live at once — no mode toggle.
pub(crate) fn mime_types(args: &Args) -> Result<String> {
    let t = tool("mime-types");
    Ok(format!(
        "{}{}",
        section(
            "Mime type to extension",
            mime_to_extensions(arg(args, t, "mime_type").trim())
        ),
        section(
            "File extension to mime type",
            extension_to_mime(arg(args, t, "extension").trim().trim_start_matches('.'))
        ),
    ))
}

pub static TOOL: Tool = Tool {
    id: "mime-types",
    name: "Mime types",
    description: "Convert file extensions to mime types and the other way around.",
    category: Category::Web,
    keywords: &["mime", "types", "extension", "content", "type"],
    fields: &[
        Field::text("mime_type", "Mime type (ex: application/pdf)"),
        Field::text("extension", "File extension (ex: pdf)"),
    ],
    kind: Kind::Pure,
    run: mime_types,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn mime_accessors_answer_both_directions() {
        assert_eq!(mime_for_extension("pdf"), Some("application/pdf"));
        assert_eq!(mime_for_extension("png"), Some("image/png"));
        assert_eq!(mime_for_extension("nope-not-an-ext"), None);
        assert_eq!(mime_for_extension(""), None);
        // Extensions the table has no entry for at all.
        assert_eq!(mime_for_extension("bru"), Some("text/plain"));
        assert_eq!(mime_for_extension("http"), Some("text/plain"));
        assert_eq!(mime_for_extension("lock"), Some("text/plain"));
        assert_eq!(mime_for_extension("jsonc"), Some("application/json"));
        // The table wins wherever it has an answer, including these, which it
        // does know — no override needed.
        assert_eq!(mime_for_extension("toml"), Some("text/x-toml"));
        assert_eq!(mime_for_extension("rs"), Some("text/x-rust"));
        assert_eq!(mime_for_extension("json"), Some("application/json"));
        // `.ts` is the one deliberate override: the table says MPEG-TS video.
        assert_eq!(mime_for_extension("ts"), Some("text/typescript"));
        assert_eq!(mime_for_extension(".TS"), Some("text/typescript"));
        // Leading dots and case are accepted everywhere.
        assert_eq!(mime_for_extension(".PNG"), Some("image/png"));
        assert!(extensions_for_mime("application/pdf").contains(&"pdf"));
        assert!(extensions_for_mime("nope-not-a-type").is_empty());
    }

    #[test]
    fn mime_types_both_directions() {
        let out = run(
            "mime-types",
            &[("mime_type", "application/pdf"), ("extension", "pdf")],
        )
        .unwrap();
        assert!(
            out.contains("Mime type of .pdf files: application/pdf"),
            "{out}"
        );
        assert!(
            out.contains("Extensions of application/pdf files: .pdf"),
            "{out}"
        );
        // One unknown side leaves the other side intact.
        let out = run(
            "mime-types",
            &[("mime_type", "nope-not-a-type"), ("extension", "pdf")],
        )
        .unwrap();
        assert!(out.contains("unknown mime type"), "{out}");
        assert!(out.contains("application/pdf"), "{out}");
    }
}
