//! What the command palette finds, and what picking a row does.
//!
//! Everything the user can reach in one list: every request in every open file,
//! all 86 tools, the declared environments, the three sections and the Variables
//! view. The overlay itself is [`rp_ui::palette::Palette`] and knows none of that —
//! it takes rows and reports an index, and [`hits`] is what maps that index back
//! onto an action.
//!
//! Matching is [`rp_ui::palette::fuzzy`], which also hands back the ranges to
//! emphasise; the only thing done here is deciding what to match *against* and in
//! what order the groups come. Deliberately no frecency: ordering by what was
//! picked recently needs somewhere to persist the counts and a decay policy to stop
//! a burst of clicks pinning a row forever, and nobody has asked for it.
//!
//! [`hits`] is a free function over plain data on purpose: "which rows come back,
//! in what order" is the whole behaviour, and it is testable without a window, a
//! file on disk or a `Source`.
use crate::Section;
use rp_ui::palette::{Row, fuzzy};
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
    /// Make this environment the active one, by index into the workspace's list.
    /// The index, not the name: two loaded files can declare the same name and the
    /// workspace already addresses them positionally.
    Environment(usize),
    /// Open the Variables view. Not a `Section` — it is a tab in Requests, which is
    /// why it cannot ride along in [`SECTIONS`].
    Vars,
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

/// How well `query` matches a row whose visible label is `label`, and which parts
/// of the label to emphasise.
///
/// `also` are fields that are searched but not shown — a request's method, URL and
/// file. A hit there still offers the row, but scores 0 so it sorts below every
/// label match, and highlights nothing, because there is nothing on the row to
/// point at.
fn score(query: &str, label: &str, also: &[&str]) -> Option<(i32, Vec<std::ops::Range<usize>>)> {
    if let Some(m) = fuzzy(label, query) {
        return Some((m.score, m.ranges));
    }
    also.iter()
        .any(|h| fuzzy(h, query).is_some())
        .then(|| (0, Vec::new()))
}

/// Best score first, then the shorter label — for one score, "Get" is more likely
/// what was meant than "Get films from the archive". `sort_by` is stable, so a real
/// tie keeps the order the rows came in, which for requests is the order the files
/// were opened.
///
/// `query` is the trimmed, lowercased one: with nothing typed every score is 0 and
/// sorting would only shuffle the list by label length, so it is left alone. An
/// untyped palette is an index, and an index's order is the order things were
/// opened or declared.
fn rank(query: &str, mut matched: Vec<(i32, usize, Hit)>) -> impl Iterator<Item = Hit> {
    if !query.is_empty() {
        matched.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    }
    matched.into_iter().map(|(_, _, hit)| hit)
}

/// Every row `query` matches, grouped by kind.
///
/// An empty query matches everything: the palette opens onto the whole list rather
/// than an empty box, which is also what makes it usable as a plain index.
///
/// `envs` are the environment names in the workspace's own order, so the index a
/// [`Choice::Environment`] carries is the one `selected_env` expects. Names rather
/// than `rp_core::Environment`, so this stays exercisable from a test with two
/// string literals.
pub fn hits(query: &str, requests: &[Candidate<'_>], envs: &[&str]) -> Vec<Hit> {
    let q = query.trim().to_lowercase();
    let mut out = Vec::new();

    let mut matched: Vec<(i32, usize, Hit)> = Vec::new();
    for c in requests {
        // The file name is searched too, so "films.http" finds everything in it.
        let Some((score, ranges)) = score(&q, c.name, &[c.method, c.url, c.file]) else {
            continue;
        };
        matched.push((
            score,
            c.name.len(),
            Hit {
                row: Row::new("Requests", c.name.to_string())
                    .detail(format!("{} · {}", c.method.to_uppercase(), c.file))
                    .icon("send")
                    .highlight(ranges),
                choice: Choice::Request {
                    source: c.source.to_path_buf(),
                    index: c.index,
                },
            },
        ));
    }
    out.extend(rank(&q, matched));

    // The same three haystacks `rp_devtools::search` uses — name, id, keywords —
    // but fuzzy, and keeping the name's ranges so the row can show where it hit.
    // Deliberately *not* calling `search`: it is a substring filter, so it would
    // throw away every fuzzy hit before this code ever saw it.
    let mut matched: Vec<(i32, usize, Hit)> = Vec::new();
    for tool in rp_devtools::all() {
        let mut also: Vec<&str> = Vec::with_capacity(tool.keywords.len() + 1);
        also.push(tool.id);
        also.extend_from_slice(tool.keywords);
        let Some((score, ranges)) = score(&q, tool.name, &also) else {
            continue;
        };
        matched.push((
            score,
            tool.name.len(),
            Hit {
                row: Row::new("Tools", tool.name)
                    .detail(tool.description)
                    .icon("wrench")
                    .highlight(ranges),
                choice: Choice::Tool(tool.id),
            },
        ));
    }
    out.extend(rank(&q, matched));

    // Environments, so switching one is a search rather than a trip to the
    // Variables tab. Ranked like the rest, since a workspace can declare a dozen.
    let mut matched: Vec<(i32, usize, Hit)> = Vec::new();
    for (i, name) in envs.iter().enumerate() {
        if let Some(m) = fuzzy(name, &q) {
            matched.push((
                m.score,
                name.len(),
                Hit {
                    row: Row::new("Environments", name.to_string())
                        .detail("Use this environment")
                        .icon("settings")
                        .highlight(m.ranges),
                    choice: Choice::Environment(i),
                },
            ));
        }
    }
    out.extend(rank(&q, matched));

    for (label, glyph, section) in SECTIONS {
        if let Some(m) = fuzzy(label, &q) {
            out.push(Hit {
                row: Row::new("Sections", label)
                    .detail("Go to section")
                    .icon(glyph)
                    .highlight(m.ranges),
                choice: Choice::Section(section),
            });
        }
    }
    // Last, and in the Sections group: it is a view you go to, and the same glyph
    // the toolbar button uses so the two read as one thing.
    if let Some(m) = fuzzy("Variables", &q) {
        out.push(Hit {
            row: Row::new("Sections", "Variables")
                .detail("Environment and captured values")
                .icon("settings")
                .highlight(m.ranges),
            choice: Choice::Vars,
        });
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
        let hits = hits("", &reqs, &["local", "prod"]);
        assert_eq!(labels(&hits, "Requests"), ["Get films"]);
        assert_eq!(labels(&hits, "Environments"), ["local", "prod"]);
        assert_eq!(
            labels(&hits, "Sections"),
            ["Requests", "Tools", "Time", "Variables"]
        );
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
        assert!(hits("zzqqxx", &reqs, &[]).is_empty());
    }

    /// One query, two kinds, one list — with the groups in a fixed order so the
    /// rows do not shuffle between keystrokes.
    #[test]
    fn one_query_can_match_a_request_and_a_tool() {
        let reqs = [candidate("api.http", 3, "Base64 upload")];
        let hits = hits("base64", &reqs, &[]);
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
        let plain = hits("get films", &reqs, &[]);
        for query in ["  GET FILMS  ", "\tGet Films\n", "get films"] {
            assert_eq!(
                hits(query, &reqs, &[]),
                plain,
                "{query:?} must match the same"
            );
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
        let hits = hits("login", &reqs, &[]);
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
            labels(&hits("token", &reqs, &[]), "Requests"),
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
        let wide = hits("", &reqs, &[]);
        let narrow = hits("logout", &reqs, &[]);
        assert!(narrow.len() < wide.len());
        let stale = wide.len() - 1;
        let clamped = rp_ui::palette::clamp_selection(stale, narrow.len());
        assert!(
            clamped < narrow.len(),
            "Enter must land on a row that exists"
        );
        assert_eq!(clamped, narrow.len() - 1);
    }

    /// The reason the matcher was replaced: initials have to find the tool.
    #[test]
    fn initials_find_a_tool_a_substring_search_would_miss() {
        let found = hits("htst", &[], &[]);
        assert!(
            labels(&found, "Tools")
                .iter()
                .any(|l| l == "HTTP status codes"),
            "got {:?}",
            labels(&found, "Tools")
        );
        // And the row carries where it hit, or the highlight has nothing to draw.
        let row = &found
            .iter()
            .find(|h| h.row.label == "HTTP status codes")
            .expect("row")
            .row;
        assert!(!row.highlight.is_empty());
        let cut: Vec<&str> = row
            .highlight
            .iter()
            .map(|r| &row.label.as_ref()[r.clone()])
            .collect();
        assert_eq!(cut, ["HT", "st"]);
    }

    /// A hit only in the URL still offers the request, but with nothing to
    /// emphasise — pointing at a substring the row does not show is worse than
    /// pointing at nothing.
    #[test]
    fn a_hit_in_a_hidden_field_highlights_nothing() {
        let reqs = [candidate("a.http", 0, "Login")];
        let found = hits("example", &reqs, &[]);
        let row = &found
            .iter()
            .find(|h| h.row.group == "Requests")
            .expect("the url matched")
            .row;
        assert!(row.highlight.is_empty());
    }

    /// The two groups that were missing, and that picking them does the right
    /// thing rather than switching section.
    #[test]
    fn environments_and_the_variables_view_are_offered() {
        let envs = ["local", "production"];
        let found = hits("prod", &[], &envs);
        assert_eq!(labels(&found, "Environments"), ["production"]);
        assert_eq!(
            found
                .iter()
                .find(|h| h.row.group == "Environments")
                .map(|h| h.choice.clone()),
            Some(Choice::Environment(1)),
            "the index into the workspace's list, not the filtered one"
        );

        let found = hits("vars", &[], &envs);
        assert_eq!(
            found
                .iter()
                .find(|h| h.row.label == "Variables")
                .map(|h| h.choice.clone()),
            Some(Choice::Vars),
            "`vars` is not a substring of `Variables`, which is the point"
        );
    }
}
