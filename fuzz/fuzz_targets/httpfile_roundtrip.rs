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
    // For non-UTF-8 bytes we fuzz a lossy projection (`from_utf8_lossy`), so
    // strict structural equality is not a useful oracle.
    if std::str::from_utf8(data).is_err() {
        return;
    }
    if data
        .iter()
        .any(|b| matches!(*b, 0x00..=0x08 | 0x0B | 0x0C | 0x0E..=0x1F | 0x7F))
    {
        return;
    }
    assert_eq!(
        second, third,
        "normalized round-trip changed the requests\n--- input ---\n{src}\n--- first serialized ---\n{text}\n--- normalized ---\n{normalized}"
    );
});
