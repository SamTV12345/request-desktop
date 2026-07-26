#![no_main]
//! `.bru` parser must never panic on hostile input — a collection is just files
//! on disk, and a bad one must not take the app down.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let src = String::from_utf8_lossy(data);
    let _ = rp_core::bru::parse(&src);
});
