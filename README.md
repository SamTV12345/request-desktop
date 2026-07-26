# Toolbench

A desktop tool that does three jobs from one window:

- **Requests** — open, edit and send IntelliJ `.http` files and Bruno `.bru`
  collections. Several files at once, with a search across all of them, a file
  watcher that picks up outside edits, environments from
  `http-client.env.json` / `http-client.private.env.json`, and response handlers
  that carry a captured token into the next request.
- **Tools** — the it-tools set, ported: encoders, hashes, converters, generators,
  cheat sheets. 86 of them.
- **Time** — local time tracking in SQLite, with an MCP server so an agent can
  book its own hours and they appear live in the window. See
  [README-mcp.md](README-mcp.md).

Written in Rust against [gpui](https://www.gpui.rs/), Zed's UI framework, so it
starts fast and stays small.

## Building

```bash
cargo build -p request-pilot     # the app
cargo build -p rp-mcp            # the MCP server, for the stdio transport
cargo run -p request-pilot -- examples/movies.http
```

`examples/movies.http` is a test sheet against three free APIs — no keys, no
sign-up — that exercises the `.http` format end to end: file variables, globals
captured from a response, dynamic variables, `# @no-redirect`, `>> file`, and two
deliberate failures so you can see what a 404 and an unresolved variable look
like.

## Repository layout

| Path              | What                                                          |
| ----------------- | ------------------------------------------------------------- |
| `crates/core`     | `.bru` / `.http` parsers, variables, request execution         |
| `crates/devtools` | the it-tools registry                                          |
| `crates/store`    | SQLite: sessions, time tracking, key/value                     |
| `crates/app`      | the gpui application                                           |
| `crates/mcp`      | the MCP server, over stdio and Streamable HTTP                 |
| `fuzz`            | fuzz targets; its own crate, needs nightly                     |
| `scripts`         | `bru_reference` — the real Bruno parser the differential        |
|                   | test compares this one against                                 |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the test suites and the coverage
gates. The short version: the parsers are held to "tested as well as SQLite".

## History

This started as a Tauri + React client and was rewritten against gpui. The Tauri
app and its web frontend have been removed; `git log` still has them.
