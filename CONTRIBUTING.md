# Contributing

The standard for this workspace is "tested as well as SQLite": the parsers and
devtools are expected to be covered by a corpus, property and fuzz suite, and
branch coverage is the number we watch.

The workspace is six crates — `rp-core` (`crates/core`), `rp-devtools`
(`crates/devtools`), `rp-store` (`crates/store`), `rp-ui` (`crates/ui`, the
design system: tokens, theme and shared components, depending on gpui and
nothing else), `request-pilot` (`crates/app`, the gpui GUI) and `rp-mcp`
(`crates/mcp`). Only `fuzz` is excluded and builds on its own, because
cargo-fuzz needs nightly and its own profile.

`rp-devtools` is itself six crates. The tools used to be four files of two to
four thousand lines each, which is where they stopped being reviewable, so each
category is now its own crate under `crates/devtools/` — `common` (the types,
the panel helpers and anything two categories share), then `crypto`, `data`,
`text`, `web` and `community` — and inside a crate every tool is one file:
`crates/devtools/web/src/url_parser.rs` holds `url-parser`, its helpers, its
`pub static TOOL` and its tests. `crates/devtools` itself is now only the
lookup (`all`, `find`, `search`).

**Adding a tool** is therefore: one new file in the right category crate, one
`mod` line and one entry in that crate's `TOOLS` list, plus the pinned lists in
`crates/devtools/tests/`. Helpers that a second tool starts needing move to the
crate's `common.rs`; helpers a second *crate* needs move to `rp-devtools-common`.

`crates/app` has both a `[lib]` and a `[bin]` target. The library is one module
(`session`, the file loader) and exists so `crates/app/tests/end_to_end.rs` can
drive the app's real loader instead of a copy of it; everything else in
`crates/app/src` is gpui and stays binary-only.

## Running the suites locally

```bash
# All from the repo root.
cargo fmt --all --check                             # formatting
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                              # 656 tests
cargo build -p request-pilot                        # gpui app compiles + links
cargo deny check                                    # advisories, bans, licenses
```

All three of the first gates are green. CI's `test` job is narrower than
`--workspace` — see [What CI gates on](#what-ci-gates-on).

`cargo fmt --all` (without `--check`) fixes formatting in place. Config is in
`rustfmt.toml` — stock rustfmt at `max_width = 100`, which is already what the
codebase is written to.

### Windows: "Failed to find fxc.exe"

gpui compiles its HLSL shaders with `fxc.exe` from the Windows SDK, and its build
script looks in `PATH` and then at exactly **one hard-coded SDK version**. With any
other version installed the build panics with `Failed to find fxc.exe`. It is easy
to miss, because a cached `target/debug` keeps working and only a clean build — or
`--release`, which has its own directory — trips over it.

Point gpui at the copy you have:

```powershell
$env:GPUI_FXC_PATH = (Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' `
    -Filter fxc.exe -Recurse | Where-Object FullName -like '*\x64\*' |
    Sort-Object FullName -Descending | Select-Object -First 1).FullName
cargo build --release -p request-pilot
```

`.github/workflows/release.yml` does the same discovery, so the release job does
not depend on which SDK version a runner happens to ship.

## Releases

Push a `v*` tag. `release.yml` runs fmt, clippy and the whole test suite, builds
`request-pilot.exe` and `rp-mcp.exe`, checks that both are real binaries and that
the app's icon actually got embedded, and attaches them to a GitHub Release. A tag
that fails a gate publishes nothing.

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

**Current measured coverage** (`rp-core` + `rp-devtools`): **88.21% branch**,
96.27% line, 95.88% region, 94.01% function, over 18,828 regions. Measured on
Windows with the commands above; CI measures the same thing on Linux, so
platform-gated code can shift the last decimal.

`crates/core/src/bru.rs` used to be the hole in this table and is not any more —
94.86% branch / 98.30% line. The two weakest spots now are
`crates/core/src/exec.rs` at **75.76% branch** (the send path: timeouts, TLS and
redirect handling are awkward to reach without a server per case) and
`crates/devtools`'s registry plumbing at **60.63% line**. Those
are where new tests buy the most.

CI **reports** these numbers and uploads `lcov.info` as a build artifact; it does
not enforce a threshold. A hard gate on a codebase this young just gets disabled
the first time it is inconvenient, so the number belongs in code review instead.
`lcov.info` is in `.gitignore`, so a local collection will not follow you into a
commit.

### Fuzzing

`fuzz/` is a separate nightly crate with seven targets: `bru_parse`, `bru_env`,
`bru_roundtrip`, `httpfile_parse`, `httpfile_roundtrip`, `vars_capture` and
`tools_run`. On Windows you must add `--no-include-main-msvc`; on Linux and macOS
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

`.github/workflows/differential.yml` is a second workflow, on the same PR/push
triggers: it installs Bruno's own JS `.bru` parser from
`scripts/bru_reference` and compares our parser against it with
`RP_DIFFERENTIAL=1`. Without that variable the test in
`crates/core/tests/differential_bru.rs` skips, which is the one ignored test in a
local `cargo test --workspace`.

The nightly run exists mostly for `deny`: it re-checks the tree against a fresh
RUSTSEC advisory database, so a newly published advisory surfaces without anyone
opening a PR.

`fuzz-smoke` runs each target discovered by `cargo fuzz list` for 60 seconds and
uploads any crash reproducers from `fuzz/artifacts/`. It is intentionally kept
off the PR path — cargo-fuzz needs nightly and a full rebuild of every target.

### Every gate is green

`fmt`, `clippy` (`-D warnings`, whole workspace, all targets) and
`cargo test --workspace` (**656 tests**) all pass. There is no allow-list
hiding a lint and no failing assertion being tolerated, so any red you see is
something you changed.

### What CI does not cover (deliberate tradeoff)

The `test` matrix runs **only `rp-core` and `rp-devtools`** on all three
platforms — 336 of the workspace's 656 tests. The 320 it skips are
`request-pilot` (169), `rp-mcp` (74), `rp-ui` (45) and `rp-store` (32). The
release workflow does run `cargo test --workspace`, so a break in one of those
four blocks a tag but not a merge.

`request-pilot` depends on gpui, Zed's GPU UI framework, which drags in a large
platform-specific graphics stack (blade/Vulkan on Linux, Metal on macOS,
DirectX on Windows) plus xkbcommon, xcb and FreeType. Compiling it on all three
runners would roughly triple CI wall-clock time, which is the reason the matrix
skips it.

Instead, `request-pilot` is covered on Linux only, in two ways: `clippy`
type-checks it as part of `--workspace`, and `build-app` fully compiles **and
links** it. The linking step is the one that matters — clippy never invokes the
linker, so `build-app` is what catches a missing gpui system library.

**The gap this leaves:** a Windows- or macOS-only compile or link break in
`request-pilot` will not be caught by CI, and neither will a failure in any of
the 320 tests outside `rp-core`/`rp-devtools`. Both surface in
`.github/workflows/release.yml`, which runs the whole suite, or on a developer's
machine. The app crate is no longer the "no tests of its own" shell this section
once described — it has 169 — so widening the matrix, or moving the logic into a
crate the matrix already covers, is a real decision and not a formality.

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

One item there needs a human decision. The other two this section used to list
have been resolved:

1. **`rsa` 0.9 is vulnerable to the Marvin timing attack** (RUSTSEC-2023-0071)
   with no patched release available. It is the one entry in `deny.toml`'s
   `advisories.ignore`. Only used by an offline local devtool, so the network
   precondition for the attack does not apply here. Revisit when `rsa` 0.10
   ships.
2. ~~`evalexpr` is `AGPL-3.0-only`~~ — gone. Nothing in the workspace depends on
   it, `deny.toml` has no license exceptions, and `cargo deny check licenses`
   passes against an allow-list that does not include AGPL.
3. ~~`serde_yml` is unsound and archived~~ (RUSTSEC-2025-0068) — migrated;
   `rp-devtools` uses `serde_norway` 0.9.

The `deny` job still passes `-A unlicensed`, and no longer needs to: all six
manifests now carry `publish = false`, which is what cargo-deny infers "private"
from, and `cargo deny check` with no flags reports `advisories ok, bans ok,
licenses ok, sources ok`. The flag and the comment above it in `ci.yml` can be
dropped.
