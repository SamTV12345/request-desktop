//! `base64-file-converter`

use crate::{Args, Category, Kind, Tool};
use anyhow::{Result, bail};

pub(crate) fn base64_file(_args: &Args) -> Result<String> {
    bail!(
        "needs the file panel: pick or drop a file to get its data: URI, or paste          a data: URI to save it back out — with an image preview either way"
    )
}

pub static TOOL: Tool = Tool {
    id: "base64-file-converter",
    name: "Base64 file converter",
    description: "Convert a file to and from its base64 representation.",
    category: Category::Converter,
    keywords: &["base64", "file", "upload", "data-url"],
    fields: &[],
    // Needs a file picker and a download target; the generic form cannot express it.
    kind: Kind::Custom,
    run: base64_file,
};
