# `.bru` reference oracle

Runs Bruno's own parser so `crates/core/tests/differential_bru.rs` can diff our
Rust port against it. Nothing else depends on this directory.

```sh
cd scripts/bru_reference && npm install
cd ../.. && RP_DIFFERENTIAL=1 cargo test -p rp-core --test differential_bru -- --nocapture
```

The parser is the published [`@usebruno/lang`](https://www.npmjs.com/package/@usebruno/lang)
(MIT), pinned to an exact version in `package.json`. Its `main` is plain
CommonJS — no build step, `require` it directly.

| env var | meaning |
| --- | --- |
| `RP_DIFFERENTIAL=1` | required, else the Rust test skips |
| `RP_BRU_CORPUS` | corpus root; defaults to the vendored `crates/core/tests/corpus/bru` |
| `BRU_LANG_DIR` | use a local bruno checkout's `packages/bruno-lang` instead of the pinned npm package |

Protocol: a JSON array of file paths on stdin, a JSON array of
`{ path, json }` / `{ path, error }` on stdout, same order. One process handles
the whole corpus — spawning Node per file is far slower than the parse itself.

Bumping the pinned version is a deliberate act: a new `@usebruno/lang` can
change the reference behaviour, so expect `KNOWN_DIVERGENCES` in the Rust test
to need revisiting.
