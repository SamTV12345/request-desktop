#![no_main]
//! Same differential oracle as bru_roundtrip, for `.http` files.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let src = String::from_utf8_lossy(data);
    let Ok(first) = rp_core::httpfile::parse(&src) else { return };
    let text = rp_core::httpfile::serialize(&first);
    let second = match rp_core::httpfile::parse(&text) {
        Ok(r) => r,
        Err(e) => panic!("serialize() emitted unparseable .http: {e}\n--- input ---\n{src}\n--- serialized ---\n{text}"),
    };
    assert_eq!(
        first, second,
        "round-trip changed the requests\n--- input ---\n{src}\n--- serialized ---\n{text}"
    );
});
