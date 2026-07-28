//! `chronometer`

use crate::{Args, Category, Kind, Tool};

// The five `Kind::Custom` tools: no generic form can express a live timer, a
// webcam, a key listener, a rich-text surface or a click-to-copy grid, so `run`
// says what the bespoke panel does instead of pretending to compute something.
pub(crate) fn chronometer_ui(_: &Args) -> anyhow::Result<String> {
    anyhow::bail!("needs the live chronometer panel (Start / Stop / Reset, ticking display)")
}

pub static TOOL: Tool = Tool {
    id: "chronometer",
    name: "Chronometer",
    description: "Monitor the duration of a thing with a simple chronometer.",
    category: Category::Measurement,
    keywords: &["chronometer", "stopwatch", "timer", "duration", "time"],
    fields: &[],
    // Live wall-clock ticking; a pure fn has nothing to return.
    kind: Kind::Custom,
    run: chronometer_ui,
};
