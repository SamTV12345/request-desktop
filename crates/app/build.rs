//! Embeds the Windows resources: the app icon, so the executable shows the
//! Toolbench mark on the desktop, in the taskbar and in search — those come from
//! the *binary's* resource table, not from anything the running app can set.
//!
//! `assets/icon.ico` is generated from `icon.png` and wraps the PNG verbatim,
//! which Vista and later accept.
fn main() {
    #[cfg(windows)]
    {
        // Rerun only when the icon itself changes, not on every build.
        println!("cargo:rerun-if-changed=assets/icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Toolbench");
        res.set(
            "FileDescription",
            "Toolbench — requests, tools and time tracking",
        );
        if let Err(e) = res.compile() {
            // A missing resource compiler must not stop a build: the app runs
            // fine without its icon, and a hard failure here would block anyone
            // without the Windows SDK.
            println!("cargo:warning=could not embed the app icon: {e}");
        }
    }
}
