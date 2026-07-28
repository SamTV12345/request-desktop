//! `html-wysiwyg-editor`

use crate::{Args, Category, Kind, Tool};

pub(crate) fn wysiwyg_ui(_: &Args) -> anyhow::Result<String> {
    anyhow::bail!("needs the rich-text editor panel, which emits the formatted HTML source")
}

pub static TOOL: Tool = Tool {
    id: "html-wysiwyg-editor",
    name: "HTML WYSIWYG editor",
    description: "Rich-text editor that generates the HTML source of the content.",
    category: Category::Web,
    keywords: &["html", "wysiwyg", "editor", "richtext", "contenteditable"],
    fields: &[],
    // Needs a contenteditable rich-text surface.
    kind: Kind::Custom,
    run: wysiwyg_ui,
};
