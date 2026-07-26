//! Assets compiled into the binary.
//!
//! The icons are embedded with `include_bytes!` rather than read from disk, so a
//! shipped build is one self-contained file and cannot lose its icons to a
//! missing directory. gpui's `svg()` element resolves paths through this source;
//! it tints by `text_color`, which is why every icon is a stroke drawing with
//! `stroke="currentColor"` and no fills of its own.
use anyhow::Result;
use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

/// `(path, bytes)` for every embedded asset. The path is what a view passes to
/// `svg().path(...)`.
macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        &[$((concat!("icons/", $name, ".svg"),
             include_bytes!(concat!("../assets/icons/", $name, ".svg")).as_slice())),*]
    };
}

const ICONS: &[(&str, &[u8])] = icons![
    "alert",
    "calendar",
    "check",
    "chevron_left",
    "chevron_right",
    "clock",
    "close",
    "copy",
    "external",
    "file",
    "folder",
    "play",
    "plus",
    "save",
    "search",
    "send",
    "stop",
    "trash",
    "window",
    "wrench",
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        // A missing asset is `Ok(None)`, not an error: gpui asks for things like
        // window icons that this app does not ship, and an `Err` there would be
        // noise rather than information.
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_loads_and_is_a_tintable_svg() {
        for (path, _) in ICONS {
            let bytes = Assets
                .load(path)
                .unwrap()
                .unwrap_or_else(|| panic!("{path} did not load"));
            let svg = std::str::from_utf8(&bytes).unwrap_or_else(|_| panic!("{path} is not UTF-8"));
            assert!(svg.starts_with("<svg"), "{path} is not an svg: {svg}");
            assert!(
                svg.contains("viewBox"),
                "{path} has no viewBox, so it cannot scale"
            );
            // Tinting only works if the drawing defers its colour to the caller.
            assert!(
                svg.contains("currentColor"),
                "{path} hardcodes a colour, so text_color cannot tint it"
            );
            assert!(
                !svg.contains("<image"),
                "{path} embeds a raster, defeating the point"
            );
        }
    }

    #[test]
    fn a_missing_asset_is_none_rather_than_an_error() {
        assert!(Assets.load("icons/not-a-real-icon.svg").unwrap().is_none());
        assert!(Assets.load("").unwrap().is_none());
    }

    #[test]
    fn listing_is_prefix_filtered() {
        assert_eq!(Assets.list("icons/").unwrap().len(), ICONS.len());
        assert!(Assets.list("nope/").unwrap().is_empty());
        assert!(
            Assets
                .list("icons/se")
                .unwrap()
                .iter()
                .any(|p| p.contains("send"))
        );
    }

    /// The icon names in `theme.rs`'s helpers have to exist, or a view renders
    /// an invisible hole with no error.
    #[test]
    fn icon_names_used_by_the_theme_exist() {
        for name in ["send", "play", "stop", "close", "search", "clock", "trash"] {
            let path = format!("icons/{name}.svg");
            assert!(
                Assets.load(&path).unwrap().is_some(),
                "{path} is referenced but missing"
            );
        }
    }
}
