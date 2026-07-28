//! `gzip-string-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, section};
use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

// it-tools PR #1695.

pub(crate) fn gzip_converter(a: &Args) -> Result<String> {
    use std::io::{Read as _, Write as _};
    let text = s(a, "input", "");
    let packed = s(a, "gzipped", "").trim();
    if text.is_empty() && packed.is_empty() {
        bail!("enter text to compress, or paste base64-encoded gzip to expand");
    }
    let compress = || -> Result<String> {
        if text.is_empty() {
            bail!("nothing to compress");
        }
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(text.as_bytes())?;
        let out = enc.finish()?;
        Ok(format!(
            "{}\n\n{} bytes in, {} bytes out ({:.1}%)\n",
            STANDARD.encode(&out),
            text.len(),
            out.len(),
            out.len() as f64 * 100.0 / text.len().max(1) as f64
        ))
    };
    let expand = || -> Result<String> {
        if packed.is_empty() {
            bail!("nothing to expand");
        }
        let bytes = STANDARD
            .decode(packed.replace(['\n', '\r', ' '], ""))
            .context("not valid base64")?;
        let mut out = String::new();
        flate2::read::GzDecoder::new(&bytes[..])
            .read_to_string(&mut out)
            .context("not gzip, or not UTF-8 text inside")?;
        Ok(out)
    };
    Ok([
        section("Gzipped, base64", compress()),
        section("Text from gzip", expand()),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "gzip-string-converter",
    name: "Gzip string converter",
    description: "Gzip text to base64, and expand base64 gzip back to text.",
    category: Category::Converter,
    keywords: &["gzip", "compress", "deflate", "base64", "zip", "expand"],
    fields: &[
        Field::area("input", "Text to compress"),
        Field::area("gzipped", "Base64 gzip to expand"),
    ],
    kind: Kind::Pure,
    run: gzip_converter,
};
