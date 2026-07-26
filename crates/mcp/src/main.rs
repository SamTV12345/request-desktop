//! `rp-mcp` -- the MCP stdio server for the local time tracker.
//!
//! stdout is the protocol channel: it carries JSON-RPC and nothing else. Every
//! message for a human goes to stderr, which is where MCP clients put server logs.
use anyhow::{Context, Result};
use rp_mcp::{Server, serve};
use rp_store::Store;
use std::io::{BufReader, Write};
use std::path::PathBuf;

const USAGE: &str = "\
rp-mcp -- MCP server for the Request Pilot time tracker (stdio transport)

Usage: rp-mcp [--db <path>]

  --db <path>   Database to use instead of the app's own. Also RP_MCP_DB.
  -h, --help    Print this and exit.
  -V, --version Print the version and exit.

Speaks newline-delimited JSON-RPC 2.0 on stdin/stdout; it is meant to be launched
by an MCP client, not run by hand. See README-mcp.md.";

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rp-mcp: {e:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let Some(db) = db_from_args()? else {
        return Ok(()); // --help / --version already printed.
    };

    let store = match &db {
        Some(path) => Store::open_at(path)
            .with_context(|| format!("cannot open the database at {}", path.display()))?,
        None => Store::open_default().context(
            "cannot open the app's database; pass --db <path> or set RP_MCP_DB to override",
        )?,
    };
    let mut server = Server::new(store)?;
    eprintln!(
        "rp-mcp {} ready on stdio (database: {})",
        env!("CARGO_PKG_VERSION"),
        db.map(|p| p.display().to_string())
            .or_else(|| rp_store::default_path()
                .ok()
                .map(|p| p.display().to_string()))
            .unwrap_or_else(|| "<unknown>".into())
    );

    let stdout = std::io::stdout();
    serve(
        &mut server,
        BufReader::new(std::io::stdin().lock()),
        stdout.lock(),
    )?;
    eprintln!("rp-mcp: stdin closed, exiting");
    Ok(())
}

/// `Ok(None)` means "we printed something and should stop"; `Ok(Some(None))`
/// means "use the app's default database".
fn db_from_args() -> Result<Option<Option<PathBuf>>> {
    let mut args = std::env::args().skip(1);
    let mut db = std::env::var_os("RP_MCP_DB").map(PathBuf::from);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => {
                db = Some(PathBuf::from(args.next().context("--db needs a path")?));
            }
            // `--db=path` is what people type when the space form fails them.
            a if a.starts_with("--db=") => db = Some(PathBuf::from(&a[5..])),
            "-h" | "--help" => {
                // Usage on stderr as well: stdout belongs to the protocol, and a
                // client that launched us with --help by mistake must not see it.
                eprintln!("{USAGE}");
                let _ = std::io::stderr().flush();
                return Ok(None);
            }
            "-V" | "--version" => {
                eprintln!("rp-mcp {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            other => anyhow::bail!("unknown argument {other:?}\n\n{USAGE}"),
        }
    }
    Ok(Some(db))
}
