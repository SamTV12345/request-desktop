//! `safelink-decoder`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Context, Result, bail};

pub(crate) fn safelink_decoder(args: &Args) -> Result<String> {
    let input = arg(args, tool("safelink-decoder"), "url").trim();
    if !input.contains(".safelinks.protection.outlook.com") {
        bail!("Invalid SafeLinks URL provided");
    }
    let query = input
        .split_once('?')
        .context("Invalid SafeLinks URL provided")?
        .1;
    // `&amp;` shows up when the link was copied out of rendered HTML.
    let target = query
        .split(['&', ';'])
        .map(|p| p.trim_start_matches("amp;"))
        .find_map(|p| p.strip_prefix("url="))
        .context("no `url` parameter in SafeLinks URL")?;
    Ok(percent_encoding::percent_decode_str(target)
        .decode_utf8()
        .context("`url` parameter is not valid percent-encoded UTF-8")?
        .into_owned())
}

pub static TOOL: Tool = Tool {
    id: "safelink-decoder",
    name: "Outlook Safelink decoder",
    description: "Decode Outlook SafeLink links.",
    category: Category::Web,
    keywords: &["outlook", "safelink", "decoder"],
    fields: &[Field::area("url", "Your SafeLink url")],
    kind: Kind::Pure,
    run: safelink_decoder,
};
