//! `list-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::Result;

// The select shows these strings, so they are the values too.
pub(crate) const SORT_NONE: &str = "Do not sort";
pub(crate) const SORT_ASC: &str = "Ascending";
pub(crate) const SORT_DESC: &str = "Descending";

/// Operations run in a fixed order, and it is this one:
///
/// 1. lowercase the whole text,
/// 2. cut it into items at line breaks,
/// 3. cut each item further at `splitOn`, if one is given (PR #1288),
/// 4. remove duplicates,
/// 5. reverse,
/// 6. sort,
/// 7. trim each item,
/// 8. strip `removeItemPrefix` / `removeItemSuffix` (PR #1302),
/// 9. drop items that are now empty,
/// 10. add the item prefix/suffix, join with the separator, wrap in the list
///     prefix/suffix.
///
/// Steps 1–2 and 4–7 keep the order the port already had, so nothing that
/// worked changes. The two new cutting and stripping steps sit where they are
/// useful: splitting before deduplication (so duplicates *across* the split are
/// caught) and stripping after trimming (so `  "a"  ` loses its padding first,
/// and the quotes second).
pub(crate) fn list_converter(args: &Args) -> Result<String> {
    let t = &TOOLS[16];
    let raw = arg(args, t, "input");
    let keep_breaks = flag(args, t, "keepLineBreaks");
    let line_break = if keep_breaks { "\n" } else { "" };

    let lowered = if flag(args, t, "lowerCase") {
        raw.to_lowercase()
    } else {
        raw.to_string()
    };
    let mut parts: Vec<String> = lowered.split('\n').map(str::to_string).collect();

    let split_on = arg(args, t, "splitOn");
    if !split_on.is_empty() {
        parts = parts
            .iter()
            .flat_map(|p| p.split(split_on).map(str::to_string))
            .collect();
    }

    if flag(args, t, "removeDuplicates") {
        let mut seen = std::collections::HashSet::new();
        parts.retain(|p| seen.insert(p.clone()));
    }
    if flag(args, t, "reverseList") {
        parts.reverse();
    }
    match arg(args, t, "sortList") {
        SORT_ASC => parts.sort(),
        SORT_DESC => {
            parts.sort();
            parts.reverse();
        }
        _ => {}
    }
    if flag(args, t, "trimItems") {
        parts = parts.iter().map(|p| p.trim().to_string()).collect();
    }
    let drop_prefix = arg(args, t, "removeItemPrefix");
    let drop_suffix = arg(args, t, "removeItemSuffix");
    if !drop_prefix.is_empty() || !drop_suffix.is_empty() {
        parts = parts
            .iter()
            .map(|p| {
                let p = p.strip_prefix(drop_prefix).unwrap_or(p);
                p.strip_suffix(drop_suffix).unwrap_or(p).to_string()
            })
            .collect();
    }
    parts.retain(|p| !p.is_empty());

    let prefix = arg(args, t, "itemPrefix");
    let suffix = arg(args, t, "itemSuffix");
    let body = parts
        .iter()
        .map(|p| format!("{prefix}{p}{suffix}"))
        .collect::<Vec<_>>()
        .join(&format!("{}{line_break}", arg(args, t, "separator")));

    Ok([
        arg(args, t, "listPrefix"),
        &body,
        arg(args, t, "listSuffix"),
    ]
    .join(line_break))
}

pub static TOOL: Tool = Tool {
    id: "list-converter",
    name: "List converter",
    description: "Split, join, sort, dedupe and decorate the lines of a list.",
    category: Category::Data,
    keywords: &[
        "list", "join", "split", "sort", "dedupe", "prefix", "convert",
    ],
    fields: &[
        Field::area("input", "List (one item per line)"),
        // it-tools defaults the separator to ", " and dedupes by default.
        Field {
            key: "separator",
            label: "Separator",
            kind: crate::FieldKind::Text,
            default: ", ",
            options: &[],
        },
        Field::text("itemPrefix", "Item prefix"),
        Field::text("itemSuffix", "Item suffix"),
        Field::text("removeItemPrefix", "Remove item prefix"),
        Field::text("removeItemSuffix", "Remove item suffix"),
        Field::text("splitOn", "Also split items on"),
        Field::text("listPrefix", "List prefix"),
        Field::text("listSuffix", "List suffix"),
        Field::bool("trimItems", "Trim items", "true"),
        Field::bool("removeDuplicates", "Remove duplicates", "true"),
        Field::bool("reverseList", "Reverse list", "false"),
        // "Do not sort" is spelled out rather than left as an empty option: a
        // blank entry in the list reads as "not loaded yet", not as a choice.
        Field::select(
            "sortList",
            "Sort",
            &[SORT_NONE, SORT_ASC, SORT_DESC],
            SORT_NONE,
        ),
        Field::bool("lowerCase", "Lowercase", "false"),
        Field::bool("keepLineBreaks", "Keep line breaks", "false"),
    ],
    kind: Kind::Pure,
    run: list_converter,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;
    #[test]
    fn list_converter_vectors() {
        let base = [
            ("separator", ", "),
            ("trimItems", "true"),
            ("removeDuplicates", "true"),
            ("itemPrefix", "\""),
            ("itemSuffix", "\""),
        ];
        let mut a = base.to_vec();
        a.push((
            "input",
            "\n        1\n        2\n        \n        3\n        3\n        4\n        ",
        ));
        assert_eq!(ok("list-converter", &a), "\"1\", \"2\", \"3\", \"4\"");

        assert_eq!(
            ok("list-converter", &[("input", ""), ("trimItems", "true")]),
            ""
        );

        assert_eq!(
            ok(
                "list-converter",
                &[
                    ("input", "\n        1\n        2\n        3\n        "),
                    ("separator", ""),
                    ("trimItems", "true"),
                    ("itemPrefix", "<li>"),
                    ("itemSuffix", "</li>"),
                    ("listPrefix", "<ul>"),
                    ("listSuffix", "</ul>"),
                    ("keepLineBreaks", "true"),
                ]
            ),
            "<ul>\n<li>1</li>\n<li>2</li>\n<li>3</li>\n</ul>"
        );
    }

    /// PRs #1302/#1288, and the documented order of operations.
    #[test]
    fn list_converter_splits_strips_and_sorts() {
        // Split on a separator, then dedupe across the split.
        assert_eq!(
            ok(
                "list-converter",
                &[("input", "a,b\nb,c"), ("splitOn", ","), ("separator", "|")]
            ),
            "a|b|c"
        );
        // Deduplication happens before trimming (step 4 before step 7), so
        // `b` and ` b` are still two items. That order is the port's, kept.
        assert_eq!(
            ok(
                "list-converter",
                &[
                    ("input", "a, b\nb, c"),
                    ("splitOn", ","),
                    ("separator", "|")
                ]
            ),
            "a|b|b|c"
        );
        // Strip the decoration a previous run added.
        assert_eq!(
            ok(
                "list-converter",
                &[
                    ("input", "\"a\"\n\"b\""),
                    ("removeItemPrefix", "\""),
                    ("removeItemSuffix", "\""),
                    ("separator", ",")
                ]
            ),
            "a,b"
        );
        // Sorting is opt-in, and the default option says so.
        let unsorted = &[("input", "b\na\nc"), ("separator", ",")];
        assert_eq!(ok("list-converter", unsorted), "b,a,c");
        let sorted = |dir: &str| {
            ok(
                "list-converter",
                &[("input", "b\na\nc"), ("separator", ","), ("sortList", dir)],
            )
        };
        assert_eq!(sorted(super::SORT_NONE), "b,a,c");
        assert_eq!(sorted(super::SORT_ASC), "a,b,c");
        assert_eq!(sorted(super::SORT_DESC), "c,b,a");
        // Duplicates go whether or not anything else is on.
        assert_eq!(
            ok(
                "list-converter",
                &[
                    ("input", "a\na\nb"),
                    ("separator", ","),
                    ("removeDuplicates", "true")
                ]
            ),
            "a,b"
        );
        assert_eq!(
            ok(
                "list-converter",
                &[
                    ("input", "a\na\nb"),
                    ("separator", ","),
                    ("removeDuplicates", "false")
                ]
            ),
            "a,a,b"
        );
    }
}
