//! `pdf-signature-checker`

use crate::{Args, Category, Kind, Tool};
use anyhow::{Result, bail};

pub(crate) fn run_pdf_signature(_: &Args) -> Result<String> {
    // it-tools: a drag-and-drop .pdf upload, then one panel per embedded
    // signature listing its certificate chain. No text input at all, hence no
    // fields and Kind::Custom.
    bail!(
        "needs bespoke UI: takes a dropped PDF file and lists the certificates \
         of each embedded PKCS#7 signature"
    )
}

pub static TOOL: Tool = Tool {
    id: "pdf-signature-checker",
    name: "PDF signature checker",
    description: "Verify the digital signatures embedded in a PDF file.",
    category: Category::Crypto,
    keywords: &["pdf", "signature", "certificate", "verify", "sign"],
    fields: &[],
    kind: Kind::Custom,
    run: run_pdf_signature,
};
