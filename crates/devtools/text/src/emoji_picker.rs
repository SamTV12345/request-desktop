//! `emoji-picker`

use crate::{Args, Category, Field, Kind, Tool};

pub(crate) fn emoji_picker_ui(_: &Args) -> anyhow::Result<String> {
    anyhow::bail!(
        "needs the emoji grid panel (click an emoji to copy it, with its unicode escape and code point)"
    )
}

pub static TOOL: Tool = Tool {
    id: "emoji-picker",
    name: "Emoji picker",
    description: "Copy emojis and get their unicode and code point values.",
    category: Category::Text,
    keywords: &["emoji", "picker", "unicode", "codepoint", "smiley"],
    fields: &[Field::text("search", "Search emojis")],
    // Grid picker with click-to-copy; see EMOJIS for the searchable data.
    kind: Kind::Custom,
    run: emoji_picker_ui,
};
