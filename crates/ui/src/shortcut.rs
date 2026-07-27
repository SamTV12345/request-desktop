//! Keyboard shortcuts, spelled the way each platform spells them.
//!
//! The three platforms disagree in ways that look like nitpicking and are obvious
//! to anyone actually using them:
//!
//! * **macOS** uses symbols, no separators, in a fixed order — `⇧⌘K`, never
//!   `Cmd+Shift+K`. The order is Control, Option, Shift, Command, which is what
//!   Apple's own menus use; reordering it looks wrong even to people who could not
//!   state the rule.
//! * **Windows and Linux** spell the words and join them with `+` — `Ctrl+Shift+K`.
//! * The *primary* modifier is Command on macOS and Control elsewhere, so one
//!   [`Chord`] renders `⌘K` on one platform and `Ctrl+K` on the others. A
//!   hard-coded `⌘K` in a label is a shortcut that does not exist for most users.
//!
//! Both the **label** and the **gpui keystroke** come from the same `Chord`, which
//! is the point: a shortcut that is shown but not bound, or bound but shown wrong,
//! is the failure this module exists to make impossible.
use std::fmt::Write as _;

/// Which platform to spell a chord for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Windows,
    Linux,
}

impl Platform {
    /// The platform this build targets. Compile-time, because a binary runs on one
    /// of them and gpui registers its bindings per target the same way.
    pub const fn current() -> Self {
        #[cfg(target_os = "macos")]
        return Platform::Mac;
        #[cfg(target_os = "windows")]
        return Platform::Windows;
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        return Platform::Linux;
    }

    const fn is_mac(self) -> bool {
        matches!(self, Platform::Mac)
    }
}

/// One shortcut: its modifiers, and the key held with them.
///
/// `primary` is "the modifier you hold for an application command" — Command on
/// macOS, Control elsewhere. Calling it that rather than `cmd` or `ctrl` is what
/// keeps every call site from having to know which platform it is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Chord {
    pub primary: bool,
    pub shift: bool,
    /// Option on macOS, Alt elsewhere.
    pub alt: bool,
    /// Control *as well as* the primary modifier. Only distinct on macOS, where
    /// Control and Command are different keys; elsewhere `primary` already is
    /// Control and the two collapse into one `Ctrl`.
    pub control: bool,
    /// The key, already cased for display: `"K"`, `"F5"`, `"↵"`.
    pub key: &'static str,
}

impl Chord {
    /// `⌘K` / `Ctrl+K`.
    pub const fn primary(key: &'static str) -> Self {
        Self {
            primary: true,
            shift: false,
            alt: false,
            control: false,
            key,
        }
    }

    /// A bare key with no modifiers, like `F5`.
    pub const fn plain(key: &'static str) -> Self {
        Self {
            primary: false,
            shift: false,
            alt: false,
            control: false,
            key,
        }
    }

    pub const fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    pub const fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    /// How this chord is written on `platform`.
    pub fn label_for(self, platform: Platform) -> String {
        let mut out = String::with_capacity(8);
        if platform.is_mac() {
            // Apple's order: Control, Option, Shift, Command. No separators.
            if self.control {
                out.push('⌃');
            }
            if self.alt {
                out.push('⌥');
            }
            if self.shift {
                out.push('⇧');
            }
            if self.primary {
                out.push('⌘');
            }
            out.push_str(self.key);
            return out;
        }
        // Words joined by `+`. `primary` and `control` are the same key here, so
        // they collapse rather than printing `Ctrl+Ctrl+K`.
        if self.primary || self.control {
            out.push_str("Ctrl+");
        }
        if self.alt {
            out.push_str("Alt+");
        }
        if self.shift {
            out.push_str("Shift+");
        }
        out.push_str(self.key);
        out
    }

    /// How it is written on the platform this build targets.
    pub fn label(self) -> String {
        self.label_for(Platform::current())
    }

    /// The gpui keystroke to hand `KeyBinding::new`, for `platform`.
    pub fn keystroke_for(self, platform: Platform) -> String {
        let mut out = String::with_capacity(12);
        // On the Mac, Control and Command are separate; elsewhere `primary` is
        // Control and naming it twice would not parse.
        if self.control && platform.is_mac() {
            out.push_str("ctrl-");
        }
        if self.primary {
            let _ = write!(out, "{}-", if platform.is_mac() { "cmd" } else { "ctrl" });
        } else if self.control {
            out.push_str("ctrl-");
        }
        if self.alt {
            out.push_str("alt-");
        }
        if self.shift {
            out.push_str("shift-");
        }
        out.push_str(&self.key.to_lowercase());
        out
    }

    pub fn keystroke(self) -> String {
        self.keystroke_for(Platform::current())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Platform; 3] = [Platform::Mac, Platform::Windows, Platform::Linux];

    #[test]
    fn the_primary_modifier_follows_the_platform() {
        let k = Chord::primary("K");
        assert_eq!(k.label_for(Platform::Mac), "⌘K");
        assert_eq!(k.label_for(Platform::Windows), "Ctrl+K");
        assert_eq!(k.label_for(Platform::Linux), "Ctrl+K");
        // The binding follows it too, or what is shown is not what works.
        assert_eq!(k.keystroke_for(Platform::Mac), "cmd-k");
        assert_eq!(k.keystroke_for(Platform::Windows), "ctrl-k");
    }

    /// Apple's order is Control, Option, Shift, Command — not the order the struct
    /// declares its fields in.
    #[test]
    fn mac_uses_symbols_in_apples_order_with_no_separators() {
        assert_eq!(
            Chord::primary("K").shift().alt().label_for(Platform::Mac),
            "⌥⇧⌘K"
        );
        let all = Chord {
            control: true,
            ..Chord::primary("K").shift().alt()
        };
        assert_eq!(all.label_for(Platform::Mac), "⌃⌥⇧⌘K");
        assert!(!all.label_for(Platform::Mac).contains('+'));
    }

    #[test]
    fn windows_and_linux_spell_the_words() {
        let c = Chord::primary("K").shift().alt();
        assert_eq!(c.label_for(Platform::Windows), "Ctrl+Alt+Shift+K");
        assert_eq!(c.label_for(Platform::Linux), "Ctrl+Alt+Shift+K");
        assert_eq!(c.keystroke_for(Platform::Linux), "ctrl-alt-shift-k");
    }

    /// Off the Mac, `primary` and `control` are one physical key. `Ctrl+Ctrl+K` is
    /// the bug this collapses — and `ctrl-ctrl-k` would not even parse.
    #[test]
    fn primary_and_control_collapse_off_the_mac() {
        let both = Chord {
            control: true,
            ..Chord::primary("K")
        };
        assert_eq!(both.label_for(Platform::Windows), "Ctrl+K");
        assert_eq!(both.keystroke_for(Platform::Windows), "ctrl-k");
        // On the Mac they really are two keys.
        assert_eq!(both.label_for(Platform::Mac), "⌃⌘K");
        assert_eq!(both.keystroke_for(Platform::Mac), "ctrl-cmd-k");
    }

    #[test]
    fn a_bare_key_is_just_itself() {
        let f5 = Chord::plain("F5");
        for p in ALL {
            assert_eq!(f5.label_for(p), "F5");
            assert_eq!(f5.keystroke_for(p), "f5");
        }
    }

    /// Every combination must render on every platform. A blank label is a hint
    /// that says nothing; a keystroke with a stray or doubled `-` does not parse.
    #[test]
    fn no_combination_renders_badly_on_any_platform() {
        for primary in [false, true] {
            for shift in [false, true] {
                for alt in [false, true] {
                    for control in [false, true] {
                        let c = Chord {
                            primary,
                            shift,
                            alt,
                            control,
                            key: "K",
                        };
                        for p in ALL {
                            let label = c.label_for(p);
                            assert!(label.ends_with('K'), "{c:?} on {p:?}: {label}");
                            let ks = c.keystroke_for(p);
                            assert!(ks.ends_with('k'), "{c:?} on {p:?}: {ks}");
                            assert!(!ks.starts_with('-'), "{ks}");
                            assert!(!ks.contains("--"), "{ks}");
                            // Exactly one `Ctrl` off the Mac, however it was asked for.
                            if !p.is_mac() {
                                assert!(label.matches("Ctrl").count() <= 1, "{label}");
                            }
                        }
                    }
                }
            }
        }
    }
}
