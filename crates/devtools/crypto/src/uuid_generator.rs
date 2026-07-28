//! `uuid-generator`

use crate::*;
use crate::{Args, Category, Field, FieldKind, Kind, Tool};
use anyhow::{Context, Result, bail};

pub(crate) const NIL_UUID: &str = "00000000-0000-0000-0000-000000000000";

/// RFC 4122 name-based UUID (v3 = MD5, v5 = SHA-1). ponytail: five lines beats
/// enabling two more `uuid` features someone else owns.
pub(crate) fn uuid_name_based(namespace: &str, name: &str, version: u8) -> Result<String> {
    let ns = uuid::Uuid::parse_str(namespace.trim()).context("invalid namespace UUID")?;
    let mut data = ns.as_bytes().to_vec();
    data.extend_from_slice(name.as_bytes());
    let hash = hash_bytes(if version == 3 { "MD5" } else { "SHA1" }, &data)?;
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | (version << 4);
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(uuid::Uuid::from_bytes(bytes).to_string())
}

/// it-tools' `v35Args` default. Its namespace buttons offer DNS
/// (`6ba7b810-…`), URL, OID and X500, with URL preselected.
pub(crate) const URL_NAMESPACE: &str = "6ba7b811-9dad-11d1-80b4-00c04fd430c8";

pub(crate) fn run_uuid(a: &Args) -> Result<String> {
    let count = n(a, "count", 1);
    if !(1..=50).contains(&count) {
        bail!("quantity must be between 1 and 50");
    }
    let version = s(a, "version", "v4");
    let namespace = s(a, "namespace", URL_NAMESPACE);
    let name = s(a, "name", "");
    let mut out = String::new();
    for _ in 0..count {
        let id = match version {
            "NIL" => NIL_UUID.to_string(),
            "v1" => {
                let node: [u8; 6] = std::array::from_fn(|_| rand::random());
                uuid::Uuid::now_v1(&node).to_string()
            }
            "v3" => uuid_name_based(namespace, name, 3)?,
            "v4" => uuid::Uuid::new_v4().to_string(),
            "v5" => uuid_name_based(namespace, name, 5)?,
            // v6 is v1 with the timestamp fields reordered most-significant
            // first, so lexical order is time order — the fix for v1's terrible
            // index locality, but still leaking a node id.
            "v6" => {
                let node: [u8; 6] = std::array::from_fn(|_| rand::random());
                uuid::Uuid::now_v6(&node).to_string()
            }
            // v7 is 48 bits of Unix milliseconds followed by randomness: also
            // sortable, no node id, and today's default recommendation for a
            // database key.
            "v7" => uuid::Uuid::now_v7().to_string(),
            other => bail!("unknown UUID version: {other}"),
        };
        out.push_str(&id);
        out.push('\n');
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "uuid-generator",
    name: "UUID generator",
    description: "Generate NIL, v1, v3, v4, v5, v6 or v7 UUIDs.",
    category: Category::Crypto,
    keywords: &["uuid", "guid", "unique", "identifier", "v4", "v7", "nil"],
    fields: &[
        // Version order, as upstream lists them; v4 stays preselected.
        Field::select(
            "version",
            "UUID version",
            &["NIL", "v1", "v3", "v4", "v5", "v6", "v7"],
            "v4",
        ),
        Field::number("count", "Quantity", "1"),
        // Prefilled with the URL namespace, as the upstream buttons are.
        Field {
            key: "namespace",
            label: "Namespace (v3/v5)",
            kind: FieldKind::Text,
            default: URL_NAMESPACE,
            options: &[],
        },
        Field::text("name", "Name (v3/v5)"),
    ],
    kind: Kind::Pure,
    run: run_uuid,
};
