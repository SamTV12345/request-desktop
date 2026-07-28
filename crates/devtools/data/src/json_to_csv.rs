//! `json-to-csv`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Result, anyhow, bail};

/// it-tools' own (non-RFC) escaping: backslash-escape newlines and quotes, then
/// wrap in quotes only when a comma is present. Hand-rolled rather than using
/// the `csv` crate because the ported test vectors depend on exactly this.
pub(crate) fn csv_cell(v: Option<&serde_json::Value>) -> String {
    let v = match v {
        None => return String::new(),
        Some(serde_json::Value::Null) => return "null".into(),
        Some(v) => v,
    };
    let s = match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let s = s
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('"', "\\\"");
    if s.contains(',') {
        format!("\"{s}\"")
    } else {
        s
    }
}

/// Flatten one row object into `path -> value`, dot-separated.
///
/// it-tools PR #1277: `{"address":{"city":"Kiel"}}` should be an
/// `address.city` column, not a cell holding a JSON blob. Two cases stay
/// blobs on purpose:
///
/// * an array of scalars is joined into one cell, because a list of tags is
///   still one fact about the row;
/// * an array holding an object is a table inside a cell, which CSV has no
///   answer for, so it is left as its JSON text rather than exploded into
///   `tags.0.name` columns nobody asked for.
///
/// Depth is bounded by `serde_json`'s own 128-level parse limit, so the
/// recursion cannot be driven past that by input.
fn flatten(prefix: &str, v: &serde_json::Value, out: &mut Vec<(String, serde_json::Value)>) {
    use serde_json::Value;
    match v {
        // An object with no keys contributes no column: CSV cannot say
        // "present but empty" any differently from "missing".
        Value::Object(o) => {
            for (k, x) in o {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(&key, x, out);
            }
        }
        Value::Array(a) if a.iter().all(|x| !x.is_object() && !x.is_array()) => {
            let joined = a
                .iter()
                .map(|x| match x {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect::<Vec<_>>()
                .join(",");
            out.push((prefix.to_string(), Value::String(joined)));
        }
        scalar => out.push((prefix.to_string(), scalar.clone())),
    }
}

pub(crate) fn json_to_csv(args: &Args) -> Result<String> {
    let t = &TOOLS[2];
    // A single object is one row (PR #1277). Anything else that is not an array
    // has no row structure at all and is still refused.
    let rows = match json(args, t, "input")? {
        serde_json::Value::Array(a) => a,
        o @ serde_json::Value::Object(_) => vec![o],
        _ => bail!("expected a JSON object, or an array of objects"),
    };
    let flat: Vec<Vec<(String, serde_json::Value)>> = rows
        .iter()
        .map(|item| {
            if !item.is_object() {
                return Err(anyhow!("every array item must be an object"));
            }
            let mut cells = Vec::new();
            flatten("", item, &mut cells);
            Ok(cells)
        })
        .collect::<Result<_>>()?;
    // The column set is the union over every row, in first-seen order, so a key
    // only some rows carry still gets a column and blanks elsewhere.
    let mut headers: Vec<&str> = Vec::new();
    // The `seen` set is not decoration: a document with many distinct keys made
    // the old `Vec::contains` scan quadratic in the column count.
    let mut seen = std::collections::HashSet::new();
    for cells in &flat {
        for (k, _) in cells {
            if seen.insert(k.as_str()) {
                headers.push(k);
            }
        }
    }
    let mut out = headers.join(",");
    for cells in &flat {
        let by_key: std::collections::HashMap<&str, &serde_json::Value> =
            cells.iter().map(|(k, v)| (k.as_str(), v)).collect();
        out.push('\n');
        let row: Vec<String> = headers
            .iter()
            .map(|h| csv_cell(by_key.get(h).copied()))
            .collect();
        out.push_str(&row.join(","));
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "json-to-csv",
    name: "JSON to CSV",
    description: "Convert a JSON array of objects to CSV.",
    category: Category::Converter,
    keywords: &["json", "csv", "table", "convert"],
    fields: &[Field::area("input", "JSON array")],
    kind: Kind::Pure,
    run: json_to_csv,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;
    #[test]
    fn json_to_csv_vectors() {
        assert_eq!(
            ok(
                "json-to-csv",
                &[("input", r#"[{"a":1,"b":2},{"a":3,"b":4}]"#)]
            ),
            "a,b\n1,2\n3,4"
        );
        assert_eq!(
            ok(
                "json-to-csv",
                &[("input", r#"[{"a":1,"b":2},{"a":3,"c":4}]"#)]
            ),
            "a,b,c\n1,2,\n3,,4"
        );
        assert_eq!(
            ok("json-to-csv", &[("input", r#"[{"a":null,"b":2}]"#)]),
            "a,b\nnull,2"
        );
        assert_eq!(
            ok("json-to-csv", &[("input", r#"[{"b":2},{"b":3}]"#)]),
            "b\n2\n3"
        );
        assert_eq!(
            ok(
                "json-to-csv",
                &[("input", r#"[{"a":"hello, world","b":2}]"#)]
            ),
            "a,b\n\"hello, world\",2"
        );
        assert_eq!(
            ok(
                "json-to-csv",
                &[("input", r#"[{"a":"hello \"world\"","b":2}]"#)]
            ),
            "a,b\nhello \\\"world\\\",2"
        );
        assert_eq!(ok("json-to-csv", &[("input", "[]")]), "");
    }

    /// PR #1277: a bare object is one row, nested objects become dotted
    /// columns, and a list of scalars is one joined cell.
    #[test]
    fn json_to_csv_flattens_and_takes_a_single_object() {
        assert_eq!(
            ok("json-to-csv", &[("input", r#"{"a":1,"b":2}"#)]),
            "a,b\n1,2"
        );
        assert_eq!(
            ok(
                "json-to-csv",
                &[(
                    "input",
                    r#"{"name":"x","address":{"city":"Kiel","zip":24103}}"#
                )]
            ),
            "name,address.city,address.zip\nx,Kiel,24103"
        );
        // A scalar list is one cell, quoted because it now contains a comma.
        assert_eq!(
            ok("json-to-csv", &[("input", r#"[{"tags":["a","b"],"n":1}]"#)]),
            "tags,n\n\"a,b\",1"
        );
        // Rows with different shapes: union of columns, blanks for the gaps.
        assert_eq!(
            ok(
                "json-to-csv",
                &[("input", r#"[{"a":{"x":1}},{"a":{"y":2},"b":3}]"#)]
            ),
            "a.x,a.y,b\n1,,\n,2,3"
        );
        // A table inside a cell has no CSV spelling, so it stays JSON text.
        assert_eq!(
            ok("json-to-csv", &[("input", r#"[{"a":[{"b":1}]}]"#)]),
            "a\n[{\\\"b\\\":1}]"
        );
        // An object with no keys is a row with no columns.
        assert_eq!(ok("json-to-csv", &[("input", "{}")]), "\n");
    }

    #[test]
    fn json_to_csv_rejects_non_arrays() {
        assert!(run("json-to-csv", &[("input", "[1,2]")]).is_err());
        assert!(run("json-to-csv", &[("input", "[{")]).is_err());
        assert!(run("json-to-csv", &[("input", "")]).is_err());
    }

    // ── json / yaml / toml round trips ──────────────────────────────────
}
