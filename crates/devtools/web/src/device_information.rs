//! `device-information`

use crate::{Args, Category, Kind, Tool};
use anyhow::{Result, bail};

pub(crate) fn device_information(_args: &Args) -> Result<String> {
    bail!(
        "needs the device panel: the upstream tool reports the browser          environment, which has no meaning here, so this reports the desktop one          instead — OS, CPU, window geometry and paths"
    )
}

pub static TOOL: Tool = Tool {
    id: "device-information",
    name: "Device information",
    description: "Get information about your current device: screen size, pixel ratio, user agent.",
    category: Category::Measurement,
    keywords: &["device", "information", "screen", "viewport", "browser"],
    fields: &[],
    // The browser environment has no meaning in a desktop app; the UI must
    // read the real window/monitor itself.
    kind: Kind::Custom,
    run: device_information,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_information_is_custom() {
        let t = &TOOL;
        assert_eq!(t.kind, Kind::Custom);
        assert!((t.run)(&Args::new()).is_err());
    }
}
