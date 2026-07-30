//! Structured display for the tools that only *report* something.
//!
//! `http-status-codes`, `git-memo` and `regex-memo` are lists and cheat sheets.
//! They used to arrive as one string in a read-only editor, which is a box you
//! read a wall of monospace out of — and looks like a field you are failing to
//! type into. What they want is headings, prose, code and tables, plus a search.
//!
//! The registry still produces text, because that is the one shape all 86 tools
//! share and what the MCP side consumes. So this module parses that text back
//! into blocks. The three formats it has to read:
//!
//! * **Markdown-ish** — `##`/`###` headings, ```` ``` ```` fences, `a | b` tables
//!   with a `:--|:--` rule. `git-memo`, `regex-memo`.
//! * **Indented** — a group at column 0, its entries at two spaces, their
//!   explanations at four. `http-status-codes`.
//!
//! Everything is line-based and nothing is dropped: a line that matches no rule
//! is prose. That is the property `parse_keeps_every_line` asserts over the real
//! output of every reference tool, which is what stops a format change upstream
//! from quietly swallowing half a page.
use gpui::{IntoElement, ParentElement as _, Styled as _, div, prelude::*};
use rp_ui::theme::{self, FONT_HEADING, FONT_MONO};
use rp_ui::tokens::{radius, space, text};
use std::collections::BTreeSet;

/// One piece of a reference page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// `level` is 1..=6, as in Markdown; the indented format's groups are 2.
    Heading { level: u8, text: String },
    /// Running prose, one or more consecutive lines joined by a space.
    Text(String),
    /// A fenced or indented code block, newlines preserved.
    Code(String),
    Table {
        header: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    /// `term` with its explanation — `404 Not Found` and what it means.
    Entry { term: String, detail: String },
}

impl Block {
    /// Every piece of text in this block, for the search to look through.
    fn haystack(&self) -> String {
        match self {
            Block::Heading { text, .. } | Block::Text(text) | Block::Code(text) => text.clone(),
            Block::Entry { term, detail } => format!("{term} {detail}"),
            Block::Table { header, rows } => {
                let mut s = header.join(" ");
                for row in rows {
                    s.push(' ');
                    s.push_str(&row.join(" "));
                }
                s
            }
        }
    }

    fn is_heading(&self) -> bool {
        matches!(self, Block::Heading { .. })
    }
}

/// Split a table line into its cells, dropping the optional leading and trailing
/// pipe. Cells keep their inner spacing trimmed, since these come from a
/// hand-aligned source.
///
/// A `\|` is an escaped pipe and not a cell boundary — `regex-memo` has a row
/// `` `\|` | pipe `` that a naive split tore in half. The escape is left in the
/// cell rather than unescaped: in a regex cheat sheet `\|` *is* the notation for
/// a literal pipe, so it is the more useful of the two readings.
fn cells(line: &str) -> Vec<String> {
    let line = line.trim();
    // Strip one optional outer pipe on each side, taking care not to eat an
    // escaped one at the end.
    let line = line.strip_prefix('|').unwrap_or(line);
    let line = match line.strip_suffix('|') {
        Some(rest) if !rest.ends_with('\\') => rest,
        _ => line,
    };
    let mut out = Vec::new();
    let mut cell = String::new();
    let mut escaped = false;
    for c in line.chars() {
        match c {
            _ if escaped => {
                cell.push(c);
                escaped = false;
            }
            '\\' => {
                cell.push(c);
                escaped = true;
            }
            '|' => out.push(std::mem::take(&mut cell).trim().to_string()),
            _ => cell.push(c),
        }
    }
    out.push(cell.trim().to_string());
    out
}

/// Whether `line` is a table's alignment rule — `:--|:--`, `---|---`.
fn is_table_rule(line: &str) -> bool {
    let line = line.trim();
    line.contains('|')
        && line
            .chars()
            .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '='))
        && line.contains('-')
}

/// Leading spaces, counting a tab as two.
fn indent_of(line: &str) -> usize {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { 2 } else { 1 })
        .sum()
}

/// Parse a reference tool's output into blocks.
pub fn parse(text: &str) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    // Prose accumulates until something that is not prose ends it.
    let mut prose: Vec<String> = Vec::new();
    let flush = |prose: &mut Vec<String>, out: &mut Vec<Block>| {
        if !prose.is_empty() {
            out.push(Block::Text(prose.join(" ")));
            prose.clear();
        }
    };

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            flush(&mut prose, &mut out);
            i += 1;
            continue;
        }

        // ---- fenced code
        if trimmed.starts_with("```") {
            flush(&mut prose, &mut out);
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                body.push(lines[i]);
                i += 1;
            }
            // An unterminated fence still yields its body rather than nothing.
            i += usize::from(i < lines.len());
            out.push(Block::Code(body.join("\n")));
            continue;
        }

        // ---- heading
        if let Some(rest) = trimmed.strip_prefix('#') {
            let extra = rest.chars().take_while(|c| *c == '#').count();
            let title = rest[extra..].trim();
            if !title.is_empty() {
                flush(&mut prose, &mut out);
                out.push(Block::Heading {
                    level: (extra as u8 + 1).min(6),
                    text: title.to_string(),
                });
                i += 1;
                continue;
            }
        }

        // ---- table: a header row followed by an alignment rule
        if trimmed.contains('|')
            && lines
                .get(i + 1)
                .is_some_and(|next| is_table_rule(next) && !is_table_rule(trimmed))
        {
            flush(&mut prose, &mut out);
            let header = cells(trimmed);
            let mut rows = Vec::new();
            i += 2;
            while i < lines.len() {
                let row = lines[i].trim();
                if row.is_empty() || !row.contains('|') {
                    break;
                }
                rows.push(cells(row));
                i += 1;
            }
            out.push(Block::Table { header, rows });
            continue;
        }

        // ---- the indented format: group / entry / explanation
        let indent = indent_of(line);
        if indent >= 4 {
            // An explanation belongs to the entry above it.
            if let Some(Block::Entry { detail, .. }) = out.last_mut() {
                if !detail.is_empty() {
                    detail.push(' ');
                }
                detail.push_str(trimmed);
                i += 1;
                continue;
            }
            // No entry above: indented text on its own is a code block, which is
            // what four spaces mean in Markdown anyway.
            flush(&mut prose, &mut out);
            out.push(Block::Code(trimmed.to_string()));
            i += 1;
            continue;
        }
        if indent >= 2 {
            flush(&mut prose, &mut out);
            out.push(Block::Entry {
                term: trimmed.to_string(),
                detail: String::new(),
            });
            i += 1;
            continue;
        }
        // A column-0 line directly above an indented one heads the group below it.
        if lines.get(i + 1).is_some_and(|next| indent_of(next) >= 2) {
            flush(&mut prose, &mut out);
            out.push(Block::Heading {
                level: 2,
                text: trimmed.to_string(),
            });
            i += 1;
            continue;
        }

        prose.push(trimmed.to_string());
        i += 1;
    }
    flush(&mut prose, &mut out);
    out
}

/// Keep what matches `query`, plus the headings the survivors sit under.
///
/// A heading alone is never a match worth keeping — searching "commit" in a git
/// cheat sheet should not leave three bare headings behind — so a heading
/// survives only when something under it does. A table keeps its header whenever
/// any of its rows match, because a table without its header is unreadable.
pub fn filter(blocks: &[Block], query: &str) -> Vec<Block> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return blocks.to_vec();
    }
    // First pass: which non-heading blocks survive, and which heading each of
    // them sits under.
    let mut keep = BTreeSet::new();
    let mut open_headings: Vec<usize> = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        if let Block::Heading { level, .. } = block {
            // A heading closes every heading at or below its own level.
            open_headings.retain(|j| match &blocks[*j] {
                Block::Heading { level: other, .. } => *other < *level,
                _ => false,
            });
            open_headings.push(i);
            continue;
        }
        let hit = match block {
            Block::Table { header, rows } => {
                let matching: Vec<Vec<String>> = rows
                    .iter()
                    .filter(|row| row.join(" ").to_lowercase().contains(&q))
                    .cloned()
                    .collect();
                !matching.is_empty() || header.join(" ").to_lowercase().contains(&q)
            }
            other => other.haystack().to_lowercase().contains(&q),
        };
        if hit {
            keep.insert(i);
            keep.extend(open_headings.iter().copied());
        }
    }

    // Second pass: emit in order, narrowing tables to their matching rows.
    let mut out = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        if !keep.contains(&i) {
            continue;
        }
        match block {
            Block::Table { header, rows } => {
                let matching: Vec<Vec<String>> = rows
                    .iter()
                    .filter(|row| row.join(" ").to_lowercase().contains(&q))
                    .cloned()
                    .collect();
                out.push(Block::Table {
                    header: header.clone(),
                    // A table matched by its header alone keeps all its rows.
                    rows: if matching.is_empty() {
                        rows.clone()
                    } else {
                        matching
                    },
                });
            }
            other => out.push(other.clone()),
        }
    }
    // A heading whose whole group was dropped by the pass above (possible when it
    // only held another heading) would be a bare title; drop those too.
    while out.last().is_some_and(Block::is_heading) {
        out.pop();
    }
    out
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

/// One drawn line of a reference page.
///
/// A table is expanded into one `Row` per line rather than staying a single
/// element, because virtualising a page means being able to skip *rows* — and a
/// block-per-element list cannot skip inside the one block that holds 68 of them.
///
/// The count that killed the old shape: `port-info-search` is a 68-row, 4-column
/// table, so one `Block::Table` was 277 cells in a single element. Every frame laid
/// out all of them, and each cell wraps its text, which gpui measures per requested
/// width. Nothing on screen needed more than a dozen rows of that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A non-table block. `leads` marks the first block on the page, above which a
    /// heading takes no space.
    Block { block: Block, leads: bool },
    /// A table's header row, or one of its body rows.
    Table {
        cells: Vec<String>,
        /// The whole table's column count, so every row splits its width alike even
        /// though each is laid out on its own now.
        columns: usize,
        head: bool,
        /// Every other body row is washed, so a wide row is followed across.
        striped: bool,
    },
}

/// Flatten parsed blocks into the rows a list draws one at a time.
pub fn rows(blocks: Vec<Block>) -> Vec<Row> {
    let mut out = Vec::with_capacity(blocks.len());
    for (i, block) in blocks.into_iter().enumerate() {
        match block {
            Block::Table { header, rows } => {
                let columns = header
                    .len()
                    .max(rows.iter().map(Vec::len).max().unwrap_or(0))
                    .max(1);
                if !header.is_empty() {
                    out.push(Row::Table {
                        cells: header,
                        columns,
                        head: true,
                        striped: false,
                    });
                }
                for (r, cells) in rows.into_iter().enumerate() {
                    out.push(Row::Table {
                        cells,
                        columns,
                        head: false,
                        striped: r % 2 == 1,
                    });
                }
            }
            block => out.push(Row::Block {
                block,
                leads: i == 0,
            }),
        }
    }
    out
}

/// Draw one row: headings in the heading face, prose at reading width, code on a
/// sunken ground, a table row as its cells.
pub fn row(t: theme::Theme, row: &Row) -> gpui::AnyElement {
    match row {
        Row::Block { block, leads } => {
            // The gap the old single-container page got from `gap(space::S2)`. A
            // table row deliberately gets none: its rows are flush by definition.
            block_element(t, block, *leads)
                .w_full()
                .pb(space::S2)
                .into_any_element()
        }
        Row::Table {
            cells,
            columns,
            head,
            striped,
        } => {
            // A definite fraction of the row, not `flex_1`. Those look identical —
            // `flex_1` splits the free space evenly too — and cost wildly
            // differently. A cell sets neither `truncate()` nor
            // `whitespace_nowrap()` on purpose, since a reference table is there to
            // be read and a long description should wrap; gpui then caches that wrap
            // keyed on the width it was measured at (`gpui::elements::text`, the
            // `wrap_width == text_layout.wrap_width` arm). A flexible cell gets
            // measured at several widths per frame — min-content, max-content, then
            // the resolved one — and every probe missed the cache and re-wrapped
            // every cell.
            let width = gpui::relative(1. / *columns as f32);
            let mut line = div()
                .flex()
                // Load-bearing, and it was not needed before: `relative()` is a
                // fraction *of the parent*, and a row is now a list item rather than
                // a child of a `flex_col` table that stretched it. Without a definite
                // width to divide, every cell fell back to its content width — so the
                // columns went ragged and the striping stopped where the text did.
                .w_full()
                .items_baseline()
                .when(*head, |d| d.bg(t.bg))
                .when(!*head, |d| d.border_t_1().border_color(t.divider))
                .when(*striped, |d| d.bg(t.hover()));
            for value in cells {
                line = line.child(
                    div()
                        .w(width)
                        // Still needed with a definite width: a flex item's automatic
                        // minimum is its min-content width, so one long unbroken word
                        // would otherwise widen the column back out.
                        .min_w_0()
                        .px(space::S2)
                        .py(space::S1)
                        .text_size(if *head { text::LABEL } else { text::CONTROL })
                        .text_color(if *head { t.text_label() } else { t.text })
                        .when(!*head, |d| d.font_family(FONT_MONO))
                        .child(value.clone()),
                );
            }
            line.into_any_element()
        }
    }
}

fn block_element(t: theme::Theme, block: &Block, leads: bool) -> gpui::Div {
    match block {
        Block::Heading { level, text } => div()
            .font_family(FONT_HEADING)
            .text_color(t.text)
            // Levels map onto the type scale rather than each getting their
            // own size: the pages only use two or three depths.
            .text_size(match level {
                1 | 2 => text::H4,
                3 => text::H5,
                _ => text::H6,
            })
            // Space above a heading, not below: it belongs to what follows it.
            .when(!leads, |d| d.mt(space::S3))
            .child(text.clone()),
        Block::Text(body) => div()
            .text_size(text::BODY)
            .line_height(text::BODY_LINE)
            .text_color(t.text_muted())
            .child(body.clone()),
        Block::Code(body) => div()
            .px(space::S3)
            .py(space::S2)
            .rounded(radius::MD)
            .bg(t.bg)
            .border_1()
            .border_color(t.divider)
            .font_family(FONT_MONO)
            .text_size(text::CONTROL)
            .text_color(t.text)
            .child(body.clone()),
        Block::Entry { term, detail } => div()
            .flex()
            .flex_col()
            .gap(space::S1 * 0.5)
            .child(
                div()
                    .font_family(FONT_MONO)
                    .text_size(text::CONTROL)
                    .text_color(t.accent)
                    .child(term.clone()),
            )
            .when(!detail.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(text::SMALL)
                        .line_height(text::BODY_LINE)
                        .text_color(t.text_muted())
                        .child(detail.clone()),
                )
            }),
        // `rows()` expands every table into `Row::Table`, so a table never reaches
        // here. An empty box rather than a panic: this is a render path.
        Block::Table { .. } => div(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heading(level: u8, text: &str) -> Block {
        Block::Heading {
            level,
            text: text.into(),
        }
    }

    #[test]
    fn markdown_headings_prose_and_fences() {
        let blocks = parse(
            "## Configuration\n\nSet the global config\n\n```shell\ngit config a\ngit config b\n```\n",
        );
        assert_eq!(
            blocks,
            vec![
                heading(2, "Configuration"),
                Block::Text("Set the global config".into()),
                Block::Code("git config a\ngit config b".into()),
            ]
        );
        // Levels come through, and consecutive prose lines join into one block.
        assert_eq!(
            parse("# One\n### Three\nline a\nline b\n"),
            vec![
                heading(1, "One"),
                heading(3, "Three"),
                Block::Text("line a line b".into()),
            ]
        );
        // An unterminated fence still yields its body.
        assert_eq!(
            parse("```\nstuck\n"),
            vec![Block::Code("stuck".into())],
            "an unterminated fence must not swallow the page"
        );
    }

    #[test]
    fn pipe_tables_keep_their_header_and_cells() {
        let blocks =
            parse("Expression | Description\n:--|:--\n`.` | any character\n`\\d` | digit\n");
        assert_eq!(
            blocks,
            vec![Block::Table {
                header: vec!["Expression".into(), "Description".into()],
                rows: vec![
                    vec!["`.`".into(), "any character".into()],
                    vec!["`\\d`".into(), "digit".into()],
                ],
            }]
        );
        // Leading and trailing pipes are optional.
        let Block::Table { header, rows } = &parse("| a | b |\n|---|---|\n| 1 | 2 |\n")[0] else {
            panic!("expected a table");
        };
        assert_eq!(header, &["a".to_string(), "b".to_string()]);
        assert_eq!(rows, &[vec!["1".to_string(), "2".to_string()]]);
        // A pipe in ordinary prose is not a table without the rule under it.
        assert_eq!(
            parse("use a | b for alternation\n"),
            vec![Block::Text("use a | b for alternation".into())]
        );
    }

    #[test]
    fn the_indented_format_becomes_groups_and_entries() {
        let blocks = parse(
            "1xx informational response\n  100 Continue\n    Waiting for the client.\n  101 Switching Protocols\n    The server agreed.\n2xx success\n  200 OK\n    Standard response.\n",
        );
        assert_eq!(
            blocks,
            vec![
                heading(2, "1xx informational response"),
                Block::Entry {
                    term: "100 Continue".into(),
                    detail: "Waiting for the client.".into(),
                },
                Block::Entry {
                    term: "101 Switching Protocols".into(),
                    detail: "The server agreed.".into(),
                },
                heading(2, "2xx success"),
                Block::Entry {
                    term: "200 OK".into(),
                    detail: "Standard response.".into(),
                },
            ]
        );
        // A wrapped explanation joins the one line above it.
        let blocks = parse("g\n  term\n    part one\n    part two\n");
        assert_eq!(
            blocks[1],
            Block::Entry {
                term: "term".into(),
                detail: "part one part two".into()
            }
        );
    }

    /// The parser must only ever *re-shape* the text. Anything it drops is a page
    /// the user cannot read any more, which is exactly the bug this display was
    /// built to fix.
    #[test]
    fn parse_keeps_every_line_of_every_reference_tool() {
        for tool in rp_devtools::all() {
            // The reference tools are the ones with no operand fields.
            let filtering = tool.fields.len() == 1 && tool.fields[0].key == "query";
            if !tool.fields.is_empty() && !filtering {
                continue;
            }
            let Ok(text) = (tool.run)(&rp_devtools::Args::new()) else {
                continue;
            };
            let blocks = parse(&text);
            let seen: String = blocks
                .iter()
                .map(|b| b.haystack())
                .collect::<Vec<_>>()
                .join(" ");
            // Every non-blank, non-syntax word of the source has to appear.
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with("```") || is_table_rule(line) {
                    continue;
                }
                for word in line.split_whitespace() {
                    let word = word.trim_matches(|c: char| c == '|' || c == '#');
                    if word.is_empty() {
                        continue;
                    }
                    assert!(
                        seen.contains(word),
                        "{}: lost {word:?} from line {line:?}",
                        tool.id
                    );
                }
            }
        }
    }

    #[test]
    fn search_keeps_the_heading_a_hit_sits_under() {
        let blocks =
            parse("## Configuration\n\nset the config\n\n## Branching\n\ncreate a branch\n");
        let hits = filter(&blocks, "branch");
        assert_eq!(
            hits,
            vec![
                heading(2, "Branching"),
                Block::Text("create a branch".into())
            ],
            "the hit keeps its own heading and nothing else"
        );

        // Case and surrounding space do not matter.
        assert_eq!(filter(&blocks, "  BRANCH \n"), hits);
        // An empty query is everything, unchanged.
        assert_eq!(filter(&blocks, "   "), blocks);
        // No match is nothing, not a page of bare headings.
        assert!(filter(&blocks, "zzz-nothing").is_empty());
    }

    #[test]
    fn search_narrows_a_table_to_its_matching_rows_but_keeps_the_header() {
        let blocks = parse(
            "### Characters\nExpression | Description\n:--|:--\n`\\d` | digit\n`\\w` | word character\n",
        );
        let hits = filter(&blocks, "digit");
        assert_eq!(
            hits,
            vec![
                heading(3, "Characters"),
                Block::Table {
                    header: vec!["Expression".into(), "Description".into()],
                    rows: vec![vec!["`\\d`".into(), "digit".into()]],
                },
            ]
        );
        // Matching the header keeps every row, since the table itself is the hit.
        let all = filter(&blocks, "expression");
        let Block::Table { rows, .. } = &all[1] else {
            panic!("expected a table");
        };
        assert_eq!(rows.len(), 2);
    }

    /// Searching a real page has to find real things, or the box is decoration.
    #[test]
    fn search_finds_real_entries_in_the_real_pages() {
        let codes = parse(
            &(rp_devtools::find("http-status-codes").unwrap().run)(&rp_devtools::Args::new())
                .unwrap(),
        );
        let hits = filter(&codes, "418");
        assert!(
            hits.iter()
                .any(|b| matches!(b, Block::Entry { term, .. } if term.contains("418"))),
            "{hits:?}"
        );
        // And the group heading came along, so the hit is in context.
        assert!(hits.iter().any(Block::is_heading), "{hits:?}");

        // Terms this cheat sheet really carries — checked against its own text,
        // not against what a git cheat sheet might be assumed to hold. This list
        // grew with the page: "rebase" used to be the term it genuinely lacked,
        // until PR #1204's branch and rebase sections were ported.
        let git = parse(
            &(rp_devtools::find("git-memo").unwrap().run)(&rp_devtools::Args::new()).unwrap(),
        );
        for term in ["commit", "branch", "reset", "rebase", "stash"] {
            let hits = filter(&git, term);
            assert!(!hits.is_empty(), "the git cheat sheet lost {term:?}");
            assert!(
                hits.iter().any(Block::is_heading),
                "{term:?} came back without the heading it sits under"
            );
        }
        // And a term it genuinely lacks finds nothing rather than everything.
        // Pick one no git cheat sheet will ever grow into.
        assert!(filter(&git, "subversion").is_empty());
    }
}
