//! `xml-formatter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Context, Result, anyhow, bail};
use std::fmt::Write as _;

pub(crate) fn xml_formatter(args: &Args) -> Result<String> {
    let t = &TOOLS[15];
    let raw = arg(args, t, "input").trim();
    if raw.is_empty() {
        bail!("no XML input");
    }
    let indent = num(args, t, "indent")?.clamp(0, 16) as usize;
    let collapse = flag(args, t, "collapseContent");
    let mut r = xml_reader(raw);
    let mut out: Vec<String> = Vec::new();
    // For each open element: where its opening tag landed in `out`, and whether
    // it has so far seen nothing but text. ponytail: xml-formatter's
    // `collapseContent` also collapses mixed text+element content; text-only is
    // the case anyone actually hits, and getting it wrong is invisible.
    let mut open: Vec<(usize, bool)> = Vec::new();
    let mut depth = 0usize;
    let mut saw_element = false;
    let push = |depth: usize, s: String, out: &mut Vec<String>| {
        out.push(format!("{}{}", " ".repeat(depth * indent), s));
    };
    let tag = |e: &quick_xml::events::BytesStart<'_>| -> Result<String> {
        let name: String = e.name().into_inner().into();
        let mut s = name;
        for (k, v) in attrs_of(e)? {
            let _ = write!(s, " {k}=\"{}\"", quick_xml::escape::escape(&v));
        }
        Ok(s)
    };

    use quick_xml::events::Event;
    loop {
        match r.read_event().context("invalid XML")? {
            Event::Start(e) => {
                saw_element = true;
                if let Some(p) = open.last_mut() {
                    p.1 = false;
                }
                open.push((out.len(), true));
                push(depth, format!("<{}>", tag(&e)?), &mut out);
                depth += 1;
            }
            Event::Empty(e) => {
                saw_element = true;
                if let Some(p) = open.last_mut() {
                    p.1 = false;
                }
                push(depth, format!("<{}/>", tag(&e)?), &mut out);
            }
            Event::End(e) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| anyhow!("unbalanced XML"))?;
                let name: String = e.name().into_inner().into();
                let (start_idx, text_only) = open.pop().ok_or_else(|| anyhow!("unbalanced XML"))?;
                if collapse && text_only && out.len() <= start_idx + 2 {
                    let text = if out.len() == start_idx + 2 {
                        out.pop().unwrap_or_default()
                    } else {
                        String::new()
                    };
                    out[start_idx].push_str(text.trim_start());
                    let _ = write!(out[start_idx], "</{name}>");
                } else {
                    push(depth, format!("</{name}>"), &mut out);
                }
            }
            Event::Text(e) => {
                let s: String = e.into_inner().into();
                if !s.trim().is_empty() {
                    push(
                        depth,
                        quick_xml::escape::escape(s.trim()).into_owned(),
                        &mut out,
                    );
                }
            }
            Event::CData(e) => push(depth, format!("<![CDATA[{}]]>", e.into_inner()), &mut out),
            Event::Comment(e) => push(depth, format!("<!--{}-->", e.into_inner()), &mut out),
            Event::Decl(e) => {
                let ver = e.version().map(|v| v.into_owned()).unwrap_or_default();
                let enc = e
                    .encoding()
                    .and_then(|r| r.ok())
                    .map(|v| format!(" encoding=\"{}\"", v))
                    .unwrap_or_default();
                let standalone = e
                    .standalone()
                    .and_then(|r| r.ok())
                    .map(|v| format!(" standalone=\"{}\"", v))
                    .unwrap_or_default();
                push(
                    depth,
                    format!("<?xml version=\"{ver}\"{enc}{standalone}?>"),
                    &mut out,
                );
            }
            Event::PI(e) => {
                let inner: String = e.into_inner().into();
                push(depth, format!("<?{inner}?>"), &mut out);
            }
            Event::DocType(e) => push(depth, format!("<!DOCTYPE {}>", e.into_inner()), &mut out),
            Event::GeneralRef(e) => push(depth, format!("&{};", e.into_inner()), &mut out),
            Event::Eof => break,
        }
    }
    if depth != 0 {
        bail!("unbalanced XML: unclosed tag");
    }
    if !saw_element {
        bail!("no XML element found");
    }
    Ok(out.join("\n"))
}

pub static TOOL: Tool = Tool {
    id: "xml-formatter",
    name: "XML formatter",
    description: "Indent an XML document and report syntax errors.",
    category: Category::Data,
    keywords: &["xml", "format", "prettify", "indent"],
    fields: &[
        Field::area("input", "XML"),
        Field::number("indent", "Indent size", "2"),
        Field::bool("collapseContent", "Collapse content", "true"),
    ],
    kind: Kind::Pure,
    run: xml_formatter,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;
    /// Vector from xml-formatter.service.test.ts, which calls the service with no
    /// options (4-space indent, no collapsing). The *UI* defaults differ, so they
    /// are passed explicitly here and asserted separately below.
    #[test]
    fn xml_formatter_vector() {
        assert_eq!(
            ok(
                "xml-formatter",
                &[
                    (
                        "input",
                        "<hello><world>foo</world><world>bar</world></hello>"
                    ),
                    ("indent", "4"),
                    ("collapseContent", "false"),
                ]
            ),
            "<hello>\n    <world>\n        foo\n    </world>\n    <world>\n        bar\n    </world>\n</hello>"
        );
        assert_eq!(
            ok(
                "xml-formatter",
                &[("input", "<a><b>c</b></a>"), ("collapseContent", "false")]
            ),
            "<a>\n  <b>\n    c\n  </b>\n</a>"
        );
    }
    /// it-tools' UI defaults: 2-space indent with `collapseContent` on, so a
    /// text-only element stays on one line.
    #[test]
    fn xml_formatter_collapses_text_content_by_default() {
        assert_eq!(
            ok(
                "xml-formatter",
                &[(
                    "input",
                    "<hello><world>foo</world><world>bar</world></hello>"
                )]
            ),
            "<hello>\n  <world>foo</world>\n  <world>bar</world>\n</hello>"
        );
        assert_eq!(ok("xml-formatter", &[("input", "<a></a>")]), "<a></a>");
    }
}
