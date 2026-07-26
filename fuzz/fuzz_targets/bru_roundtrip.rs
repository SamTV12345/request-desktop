#![no_main]
//! Differential oracle: parse -> serialize -> parse must be a fixed point.
//! Stronger than "does not panic": it catches fields the serializer drops and
//! escaping the parser cannot read back.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let src = String::from_utf8_lossy(data);
    let Ok(first) = rp_core::bru::parse(&src) else { return };
    let text = rp_core::bru::serialize(&first);
    let second = match rp_core::bru::parse(&text) {
        Ok(r) => r,
        Err(e) => panic!("serialize() emitted unparseable .bru: {e}\n--- input ---\n{src}\n--- serialized ---\n{text}"),
    };
    assert_eq!(
        first, second,
        "round-trip changed the request\n--- input ---\n{src}\n--- serialized ---\n{text}"
    );
});
