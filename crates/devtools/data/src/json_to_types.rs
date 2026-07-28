//! `json-to-types` — it-tools PRs #1002, #1031, #1102, #1192, #1105, #1246, #1811.
//!
//! Upstream this arrived as five near-identical pull requests, one per target
//! language (TypeScript, Go, Rust, Java, C#), each re-walking the same JSON with
//! the same rules and differing only in how it prints a field. That is one tool
//! wearing five hats: the analysis is shared, so it is written once here and the
//! language is a select. A language picker is not a direction toggle — there is
//! no inverse operation to be in the wrong half of, and six panels of
//! generated code at once would be noise, not help.

use crate::*;
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::fmt::Write as _;

/// The shape of one value, after the JSON has been reduced to something a
/// static type system can name.
#[derive(Clone)]
enum Ty {
    Bool,
    Int,
    Float,
    Str,
    /// `null`, an empty array, or a value past the depth cap: nothing to infer.
    Any,
    Array(Box<Ty>),
    /// A named struct/interface/class, defined in the `Def` list.
    Object(String),
}

struct FieldDef {
    /// The key exactly as it appears in the JSON, for the tag/attribute.
    json: String,
    ty: Ty,
    nullable: bool,
}

struct Def {
    name: String,
    fields: Vec<FieldDef>,
}

/// serde_json refuses to parse past ~128 levels itself, so this only has to
/// stop *our* recursion from being the thing that overflows the stack. Well
/// under the parser's own ceiling, and deeper than any hand-written document.
const MAX_DEPTH: usize = 24;

pub(crate) const SAMPLE: &str = r#"{
  "id": 42,
  "user_name": "ada",
  "score": 9.5,
  "admin": true,
  "tags": ["math", "engines"],
  "address": { "city": "London", "postal_code": null }
}"#;

const LANGUAGES: &[&str] = &["TypeScript", "Go", "Rust", "Java", "C#", "PHP"];

// ── naming ──────────────────────────────────────────────────────────────────

/// Split an arbitrary JSON key into words, so it can be re-cased. Handles
/// `postal_code`, `postal-code`, `postalCode` and `POSTALCode` alike.
fn words(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in raw.chars() {
        if c.is_alphanumeric() {
            if c.is_uppercase() && prev_lower && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = !c.is_uppercase();
            cur.push(c);
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
            prev_lower = false;
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn cap(w: &str) -> String {
    let mut c = w.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// No identifier may start with a digit, and a key of `""` or `"!!"` has no
/// letters at all — both fall back rather than emitting code that will not
/// compile.
fn safe(ident: String, fallback: &str) -> String {
    match ident.chars().next() {
        None => fallback.to_string(),
        Some(c) if c.is_ascii_digit() => format!("_{ident}"),
        _ => ident,
    }
}

fn pascal(raw: &str) -> String {
    safe(words(raw).iter().map(|w| cap(w)).collect(), "Field")
}

fn camel(raw: &str) -> String {
    let ws = words(raw);
    let mut out = String::new();
    for (i, w) in ws.iter().enumerate() {
        if i == 0 {
            out.push_str(&w.to_lowercase());
        } else {
            out.push_str(&cap(w));
        }
    }
    safe(out, "field")
}

fn snake(raw: &str) -> String {
    safe(
        words(raw)
            .iter()
            .map(|w| w.to_lowercase())
            .collect::<Vec<_>>()
            .join("_"),
        "field",
    )
}

/// `tags` holds a `Tag`. Crude on purpose — the alternative is an English
/// pluralisation table, and a wrong type name is renamed in two keystrokes.
fn singular(hint: &str) -> String {
    match hint.strip_suffix('s') {
        Some(stem) if !stem.is_empty() && !stem.ends_with('s') => stem.to_string(),
        _ => hint.to_string(),
    }
}

// ── analysis ────────────────────────────────────────────────────────────────

/// Reduce one value to a [`Ty`], appending a definition for every object met.
///
/// An array takes the type of its *first* element. Merging all elements would
/// mean union types that only TypeScript can express; the first element is what
/// every upstream PR did, and a sample document that disagrees with itself is
/// the rarer problem.
fn walk(v: &Value, hint: &str, defs: &mut Vec<Def>, depth: usize) -> Ty {
    if depth > MAX_DEPTH {
        return Ty::Any;
    }
    match v {
        Value::Null => Ty::Any,
        Value::Bool(_) => Ty::Bool,
        Value::Number(n) => {
            if n.is_f64() {
                Ty::Float
            } else {
                Ty::Int
            }
        }
        Value::String(_) => Ty::Str,
        Value::Array(items) => Ty::Array(Box::new(match items.first() {
            Some(first) => walk(first, &singular(hint), defs, depth + 1),
            None => Ty::Any,
        })),
        Value::Object(map) => {
            let mut name = pascal(hint);
            // Two `address` objects in different branches would otherwise
            // collide into one definition that fits neither.
            let mut n = 2;
            while defs.iter().any(|d| d.name == name) {
                name = format!("{}{n}", pascal(hint));
                n += 1;
            }
            let at = defs.len();
            defs.push(Def {
                name: name.clone(),
                fields: Vec::new(),
            });
            for (k, child) in map {
                let ty = walk(child, k, defs, depth + 1);
                defs[at].fields.push(FieldDef {
                    json: k.clone(),
                    ty,
                    nullable: child.is_null(),
                });
            }
            Ty::Object(name)
        }
    }
}

// ── rendering ───────────────────────────────────────────────────────────────

/// Index of `lang` in the per-primitive tables below.
fn slot(lang: &str) -> usize {
    LANGUAGES.iter().position(|l| *l == lang).unwrap_or(0)
}

fn ty_name(ty: &Ty, lang: &str) -> String {
    let i = slot(lang);
    match ty {
        Ty::Object(n) => n.clone(),
        Ty::Array(inner) => {
            let e = ty_name(inner, lang);
            match lang {
                "TypeScript" => format!("{e}[]"),
                "Go" => format!("[]{e}"),
                "Rust" => format!("Vec<{e}>"),
                "Java" | "C#" => format!("List<{e}>"),
                // PHP arrays are untyped; the element type goes in the docblock.
                _ => "array".to_string(),
            }
        }
        prim => {
            let row: [&str; 6] = match prim {
                Ty::Bool => ["boolean", "bool", "bool", "Boolean", "bool", "bool"],
                Ty::Int => ["number", "int64", "i64", "long", "long", "int"],
                Ty::Float => ["number", "float64", "f64", "double", "double", "float"],
                Ty::Str => ["string", "string", "String", "String", "string", "string"],
                _ => [
                    "unknown",
                    "any",
                    "serde_json::Value",
                    "Object",
                    "object",
                    "mixed",
                ],
            };
            row[i].to_string()
        }
    }
}

/// The nullable form of an already-rendered type name.
fn nullable(name: &str, ty: &Ty, lang: &str) -> String {
    match lang {
        "TypeScript" => format!("{name} | null"),
        // A nil slice is already the empty case; `*[]T` helps nobody.
        "Go" if matches!(ty, Ty::Array(_)) => name.to_string(),
        "Go" if name == "any" => name.to_string(),
        "Go" => format!("*{name}"),
        "Rust" => format!("Option<{name}>"),
        // Java needs the boxed form to hold null.
        "Java" => match name {
            "long" => "Long".to_string(),
            "double" => "Double".to_string(),
            _ => name.to_string(),
        },
        "C#" => format!("{name}?"),
        _ if name == "mixed" || name == "array" => name.to_string(),
        _ => format!("?{name}"),
    }
}

fn render(defs: &[Def], lang: &str) -> String {
    let mut out = String::new();
    for def in defs {
        match lang {
            "TypeScript" => {
                let _ = writeln!(out, "export interface {} {{", def.name);
                for f in &def.fields {
                    let t = ty_name(&f.ty, lang);
                    let t = if f.nullable {
                        nullable(&t, &f.ty, lang)
                    } else {
                        t
                    };
                    // TS keeps the JSON key verbatim; only one that is not a
                    // plain identifier has to be quoted.
                    let plain = !f.json.is_empty()
                        && !f.json.starts_with(|c: char| c.is_ascii_digit())
                        && f.json
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
                    let key = if plain {
                        f.json.clone()
                    } else {
                        format!("{:?}", f.json)
                    };
                    let opt = if f.nullable { "?" } else { "" };
                    let _ = writeln!(out, "  {key}{opt}: {t};");
                }
                out.push_str("}\n\n");
            }
            "Go" => {
                let _ = writeln!(out, "type {} struct {{", def.name);
                for f in &def.fields {
                    let t = ty_name(&f.ty, lang);
                    let t = if f.nullable {
                        nullable(&t, &f.ty, lang)
                    } else {
                        t
                    };
                    let _ = writeln!(
                        out,
                        "\t{} {t} `json:\"{}\"`",
                        pascal(&f.json),
                        f.json.replace('`', "")
                    );
                }
                out.push_str("}\n\n");
            }
            "Rust" => {
                out.push_str("#[derive(Serialize, Deserialize)]\n");
                let _ = writeln!(out, "pub struct {} {{", def.name);
                for f in &def.fields {
                    let t = ty_name(&f.ty, lang);
                    let t = if f.nullable {
                        nullable(&t, &f.ty, lang)
                    } else {
                        t
                    };
                    let name = snake(&f.json);
                    if name != f.json {
                        let _ = writeln!(out, "    #[serde(rename = {:?})]", f.json);
                    }
                    let _ = writeln!(out, "    pub {name}: {t},");
                }
                out.push_str("}\n\n");
            }
            "Java" => {
                // A record is the shortest honest form for a data holder, and
                // the components carry the mapping annotation just as fields do.
                let _ = writeln!(out, "public record {}(", def.name);
                let body: Vec<String> = def
                    .fields
                    .iter()
                    .map(|f| {
                        let t = ty_name(&f.ty, lang);
                        let t = if f.nullable {
                            nullable(&t, &f.ty, lang)
                        } else {
                            t
                        };
                        let name = camel(&f.json);
                        let ann = if name == f.json {
                            String::new()
                        } else {
                            format!("@JsonProperty({:?}) ", f.json)
                        };
                        format!("    {ann}{t} {name}")
                    })
                    .collect();
                out.push_str(&body.join(",\n"));
                out.push_str("\n) {}\n\n");
            }
            "C#" => {
                let _ = writeln!(out, "public class {}\n{{", def.name);
                for f in &def.fields {
                    let t = ty_name(&f.ty, lang);
                    let t = if f.nullable {
                        nullable(&t, &f.ty, lang)
                    } else {
                        t
                    };
                    let _ = writeln!(out, "    [JsonPropertyName({:?})]", f.json);
                    let _ = writeln!(out, "    public {t} {} {{ get; set; }}", pascal(&f.json));
                }
                out.push_str("}\n\n");
            }
            _ => {
                let _ = writeln!(out, "class {}\n{{", def.name);
                for f in &def.fields {
                    let t = ty_name(&f.ty, lang);
                    let t = if f.nullable {
                        nullable(&t, &f.ty, lang)
                    } else {
                        t
                    };
                    let name = camel(&f.json);
                    // PHP has no attribute every decoder agrees on, so the
                    // original key goes in a docblock where it stays readable.
                    if name != f.json {
                        let _ = writeln!(out, "    /** @var mixed json: {} */", f.json);
                    }
                    let _ = writeln!(out, "    public {t} ${name};");
                }
                out.push_str("}\n\n");
            }
        }
    }
    out.trim_end().to_string() + "\n"
}

fn json_to_types(a: &Args) -> Result<String> {
    let raw = s(a, "input", SAMPLE).trim();
    if raw.is_empty() {
        bail!("paste a sample JSON document — the type definitions are inferred from it");
    }
    let lang = s(a, "language", "TypeScript");
    if !LANGUAGES.contains(&lang) {
        bail!(
            "unknown target language {lang:?}; pick one of {}",
            LANGUAGES.join(", ")
        );
    }
    let root = {
        let r = s(a, "rootName", "Root").trim();
        if r.is_empty() { "Root" } else { r }
    };
    let value: Value = serde_json::from_str(raw).context("invalid JSON")?;
    let mut defs = Vec::new();
    let ty = walk(&value, root, &mut defs, 0);
    if defs.is_empty() {
        // A scalar or an array of scalars has no object to name, so say what
        // it is rather than emitting an empty file.
        return Ok(format!(
            "// {} is {}, which needs no type definition\n",
            pascal(root),
            ty_name(&ty, lang)
        ));
    }
    Ok(render(&defs, lang))
}

pub static TOOL: Tool = Tool {
    id: "json-to-types",
    name: "JSON to type definitions",
    description: "Generate TypeScript, Go, Rust, Java, C# or PHP types from a sample JSON document.",
    category: Category::Data,
    keywords: &[
        "json",
        "types",
        "typescript",
        "interface",
        "go",
        "struct",
        "rust",
        "java",
        "csharp",
        "php",
        "codegen",
    ],
    fields: &[
        Field::area("input", "Sample JSON").with_default(SAMPLE),
        Field::select("language", "Target language", LANGUAGES, "TypeScript"),
        Field::text("rootName", "Root type name").with_default("Root"),
    ],
    kind: Kind::Pure,
    run: json_to_types,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    const DOC: &str = r#"{"id":42,"user_name":"ada","score":9.5,"admin":true,
        "tags":["math"],"address":{"city":"London","postal_code":null}}"#;

    #[test]
    fn typescript_is_the_default_and_nests() {
        let out = ok("json-to-types", &[("input", DOC)]);
        assert!(out.contains("export interface Root {"), "{out}");
        assert!(out.contains("  id: number;"), "{out}");
        assert!(out.contains("  admin: boolean;"), "{out}");
        assert!(out.contains("  tags: string[];"), "{out}");
        assert!(out.contains("  address: Address;"), "{out}");
        assert!(out.contains("export interface Address {"), "{out}");
        // null is optional and nullable, with no type to guess at.
        assert!(out.contains("postal_code?: unknown | null;"), "{out}");
    }

    #[test]
    fn every_language_uses_its_own_conventions() {
        let go = ok("json-to-types", &[("input", DOC), ("language", "Go")]);
        assert!(go.contains("type Root struct {"), "{go}");
        assert!(
            go.contains("\tUserName string `json:\"user_name\"`"),
            "{go}"
        );
        assert!(go.contains("\tId int64 `json:\"id\"`"), "{go}");
        assert!(go.contains("\tScore float64 `json:\"score\"`"), "{go}");
        assert!(go.contains("\tTags []string `json:\"tags\"`"), "{go}");

        let rust = ok("json-to-types", &[("input", DOC), ("language", "Rust")]);
        assert!(rust.contains("#[derive(Serialize, Deserialize)]"), "{rust}");
        assert!(rust.contains("pub user_name: String,"), "{rust}");
        // snake_case already matches, so no rename is emitted for it.
        assert!(!rust.contains("rename = \"user_name\""), "{rust}");
        assert!(rust.contains("pub tags: Vec<String>,"), "{rust}");
        assert!(
            rust.contains("pub postal_code: Option<serde_json::Value>,"),
            "{rust}"
        );

        let java = ok("json-to-types", &[("input", DOC), ("language", "Java")]);
        assert!(java.contains("public record Root("), "{java}");
        assert!(
            java.contains("@JsonProperty(\"user_name\") String userName"),
            "{java}"
        );
        assert!(java.contains("long id"), "{java}");
        assert!(java.contains("List<String> tags"), "{java}");

        let cs = ok("json-to-types", &[("input", DOC), ("language", "C#")]);
        assert!(cs.contains("public class Root"), "{cs}");
        assert!(cs.contains("[JsonPropertyName(\"user_name\")]"), "{cs}");
        assert!(cs.contains("public string UserName { get; set; }"), "{cs}");
        assert!(
            cs.contains("public object? PostalCode { get; set; }"),
            "{cs}"
        );

        let php = ok("json-to-types", &[("input", DOC), ("language", "PHP")]);
        assert!(php.contains("class Root"), "{php}");
        assert!(php.contains("public string $userName;"), "{php}");
        assert!(php.contains("public int $id;"), "{php}");
        assert!(php.contains("public array $tags;"), "{php}");
    }

    #[test]
    fn names_are_the_users_call() {
        let out = ok(
            "json-to-types",
            &[("input", r#"{"a":{"b":1}}"#), ("rootName", "Payload")],
        );
        assert!(out.contains("export interface Payload {"), "{out}");
        assert!(out.contains("export interface A {"), "{out}");
        // An array of objects names its element type in the singular.
        let list = ok("json-to-types", &[("input", r#"{"users":[{"n":1}]}"#)]);
        assert!(list.contains("export interface User {"), "{list}");
        assert!(list.contains("users: User[];"), "{list}");
        // Two objects that would share a name do not merge.
        let twice = ok(
            "json-to-types",
            &[("input", r#"{"a":{"x":1},"b":{"a":{"y":2}}}"#)],
        );
        assert!(twice.contains("interface A {"), "{twice}");
        assert!(twice.contains("interface A2 {"), "{twice}");
    }

    #[test]
    fn empty_args_show_the_sample() {
        let out = ok("json-to-types", &[]);
        assert!(out.contains("export interface Root {"), "{out}");
        assert!(run("json-to-types", &[("input", "")]).is_err());
        assert!(run("json-to-types", &[("input", "{\"a\":")]).is_err());
        assert!(run("json-to-types", &[("input", DOC), ("language", "COBOL")]).is_err());
        // A blank root name falls back rather than emitting `interface  {`.
        let blank = ok("json-to-types", &[("input", DOC), ("rootName", "  ")]);
        assert!(blank.contains("interface Root {"), "{blank}");
    }

    #[test]
    fn survives_hostile_documents() {
        // No object at all: a sentence, not an empty file.
        let scalar = ok("json-to-types", &[("input", "42")]);
        assert!(scalar.contains("needs no type definition"), "{scalar}");
        // Keys that are not identifiers anywhere.
        let odd = ok(
            "json-to-types",
            &[
                ("input", r#"{"":1,"9lives":2,"!!":3}"#),
                ("language", "Rust"),
            ],
        );
        assert!(odd.contains("pub field: i64,"), "{odd}");
        assert!(odd.contains("pub _9lives: i64,"), "{odd}");
        // Deep nesting: serde_json stops first, and our own cap holds after it.
        let deep = format!("{}1{}", "[".repeat(10_000), "]".repeat(10_000));
        assert!(run("json-to-types", &[("input", &deep)]).is_err());
        let braces = "{".repeat(10_000);
        assert!(run("json-to-types", &[("input", &braces)]).is_err());
        let ok_deep = format!("{{{}\"x\":1{}}}", "\"a\":{".repeat(40), "}".repeat(40));
        assert!(run("json-to-types", &[("input", &ok_deep)]).is_ok());
        // A megabyte of string, and numbers no integer type holds.
        let big = format!("{{\"s\":\"{}\"}}", "x".repeat(1_000_000));
        assert!(ok("json-to-types", &[("input", &big)]).contains("s: string;"));
        let huge = r#"{"n":123456789012345678901234567890,"m":-1e308}"#;
        assert!(ok("json-to-types", &[("input", huge)]).contains("n: number;"));
    }
}
