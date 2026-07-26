// Windows/MSVC only: libFuzzer supplies `main`, and the MSVC linker needs to be
// told to pull it out of the rlib. cargo-fuzz can add `-Clink-arg=/include:main`
// itself, but it puts it in RUSTFLAGS, which hits *every* crate in the graph —
// and `slug` (a rp-devtools dependency) is a `cdylib`, which then fails to link
// with "unresolved external symbol main". So we build with
// `--no-include-main-msvc` and add the arg here, scoped to this crate's bins.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo::rustc-link-arg-bins=/include:main");
    }
}
