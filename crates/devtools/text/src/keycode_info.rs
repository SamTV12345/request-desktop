//! `keycode-info`

use crate::{Args, Category, Kind, Tool};

pub(crate) fn keycode_info_ui(_: &Args) -> anyhow::Result<String> {
    anyhow::bail!(
        "needs the keycode panel: press a key and it reports key, keyCode, code, location and modifiers"
    )
}

pub static TOOL: Tool = Tool {
    id: "keycode-info",
    name: "Keycode info",
    description: "Find the keycode, code, location and modifiers of any pressed key.",
    category: Category::Development,
    keywords: &["keycode", "key", "code", "keyboard", "event", "modifier"],
    fields: &[],
    // Needs live keydown events from the host window.
    kind: Kind::Custom,
    run: keycode_info_ui,
};
