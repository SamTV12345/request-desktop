//! `camera-recorder`

use crate::{Args, Category, Kind, Tool};

pub(crate) fn camera_recorder_ui(_: &Args) -> anyhow::Result<String> {
    anyhow::bail!("needs the camera panel (device pickers, live preview, screenshot and recording)")
}

pub static TOOL: Tool = Tool {
    id: "camera-recorder",
    name: "Camera recorder",
    description: "Take a picture or record a video from your webcam or camera.",
    category: Category::Images,
    keywords: &["camera", "recorder", "webcam", "video", "picture", "photo"],
    fields: &[],
    // Needs webcam capture and a media recorder.
    kind: Kind::Custom,
    run: camera_recorder_ui,
};
