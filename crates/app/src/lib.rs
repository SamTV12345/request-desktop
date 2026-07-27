//! The part of the app that is not the UI, exposed so it can be tested from
//! outside.
//!
//! `crates/app` is primarily a binary, and a binary's modules are invisible to
//! `tests/`. That is why `tests/end_to_end.rs` used to carry its own copy of the
//! file loader: the seam it exists to test was the one thing it could not reach.
//! This library target is the whole fix — the binary now uses `session` from
//! here, so the integration test and the app run the same loader.
//!
//! Only `session` lives here. Everything else in `src/` is gpui, and gpui has no
//! test support in this workspace, so moving it out would buy nothing.
//! ponytail: one module, no `rp-app-core` crate; add modules when a test needs them.

pub mod session;
