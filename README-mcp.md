# rp-mcp — book your agent's time into the desktop app

`rp-mcp` is an [MCP](https://modelcontextprotocol.io) server that gives an AI agent
nine tools for the time tracker inside Request Pilot. Tell Claude once that it
should book its time, and every session shows up live in the app; ask for a
summary in the evening and you get the day back as a readable report.

It writes to the same SQLite database the desktop app reads
(`%APPDATA%\request-pilot\state.db` and its equivalents), so entries appear in the
app without a restart. Entries booked this way are tagged `source = "mcp"`, which
is how the app can tell your own timers from the agent's.

Everything is local. There are two ways to reach it: the `rp-mcp` binary over
**stdio** (below), and an **HTTP** listener the desktop app starts on request
(["Over HTTP"](#over-http-from-the-desktop-app)). Both talk to the same database,
and neither has an account or a cloud.

## Build

```sh
cargo build --release -p rp-mcp
```

The binary lands in `target/release/rp-mcp` (`rp-mcp.exe` on Windows). Use the
**absolute path** to it in the configuration below — MCP clients do not run your
shell's `PATH` in a login shell, so `rp-mcp` alone often will not resolve.

## Register with Claude Code

One command, from anywhere:

```sh
claude mcp add time-tracker --scope user -- C:\path\to\request-desktop\target\release\rp-mcp.exe
```

Or edit the config by hand — `~/.claude.json` for user scope, or `.mcp.json` in a
project root to share it with a repo:

```json
{
  "mcpServers": {
    "time-tracker": {
      "command": "C:\\path\\to\\request-desktop\\target\\release\\rp-mcp.exe",
      "args": [],
      "env": {}
    }
  }
}
```

Check it with `claude mcp list`, or `/mcp` inside a session.

## Register with Claude Desktop

Edit `claude_desktop_config.json` and restart Claude Desktop:

* **Windows** `%APPDATA%\Claude\claude_desktop_config.json`
* **macOS** `~/Library/Application Support/Claude/claude_desktop_config.json`
* **Linux** `~/.config/Claude/claude_desktop_config.json`

```json
{
  "mcpServers": {
    "time-tracker": {
      "command": "C:\\path\\to\\request-desktop\\target\\release\\rp-mcp.exe",
      "args": [],
      "env": {}
    }
  }
}
```

On macOS and Linux use a plain path, no escaping: `/Users/you/request-desktop/target/release/rp-mcp`.

### Pointing at a different database

Either form works, for a test database or a second profile:

```json
{
  "mcpServers": {
    "time-tracker": {
      "command": "/path/to/rp-mcp",
      "args": ["--db", "/path/to/other.db"]
    }
  }
}
```

```json
{
  "mcpServers": {
    "time-tracker": {
      "command": "/path/to/rp-mcp",
      "env": { "RP_MCP_DB": "/path/to/other.db" }
    }
  }
}
```

`--db` wins over `RP_MCP_DB`; with neither, the app's own database is used. The
file and its directory are created if they do not exist.

## Over HTTP, from the desktop app

The desktop app can host the same nine tools itself, so an agent reaches them over
HTTP instead of launching a process. Open the **gear in the title bar → Agent
access (MCP)** and press **Start server**. The card then shows the URL and a token;
copy the token with the button next to it.

```
http://127.0.0.1:4319/mcp
```

Register it with Claude Code:

```sh
claude mcp add --transport http time-tracker http://127.0.0.1:4319/mcp   --header "Authorization: Bearer <the token from the settings window>"
```

Or by hand, in `~/.claude.json` or a project's `.mcp.json`:

```json
{
  "mcpServers": {
    "time-tracker": {
      "type": "http",
      "url": "http://127.0.0.1:4319/mcp",
      "headers": { "Authorization": "Bearer <the token from the settings window>" }
    }
  }
}
```

Things worth knowing before you rely on it:

* **The token changes every time the server is started.** It is generated per
  activation and never written to disk, so after a restart of the app you paste a
  new one. That is the trade for not storing a long-lived credential next to the
  database it unlocks.
* **The switch is off by default** and remembered: turn it on and the next launch
  starts the server again — with a new token.
* **The app does not have to be showing the Time section**, but it does have to be
  running. Closing the app stops the listener; for a server that outlives the UI,
  use the stdio binary.
* **The port** is 4319. If something else already has it, the settings card says so
  and the server stays off rather than moving to a port your configuration does not
  name.
* Only `127.0.0.1` is bound, the `Origin` header is checked, and the token is
  required — see ["Security"](#security-of-the-http-listener).

The stdio binary and the HTTP listener can be registered at the same time, but
there is little point: they expose the same tools over the same database, and
Claude would see eighteen.

## Security of the HTTP listener

This listener can read, book and delete your time entries, so it is treated as a
credential boundary rather than a convenience:

* **Loopback only.** It binds `127.0.0.1`, never `0.0.0.0`. Nothing on your network
  can reach it.
* **A bearer token per activation**, 32 random bytes from the OS. Requests without
  it, or with the wrong one, get `401` — and the refusal is identical whether or
  not the session id was real, so an unauthenticated caller cannot probe for
  sessions. Without this any other program on your machine, including a web page's
  `fetch("http://127.0.0.1:4319/mcp")`, could book time as you.
* **The `Origin` header is validated** and anything that is not a loopback web
  origin gets `403`. This is the mitigation the MCP specification asks for against
  DNS rebinding, where a site you visit resolves its own name to `127.0.0.1`.
* **Nothing is logged that unlocks it.** The URL is printed to stderr on start; the
  token is not.

## Telling the agent to use it

The server ships `instructions` that Claude reads on connect, but a line in your
`CLAUDE.md` (or in the conversation) is what makes it a habit:

> When you work with me, track your time: call `start_timer` with a short task
> name when you begin, and `stop_timer` when you finish or switch tasks. If we
> covered work you did not track, book it with `log_time`.

Then, in the evening:

> Give me today's summary.

## The tools

| Tool | Arguments | What it does |
| --- | --- | --- |
| `start_timer` | `task`, `note?` | Starts tracking now. **Stops any running timer first**, so switching tasks is one call. Creates the task on first use. |
| `stop_timer` | — | Stops the running timer and reports the duration. Saying "nothing was running" is not an error. |
| `log_time` | `task`, `started_at`, `stopped_at`, `note?` | Books a finished block after the fact. Does not touch a running timer. |
| `current_timer` | — | What is running, since when, and for how long. |
| `list_entries` | `date?` \| `from?`/`to?` \| `search?`, `limit?` | Individual entries with their ids. Exactly one selector; no selector means today. |
| `summary` | `date?` | One day: per-task totals and a grand total. Defaults to today. |
| `summary_month` | `month?` | A month: one line per day, then per-task totals. Defaults to this month. |
| `list_tasks` | — | Every task name used so far. |
| `delete_entry` | `id` | Deletes one entry. Get the id from `list_entries`. |

### Input formats

`started_at`, `stopped_at`, `from` and `to` accept:

* ISO 8601 — `2026-07-26T09:00:00`, `2026-07-26 09:00:00`, `2026-07-26T09:00:00+02:00`,
  `2026-07-26T07:00:00Z`, or a bare `2026-07-26` (midnight)
* Unix seconds, as a number or a string — `1785049200`, `"1785049200"`

**Without a timezone, a timestamp is read as your local time**, because that is
what "09:00" means to a person. Add `Z` or an offset to be explicit.

`date` accepts `today`, `yesterday`, `tomorrow` or `YYYY-MM-DD`.
`month` accepts `this month`, `last month` or `YYYY-MM`.

Anything else is refused with a message naming the accepted formats, rather than
booked on a day nobody meant. So are milliseconds-instead-of-seconds, timestamps
in the future, reversed start/end pairs, and single blocks longer than 24 hours.

### Example

```
> log 9 to 11:30 this morning on the mcp server, then summarise the day

Booked 2h 30m on "mcp server" (2026-07-26 09:00-11:30), entry #1.

2026-07-26 (today) -- total 3h 15m
     2h 30m  mcp server
     0h 45m  code review
```

## Running it by hand

It speaks newline-delimited JSON-RPC 2.0 on stdin/stdout and is meant to be
launched by a client, but it is a pipe like any other:

```sh
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"curl","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"summary","arguments":{"date":"today"}}}' \
  | rp-mcp --db /tmp/scratch.db
```

**stdout carries the protocol and nothing else.** Diagnostics — the startup
banner, the database path, errors — all go to stderr, which is where MCP clients
collect server logs (`~/.cache/claude-cli-nodejs/**/mcp-logs-*` for Claude Code,
`~/Library/Logs/Claude/mcp-server-*.log` for Claude Desktop on macOS).

## Troubleshooting

**The server does not appear / shows as failed.** Almost always the command path.
Run it by hand first: `/path/to/rp-mcp --version` should print a version to
stderr and exit 0. Then check the client's MCP log for the `rp-mcp: …` line.

**"cannot open the app's database".** Request Pilot has not created it yet — start
the app once, or pass `--db` at a path you can write.

**"database is locked".** Two processes share one file and SQLite serialises
writers. The tool call returns the error and the session continues; retry it.

**Times land on the wrong day.** The agent is sending timestamps in UTC while
meaning local, or vice versa. Have it pass an explicit offset or `Z`; local-day
grouping itself is done by SQLite against your system timezone.

## Notes for maintainers

**No SDK.** The official `rmcp` crate is at `3.0.0-beta.2`; the parts of MCP this
server needs — `initialize`, `tools/list`, `tools/call`, `notifications/*` over
newline-delimited stdio — are about 150 lines against `serde_json`, which the
workspace already depends on. A pre-1.0 dependency in the middle of the protocol
path of something the user runs every day is a worse trade than owning those
lines. The Streamable HTTP transport added later is another ~200 lines on top of
`axum`, and it reuses the same dispatch rather than duplicating it. Revisit if this
ever needs sampling, resources or SSE streams, where an SDK earns its keep.

**Protocol revision** is `2025-11-25`; `2025-06-18`, `2025-03-26` and `2024-11-05`
are echoed back when a client asks for them, since none of the shapes used here
differ between those revisions. Only the `tools` capability is declared.

**Layout.** `src/proto.rs` is the JSON-RPC dispatch, `src/tools.rs` the tool
definitions and their output text, `src/time.rs` the input coercion, `src/lib.rs`
the stdio read loop, `src/http.rs` the Streamable HTTP transport. The two
transports are two front ends onto one `Server::handle_line`; a tool added to
`tools.rs` appears on both. `tests/subprocess.rs` drives the real binary over real
pipes, which is the only test that can catch a stray `println!`;
`tests/http.rs` drives the HTTP transport over a real socket, which is the only
kind of test that catches a wrong header.

**The HTTP transport** answers a JSON-RPC *request* with `application/json` and a
*notification* with `202`, which the specification allows in place of an SSE
stream; `GET` is `405` for the same reason. Sessions are `Mcp-Session-Id`, minted
on `initialize`, ended by `DELETE`. It opens its **own** connection to the database
rather than sharing the app's: `rusqlite::Connection` is not `Sync`, WAL makes a
second connection safe, and "only one timer runs" is a `UNIQUE` index rather than
application logic — the same reasoning that lets the stdio server run as a separate
process at the same time.
