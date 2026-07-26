# Contributing

The standard for this workspace is "tested as well as SQLite": the parsers and
devtools are expected to be covered by a corpus, property and fuzz suite, and
branch coverage is the number we watch.

The workspace is five crates — `rp-core` (`crates/core`), `rp-devtools`
(`crates/devtools`), `rp-store` (`crates/store`), `request-pilot` (`crates/app`,
the gpui GUI) and `rp-mcp` (`crates/mcp`). Only `fuzz` is excluded and builds on
its own, because cargo-fuzz needs nightly and its own profile.

## Running the suites locally

```bash
# The gates, in the order CI runs them. All from the repo root.
cargo fmt --all --check                             # formatting
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p rp-core -p rp-devtools                # the real test suite
cargo build -p request-pilot                        # gpui app compiles + links
cargo deny check -A unlicensed                      # advisories, bans, licenses
```

`cargo fmt --all` (without `--check`) fixes formatting in place. Config is in
`rustfmt.toml` — stock rustfmt at `max_width = 100`, which is already what the
codebase is written to.

### Coverage

Branch coverage needs a nightly toolchain, because `--branch` is gated on
`-Z coverage-options=branch`:

```bash
cargo install cargo-llvm-cov
rustup toolchain install nightly
rustup component add llvm-tools-preview --toolchain nightly

cargo +nightly llvm-cov --branch --ignore-run-fail --no-report -p rp-core -p rp-devtools
cargo +nightly llvm-cov report --branch --summary-only
```

Collect once, render separately: passing `--summary-only` together with `--lcov`
silently truncates the lcov file, so `--no-report` then `llvm-cov report` is the
supported combination. Add `--html` to `report` for a browsable tree.

**Current measured coverage** (`rp-core` + `rp-devtools`): **75.63% branch**,
89.74% line, 89.13% region, 87.90% function, over 15,910 regions.

The weakest area by far is `crates/core/src/bru.rs` at **31.90% branch** /
50.61% line — the `.bru` parser, which is also the main fuzz target. That is the
place where new tests buy the most. `crates/devtools/src/lib.rs` is at 0% branch.

CI **reports** these numbers and uploads `lcov.info` as a build artifact; it does
not enforce a threshold. A hard gate on a codebase this young just gets disabled
the first time it is inconvenient, so the number belongs in code review instead.
Note `lcov.info` is not currently in `.gitignore` — don't commit it.

### Fuzzing

`fuzz/` is a separate nightly crate with targets for the `.bru` and `.http`
parsers. On Windows you must add `--no-include-main-msvc`; on Linux and macOS
that flag is a no-op, so the same line works everywhere.

```bash
cargo install cargo-fuzz
cargo +nightly fuzz list                                   # discover targets
cargo +nightly fuzz run bru_parse -- -max_total_time=60
```

## What CI gates on

`.github/workflows/ci.yml` runs on push to `main`, on every pull request, and
nightly at 03:17 UTC. No job depends on another, so a formatting failure never
hides a failing test — every gate reports its own verdict.

| Job | Platform | Blocks merge |
| --- | --- | --- |
| `fmt` | Linux | yes |
| `clippy` | Linux | yes — `-D warnings`, whole workspace, all targets |
| `test` | Linux, Windows, macOS | yes |
| `build-app` | Linux only | yes |
| `coverage` | Linux (nightly) | no — reports only |
| `deny` | Linux | yes |
| `fuzz-smoke` | Linux (nightly) | not on PRs — schedule/dispatch only |

The nightly run exists mostly for `deny`: it re-checks the tree against a fresh
RUSTSEC advisory database, so a newly published advisory surfaces without anyone
opening a PR.

`fuzz-smoke` runs each target discovered by `cargo fuzz list` for 60 seconds and
uploads any crash reproducers from `fuzz/artifacts/`. It is intentionally kept
off the PR path — cargo-fuzz needs nightly and a full rebuild of every target.

### Two gates are currently red, on purpose

- **`clippy`** — there are pre-existing lints in all three crates (collapsible
  ifs, `type_complexity`, `manual_is_multiple_of`, `format_in_format_args`,
  `large_enum_variant`, `field_reassign_with_default`, `manual_strip`, a manual
  `Vec::pop_if`, and a single-pattern `match`). They are deliberately **not**
  allow-listed: the gate names them so the owners fix them.
- **`test`** — the corpus and property suites are still being written and a
  handful of assertions fail. That is in-flight work, not a pipeline bug.

### What CI does not cover (deliberate tradeoff)

The `test` matrix runs **only `rp-core` and `rp-devtools`** on all three
platforms. It does not test `request-pilot`.

`request-pilot` depends on gpui, Zed's GPU UI framework, which drags in a large
platform-specific graphics stack (blade/Vulkan on Linux, Metal on macOS,
DirectX on Windows) plus xkbcommon, xcb and FreeType. Compiling it on all three
runners would roughly triple CI wall-clock time and, since the crate is a GUI
shell with no tests of its own, would buy zero extra assertions.

Instead, `request-pilot` is covered on Linux only, in two ways: `clippy`
type-checks it as part of `--workspace`, and `build-app` fully compiles **and
links** it. The linking step is the one that matters — clippy never invokes the
linker, so `build-app` is what catches a missing gpui system library.

**The gap this leaves:** a Windows- or macOS-only compile or link break in
`request-pilot` will not be caught by CI. It will surface in the release
pipeline (`.github/workflows/build.yml`) or on a developer's machine. If the app
crate grows real logic, move that logic into `rp-core`/`rp-devtools` where the
cross-platform matrix already covers it, rather than widening the gpui build.

### Linux system packages

The gpui jobs (`clippy`, `build-app`) install:

```bash
sudo apt-get install -y pkg-config libssl-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev libfreetype-dev
```

gpui 0.2 loads Wayland and Vulkan dynamically (`dlopen`/`ash`), so no
`libwayland-dev` or Vulkan SDK is needed to build — only to *run*. `xkbcommon`
and `x11rb` link normally and do need their dev packages.

`libssl-dev` is required by `rp-core`, not gpui: `reqwest`'s default TLS backend
is `native-tls`, which links OpenSSL on Linux. Windows (schannel) and macOS
(Security.framework) need nothing, which is why the `test` matrix only installs
packages on Ubuntu.

## Dependency policy

`deny.toml` gates advisories, bans, licenses and sources. Every exception is
scoped to a specific crate or advisory ID and carries a `reason`; there are no
blanket ignores. Duplicate dependency versions are reported but not gated — the
gpui, reqwest and RustCrypto trees legitimately disagree on versions we do not
control.

Three items there need a human decision and should not be left to rot:

1. **`evalexpr` is `AGPL-3.0-only`** — not dual-licensed. It is a direct
   dependency of `rp-devtools`, so it links into the shipped `request-pilot`
   binary, which on a strict reading makes the whole application AGPL. Either
   replace it with a permissively licensed expression evaluator or accept AGPL
   for the project deliberately. **Resolve before the first public release.**
2. **`serde_yml` is unsound and archived** (RUSTSEC-2025-0068). We *do* reach
   the affected serializer via `serde_yml::to_string`. Migrate to
   `serde_norway`.
3. **`rsa` 0.9 is vulnerable to the Marvin timing attack** (RUSTSEC-2023-0071)
   with no patched release available. Only used by an offline local devtool, so
   the network precondition for the attack does not apply here.

The `deny` job passes `-A unlicensed` because our own three crates declare
neither a `license` field nor `publish = false`, and cargo-deny infers "private"
solely from `publish`. Adding `publish = false` (or a real license) to the three
manifests would let that flag be dropped.
