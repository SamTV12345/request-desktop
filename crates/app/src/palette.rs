//! What the command palette finds, and what picking a row does.
//!
//! Everything the user can reach in one list: every request in every open file,
//! all 86 tools, and the three sections. The overlay itself is
//! [`rp_ui::palette::Palette`] and knows none of that — it takes rows and reports
//! an index, and [`hits`] is what maps that index back onto an action.
//!
//! [`hits`] is a free function over plain data on purpose: "which rows come back,
//! in what order" is the whole behaviour, and it is testable without a window, a
//! file on disk or a `Source`.
use crate::Section;
use rp_ui::palette::Row;
use std::path::{Path, PathBuf};

/// What choosing a row does. Data rather than a boxed closure, so a test can read
/// the answer and the behaviour has one implementation, in `Workspace::choose`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Request {
        source: PathBuf,
        index: usize,
    },
    /// A tool id, not a `&'static Tool`: `rp_devtools::find` resolves it and the id
    /// is comparable, which the tool's fn pointer is not.
    Tool(&'static str),
    Section(Section),
}

/// One row and the thing it stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub row: Row,
    pub choice: Choice,
}

/// One request, flattened out of the open sources.
///
/// A borrowed view rather than the `Source` itself, so `hits` needs nothing that
/// touches the filesystem to be exercised.
pub struct Candidate<'a> {
    pub source: &'a Path,
    /// The file's own name. On the row because two files can hold a request with
    /// the same name, and two identical rows doing different things is the failure
    /// worth going out of the way to avoid.
    pub file: &'a str,
    pub index: usize,
    pub name: &'a str,
    pub method: &'a str,
    pub url: &'a str,
}

/// The three sections, with the rail's own labels and glyphs.
const SECTIONS: [(&str, &str, Section); 3] = [
    ("Requests", "folder", Section::Requests),
    ("Tools", "wrench", Section::Tools),
    ("Time", "clock", Section::Track),
];

/// Rows whose label begins with the query come first — a search for "get" wants
/// "Get films" above "Widget by id". Everything else keeps the order it came in,
/// which for requests is the order the files were opened.
fn rank(label: &str, query: &str) -> u8 {
    u8::from(!label.to_lowercase().starts_with(query))
}

/// Every row `query` matches, grouped by kind.
///
/// An empty query matches everything: the palette opens onto the whole list rather
/// than an empty box, which is also what makes it usable as a plain index.
pub fn hits(query: &str, requests: &[Candidate<'_>]) -> Vec<Hit> {
    let q = query.trim().to_lowercase();
    let mut out = Vec::new();

    let mut matched: Vec<(u8, Hit)> = Vec::new();
    for c in requests {
        // The file name is searched too, so "films.http" finds everything in it.
        let hay = [c.name, c.method, c.url, c.file];
        if !q.is_empty() && !hay.iter().any(|h| h.to_lowercase().contains(&q)) {
            continue;
        }
        matched.push((
            rank(c.name, &q),
            Hit {
                row: Row::new("Requests", c.name.to_string())
                    .detail(format!("{} · {}", c.method.to_uppercase(), c.file))
                    .icon("send"),
                choice: Choice::Request {
                    source: c.source.to_path_buf(),
                    index: c.index,
                },
            },
        ));
    }
    // `sort_by_key` is stable, so equal ranks keep the order the files were in.
    matched.sort_by_key(|(rank, _)| *rank);
    out.extend(matched.into_iter().map(|(_, hit)| hit));

    // The tools' own search, over name, id and keywords. Not reimplemented here:
    // that one is tested, and a second would drift from it.
    let mut matched: Vec<(u8, Hit)> = rp_devtools::search(&q)
        .into_iter()
        .map(|tool| {
            (
                rank(tool.name, &q),
                Hit {
                    row: Row::new("Tools", tool.name)
                        .detail(tool.description)
                        .icon("wrench"),
                    choice: Choice::Tool(tool.id),
                },
            )
        })
        .collect();
    matched.sort_by_key(|(rank, _)| *rank);
    out.extend(matched.into_iter().map(|(_, hit)| hit));

    for (label, glyph, section) in SECTIONS {
        if q.is_empty() || label.to_lowercase().contains(&q) {
            out.push(Hit {
                row: Row::new("Sections", label)
                    .detail("Go to section")
                    .icon(glyph),
                choice: Choice::Section(section),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate<'a>(file: &'a str, index: usize, name: &'a str) -> Candidate<'a> {
        Candidate {
            source: Path::new(file),
            file,
            index,
            name,
            method: "GET",
            url: "https://example.test/",
        }
    }

    fn labels(hits: &[Hit], group: &str) -> Vec<String> {
        hits.iter()
            .filter(|h| h.row.group == group)
            .map(|h| h.row.label.to_string())
            .collect()
    }

    /// Nothing typed yet: the whole reachable surface, not an empty box.
    #[test]
    fn an_empty_query_offers_everything() {
        let reqs = [candidate("films.http", 0, "Get films")];
        let hits = hits("", &reqs);
        assert_eq!(labels(&hits, "Requests"), ["Get films"]);
        assert_eq!(labels(&hits, "Sections"), ["Requests", "Tools", "Time"]);
        assert_eq!(
            labels(&hits, "Tools").len(),
            rp_devtools::all().len(),
            "every tool, since nothing has been narrowed yet"
        );
    }

    /// A miss is an empty list, never a list of everything — a palette that
    /// silently ignores the query is worse than one that says "no matches".
    #[test]
    fn a_query_matching_nothing_returns_nothing() {
        let reqs = [candidate("films.http", 0, "Get films")];
        assert!(hits("zzqqxx", &reqs).is_empty());
    }

    /// One query, two kinds, one list — with the groups in a fixed order so the
    /// rows do not shuffle between keystrokes.
    #[test]
    fn one_query_can_match_a_request_and_a_tool() {
        let reqs = [candidate("api.http", 3, "Base64 upload")];
        let hits = hits("base64", &reqs);
        assert_eq!(labels(&hits, "Requests"), ["Base64 upload"]);
        assert!(
            !labels(&hits, "Tools").is_empty(),
            "the base64 tools must be found through rp_devtools::search"
        );
        let groups: Vec<&str> = hits.iter().map(|h| h.row.group.as_ref()).collect();
        let first_tool = groups.iter().position(|g| *g == "Tools").unwrap();
        assert!(
            groups[..first_tool].iter().all(|g| *g == "Requests"),
            "requests come first and a group stays contiguous: {groups:?}"
        );
    }

    /// The query is what the user typed, spacing and shouting included.
    #[test]
    fn case_and_surrounding_whitespace_do_not_matter() {
        let reqs = [candidate("films.http", 0, "Get films")];
        let plain = hits("get films", &reqs);
        for query in ["  GET FILMS  ", "\tGet Films\n", "get films"] {
            assert_eq!(hits(query, &reqs), plain, "{query:?} must match the same");
        }
    }

    /// Two files, one request name. Both must be offered, and be told apart — this
    /// is the reason `detail` carries the file name at all.
    #[test]
    fn two_requests_with_one_name_are_both_offered_and_distinguishable() {
        let reqs = [
            candidate("staging.http", 0, "Login"),
            candidate("prod.http", 4, "Login"),
        ];
        let hits = hits("login", &reqs);
        let rows: Vec<_> = hits.iter().filter(|h| h.row.group == "Requests").collect();
        assert_eq!(rows.len(), 2, "neither may be swallowed as a duplicate");
        assert_ne!(
            rows[0].row.detail, rows[1].row.detail,
            "identical rows doing different things is the failure"
        );
        assert!(rows.iter().any(|h| h.row.detail.contains("staging.http")));
        assert!(rows.iter().any(|h| h.row.detail.contains("prod.http")));
        // And they open different things, which is what the detail promises.
        assert_ne!(rows[0].choice, rows[1].choice);
        assert_eq!(
            rows.iter()
                .filter(|h| h.choice
                    == Choice::Request {
                        source: PathBuf::from("prod.http"),
                        index: 4,
                    })
                .count(),
            1
        );
    }

    /// A prefix beats a match in the middle, or the row the user is typing the
    /// name of sits below unrelated ones.
    #[test]
    fn a_prefix_match_outranks_a_match_in_the_middle() {
        let reqs = [
            candidate("a.http", 0, "Refresh token"),
            candidate("a.http", 1, "Token exchange"),
        ];
        assert_eq!(
            labels(&hits("token", &reqs), "Requests"),
            ["Token exchange", "Refresh token"]
        );
    }

    /// The narrowing case the selection index has to survive: a query that goes
    /// from many rows to few leaves a stale index pointing past the end.
    #[test]
    fn a_narrowing_query_keeps_the_selection_in_range() {
        let reqs = [
            candidate("a.http", 0, "Login"),
            candidate("a.http", 1, "Logout"),
        ];
        let wide = hits("", &reqs);
        let narrow = hits("logout", &reqs);
        assert!(narrow.len() < wide.len());
        let stale = wide.len() - 1;
        let clamped = rp_ui::palette::clamp_selection(stale, narrow.len());
        assert!(
            clamped < narrow.len(),
            "Enter must land on a row that exists"
        );
        assert_eq!(clamped, narrow.len() - 1);
    }
}
