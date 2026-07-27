//! Which font families the app actually draws with.
//!
//! The design names three faces — Caprasimo, Figtree, JetBrains Mono — and a
//! machine may have none of them. This resolves each to the best installed
//! substitute **once, at startup**, and hands the result out as a `Global`. It is
//! part of the design system rather than the app because "what does a heading look
//! like" is exactly what this crate is for, and because the input widgets need the
//! answer too.
//!
//! It reports its substitutions on stderr. A window silently drawing in Arial when
//! the design says Caprasimo is a difference nobody can see and everybody argues
//! about.
use crate::theme::{FONT_BODY, FONT_HEADING, FONT_MONO};
use gpui::{App, Global, SharedString};

/// Substitutes per role, closest first, ending in a face Windows always has.
/// Nothing is downloaded and no dependency is added: the design's own face wins
/// when it is installed, otherwise the nearest shape in the same category —
/// a slab for the display face, a humanist sans for body, a mono for mono.
const HEADING_FACES: &[&str] = &[FONT_HEADING, "Rockwell", "Bookman Old Style", "Georgia"];
const BODY_FACES: &[&str] = &[FONT_BODY, "Corbel", "Candara", "Segoe UI"];
const MONO_FACES: &[&str] = &[FONT_MONO, "Cascadia Mono", "Consolas", "Courier New"];
const SCRIPT_FACES: &[&str] = &["Segoe Script", "Ink Free", "Gabriola", "Brush Script MT"];

#[derive(Clone)]
pub struct Fonts {
    pub heading: SharedString,
    pub body: SharedString,
    pub mono: SharedString,
    /// The handwriting for the signature. Falls back to the resolved heading
    /// face, never to the default sans — a "handwritten" mark rendered in body
    /// text reads as a bug, not as a signature.
    pub script: SharedString,
}
impl Global for Fonts {}
fn pick_face(installed: &[String], wanted: &[&str]) -> SharedString {
    wanted
        .iter()
        .find(|w| installed.iter().any(|i| i.eq_ignore_ascii_case(w)))
        // Nothing in the row exists: ask for the last one anyway and let the
        // platform substitute, rather than silently claiming the design's face.
        .unwrap_or(&wanted[wanted.len() - 1])
        .to_string()
        .into()
}
/// The first installed handwriting face, or `fallback` with a word about it.
fn pick_script(installed: &[String], fallback: &SharedString) -> SharedString {
    match SCRIPT_FACES
        .iter()
        .find(|w| installed.iter().any(|i| i.eq_ignore_ascii_case(w)))
    {
        Some(face) => {
            eprintln!("request-pilot: the signature is set in {face:?}");
            face.to_string().into()
        }
        None => {
            eprintln!(
                "request-pilot: no handwriting face is installed ({}) — \
                 the signature is set in {fallback:?} and will not look handwritten",
                SCRIPT_FACES.join(", ")
            );
            fallback.clone()
        }
    }
}
pub fn resolve_fonts(cx: &mut App) -> Fonts {
    let installed = cx.text_system().all_font_names();
    let heading = pick_face(&installed, HEADING_FACES);
    let fonts = Fonts {
        script: pick_script(&installed, &heading),
        heading,
        body: pick_face(&installed, BODY_FACES),
        mono: pick_face(&installed, MONO_FACES),
    };
    for (role, wanted, got) in [
        ("heading", FONT_HEADING, &fonts.heading),
        ("body", FONT_BODY, &fonts.body),
        ("mono", FONT_MONO, &fonts.mono),
    ] {
        if got.as_ref() != wanted {
            eprintln!("request-pilot: the {role} face {wanted:?} is not installed — using {got:?}");
        }
    }
    fonts
}
/// The resolved faces. Falls back to the design's names when the global is
/// missing, which only happens outside `main`.
pub fn fonts(cx: &App) -> Fonts {
    cx.try_global::<Fonts>().cloned().unwrap_or_else(|| Fonts {
        heading: FONT_HEADING.into(),
        body: FONT_BODY.into(),
        mono: FONT_MONO.into(),
        script: SCRIPT_FACES[0].into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the faces that exist may be asked for, or the app claims a typeface
    /// it is not drawing with.
    #[test]
    fn a_face_is_only_picked_when_it_is_installed() {
        let installed = vec!["Segoe UI".to_string(), "consolas".to_string()];
        assert_eq!(pick_face(&installed, BODY_FACES).as_ref(), "Segoe UI");
        // Case-insensitive, the way font names are matched.
        assert_eq!(pick_face(&installed, MONO_FACES).as_ref(), "Consolas");
        // Nothing in the row installed: the last, most generic entry stands.
        assert_eq!(
            pick_face(&installed, HEADING_FACES).as_ref(),
            HEADING_FACES[HEADING_FACES.len() - 1]
        );
        // The design's own face wins whenever it is there.
        let with_design = vec![FONT_BODY.to_string(), "Segoe UI".to_string()];
        assert_eq!(pick_face(&with_design, BODY_FACES).as_ref(), FONT_BODY);
    }
}
