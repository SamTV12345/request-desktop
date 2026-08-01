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
    // For hostile/malformed multipart payloads, parse -> serialize may normalize
    // structure once. Enforce stability after that normalization step.
    let normalized = rp_core::httpfile::serialize(&second);
    let third = match rp_core::httpfile::parse(&normalized) {
        Ok(r) => r,
        Err(e) => panic!(
            "normalized serialize() emitted unparseable .http: {e}\n--- input ---\n{src}\n--- first serialized ---\n{text}\n--- normalized ---\n{normalized}"
        ),
    };
    assert_eq!(
        second, third,
        "normalized round-trip changed the requests\n--- input ---\n{src}\n--- first serialized ---\n{text}\n--- normalized ---\n{normalized}"
    );
});
