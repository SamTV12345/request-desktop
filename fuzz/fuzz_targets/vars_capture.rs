#![no_main]
//! `capture` walks a post-response script byte by byte and indexes into it, so
//! it is the most slicing-heavy code in core. Split the input: first NUL byte
//! separates the script from the response body the script reads.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let (script, body) = match data.iter().position(|b| *b == 0) {
        Some(i) => (&data[..i], &data[i + 1..]),
        None => (data, &data[..0]),
    };
    let script = String::from_utf8_lossy(script);
    let status = body.first().copied().unwrap_or(0) as u16 + 100;
    let headers = [
        ("content-type".to_string(), "application/json".to_string()),
        ("x-fuzz".to_string(), String::from_utf8_lossy(&body[..body.len().min(16)]).into_owned()),
    ];
    let _ = rp_core::vars::capture(&script, body, status, &headers);
});
