# `.bru` corpus provenance

## Where these files came from

Everything in this directory **except `malformed/`** is a verbatim copy of the
`.bru` (and accompanying `bruno.json`) files from the Bruno repository:

- Upstream project: <https://github.com/usebruno/bruno>
- Source path: `packages/bruno-tests/`
- Upstream commit: `c70c5fee93b4fac4019eb2f14fc066f444a2ce66` (committed 2026-07-24)
- Files vendored: 275 `.bru` files, 8 `bruno.json` files
- Directory structure is preserved relative to `packages/bruno-tests/`, so
  `collection/`, `collection_oauth2/`, `keycloak-*/`, `sequential_exec/` etc.
  are the upstream collection roots.

The copy is **faithful and byte-for-byte**. Nothing was reformatted, reindented,
line-ending-normalised or "fixed". Some of these files are deliberately odd
(inconsistent indentation, empty blocks, unusual multiline values) — that is the
point of vendoring them. If a file here looks wrong, do not edit it; either the
parser handles it or a test pins the current behaviour.

`malformed/` is **not** upstream. Those files are hand-written by this project to
exercise error paths; see `crates/core/tests/bru_corpus.rs` for what each one is
supposed to be wrong about.

## Why

These are the only large body of real-world `.bru` files in existence. They are
used here purely as **test data** for `rp-core`'s `.bru` parser — to assert that
every real file parses, that `parse`/`serialize` round-trip, and that the parser
never panics on mutated input. None of it is compiled into or shipped with the
application.

## License

Bruno is MIT licensed. The upstream license text (`license.md` at the root of the
Bruno repository) is reproduced in full:

```
MIT License

Copyright (c) 2022 Anoop M D, Anusree P S and Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Refreshing the corpus

Re-copy `packages/bruno-tests/**/*.bru` (plus `bruno.json`) from a checkout of
upstream Bruno, preserving structure, then update the commit hash above. Expect
`corpus_has_a_meaningful_number_of_files` and
`load_collection_over_the_vendored_tree` in `bru_corpus.rs` to need their
counts bumped.
