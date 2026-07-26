#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let src = String::from_utf8_lossy(data);
    let _ = rp_core::bru::parse_environment(&src);
});
