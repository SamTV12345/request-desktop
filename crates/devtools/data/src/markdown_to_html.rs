//! `markdown-to-html`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Result, bail};

pub(crate) fn markdown_to_html(args: &Args) -> Result<String> {
    let input = arg(args, TOOLS[17], "input");
    if input.trim().is_empty() {
        bail!("no markdown input");
    }
    let mut opts = pulldown_cmark::Options::empty();
    opts.insert(pulldown_cmark::Options::ENABLE_TABLES);
    opts.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    opts.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
    opts.insert(pulldown_cmark::Options::ENABLE_FOOTNOTES);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, pulldown_cmark::Parser::new_ext(input, opts));
    Ok(html)
}

pub static TOOL: Tool = Tool {
    id: "markdown-to-html",
    name: "Markdown to HTML",
    description: "Render Markdown (with tables, task lists and footnotes) to HTML.",
    category: Category::Converter,
    keywords: &["markdown", "md", "html", "convert"],
    fields: &[Field::area("input", "Markdown")],
    kind: Kind::Pure,
    run: markdown_to_html,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;
    #[test]
    fn markdown_to_html_renders() {
        assert_eq!(
            ok("markdown-to-html", &[("input", "# hi")]),
            "<h1>hi</h1>\n"
        );
        let t = ok("markdown-to-html", &[("input", "| a |\n|---|\n| 1 |")]);
        assert!(t.contains("<table>"), "{t}");
        assert!(run("markdown-to-html", &[("input", "  ")]).is_err());
    }
}
