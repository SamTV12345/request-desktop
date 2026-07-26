#![no_main]
//! 85 devtools, each `run` called on every keystroke in the UI, so a panic in
//! any of them is a crash of the whole app. Byte 0 picks the tool, byte 1 picks
//! which field gets the remaining bytes; the other fields keep their defaults,
//! which is exactly what the UI passes for an untouched form.
use libfuzzer_sys::fuzz_target;
use rp_devtools::Args;

/// Deliberately expensive tools: bcrypt is a work-factor KDF and RSA keygen
/// takes seconds. Fuzzing them just burns the budget on their own slowness
/// instead of exploring parsers, and libFuzzer reports them as timeouts.
const SLOW: &[&str] = &["bcrypt", "rsa-key-pair-generator"];

fuzz_target!(|data: &[u8]| {
    let Some((&pick, rest)) = data.split_first() else { return };
    let tools = rp_devtools::all();
    let tool = tools[pick as usize % tools.len()];
    if SLOW.contains(&tool.id) {
        return;
    }
    let (field_pick, rest) = rest.split_first().map(|(b, r)| (*b, r)).unwrap_or((0, rest));

    let mut args = Args::new();
    for f in tool.fields {
        args.insert(f.key.to_string(), f.default.to_string());
    }
    if !tool.fields.is_empty() {
        let f = &tool.fields[field_pick as usize % tool.fields.len()];
        args.insert(f.key.to_string(), String::from_utf8_lossy(rest).into_owned());
    }
    // Ok or Err, never a panic.
    let _ = (tool.run)(&args);
});
