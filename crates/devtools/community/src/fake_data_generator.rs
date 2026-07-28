//! `fake-data-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Result, bail};
use serde_json::Value;
use std::fmt::Write as _;

#[rustfmt::skip]
pub(crate) const FIRST_EN: &[&str] = &[
    "Emma", "Liam", "Olivia", "Noah", "Ava", "Ethan", "Sophia", "Mason", "Isabella",
    "Lucas", "Mia", "Logan", "Charlotte", "James", "Amelia", "Benjamin", "Harper",
    "Elijah", "Evelyn", "Henry", "Abigail", "Alexander", "Ella", "Daniel", "Scarlett",
    "Matthew", "Grace", "Samuel", "Chloe", "David",
];
#[rustfmt::skip]
pub(crate) const LAST_EN: &[&str] = &[
    "Smith", "Johnson", "Williams", "Brown", "Jones", "Garcia", "Miller", "Davis",
    "Rodriguez", "Martinez", "Hernandez", "Lopez", "Wilson", "Anderson", "Thomas",
    "Taylor", "Moore", "Jackson", "Martin", "Lee", "Perez", "Thompson", "White",
    "Harris", "Clark", "Lewis", "Robinson", "Walker", "Young", "Allen",
];
#[rustfmt::skip]
pub(crate) const FIRST_DE: &[&str] = &[
    "Lukas", "Marie", "Jonas", "Sophie", "Felix", "Hannah", "Leon", "Emilia", "Paul",
    "Lena", "Maximilian", "Johanna", "Elias", "Clara", "Moritz", "Greta", "Julian",
    "Frieda", "Tobias", "Katharina", "Jannik", "Annika", "Sebastian", "Charlotte",
    "Matthias", "Ines", "Philipp", "Nele", "Andreas", "Birgit",
];
#[rustfmt::skip]
pub(crate) const LAST_DE: &[&str] = &[
    "Müller", "Schmidt", "Schneider", "Fischer", "Weber", "Meyer", "Wagner", "Becker",
    "Schulz", "Hoffmann", "Schäfer", "Koch", "Bauer", "Richter", "Klein", "Wolf",
    "Schröder", "Neumann", "Schwarz", "Zimmermann", "Braun", "Krüger", "Hofmann",
    "Hartmann", "Lange", "Werner", "Krause", "Lehmann", "Köhler", "Herrmann",
];
#[rustfmt::skip]
pub(crate) const CITY_EN: &[(&str, &str)] = &[
    ("Springfield", "62701"), ("Riverside", "92501"), ("Fairview", "97024"),
    ("Georgetown", "78626"), ("Madison", "53703"), ("Salem", "97301"),
    ("Bristol", "06010"), ("Clinton", "52732"), ("Ashland", "41101"),
    ("Dover", "19901"),
];
#[rustfmt::skip]
pub(crate) const CITY_DE: &[(&str, &str)] = &[
    ("Berlin", "10115"), ("Hamburg", "20095"), ("München", "80331"),
    ("Köln", "50667"), ("Frankfurt", "60311"), ("Stuttgart", "70173"),
    ("Leipzig", "04109"), ("Dresden", "01067"), ("Bonn", "53111"),
    ("Bielefeld", "33602"),
];
#[rustfmt::skip]
pub(crate) const STREET_EN: &[&str] = &[
    "Maple Street", "Oak Avenue", "Cedar Lane", "Sunset Boulevard", "Park Road",
    "Elm Street", "Hillcrest Drive", "Lakeview Terrace", "Church Street", "Mill Road",
];
#[rustfmt::skip]
pub(crate) const STREET_DE: &[&str] = &[
    "Hauptstraße", "Bahnhofstraße", "Gartenweg", "Lindenallee", "Schulstraße",
    "Bergstraße", "Amselweg", "Ringstraße", "Kirchplatz", "Mozartstraße",
];
#[rustfmt::skip]
pub(crate) const COMPANY_A: &[&str] = &[
    "Nord", "Blue", "Iron", "Vertex", "Lumen", "Cobalt", "Atlas", "Nimbus", "Quantum",
    "Orbit", "Cinder", "Helio",
];
#[rustfmt::skip]
pub(crate) const COMPANY_B: &[&str] = &[
    "works", "labs", "systems", "logic", "forge", "soft", "data", "cloud", "tech",
    "dynamics",
];
#[rustfmt::skip]
pub(crate) const JOBS: &[&str] = &[
    "Backend Developer", "Product Manager", "QA Engineer", "Data Analyst",
    "UX Designer", "Site Reliability Engineer", "Technical Writer", "Sales Engineer",
    "Support Specialist", "Security Analyst", "Scrum Master", "Database Administrator",
];
pub(crate) const DOMAINS: &[&str] = &["example.com", "example.org", "mail.example", "test.invalid"];

/// One person, in the field order the output keeps.
///
/// ponytail: a `Vec<(key, value)>` rather than a struct with four
/// serialisations hanging off it — the four output formats are four loops over
/// the same pairs.
pub(crate) fn person(locale: &str) -> Vec<(&'static str, String)> {
    let de = locale == "de";
    let first = *pick(if de { FIRST_DE } else { FIRST_EN });
    let last = *pick(if de { LAST_DE } else { LAST_EN });
    let (city, zip) = *pick(if de { CITY_DE } else { CITY_EN });
    let street = *pick(if de { STREET_DE } else { STREET_EN });
    // slugify strips the umlauts an address keeps but an address never gets.
    let (fs, ls) = (slug::slugify(first), slug::slugify(last));
    let company = format!("{}{}", pick(COMPANY_A), pick(COMPANY_B));
    let year = rand::random_range(1955..=2006);
    let birth = chrono::NaiveDate::from_ymd_opt(
        year,
        rand::random_range(1..=12),
        rand::random_range(1..=28),
    )
    .expect("day 1-28 of month 1-12 exists in every year");
    vec![
        ("first_name", first.to_string()),
        ("last_name", last.to_string()),
        ("email", format!("{fs}.{ls}@{}", pick(DOMAINS))),
        (
            "username",
            format!("{}{ls}{}", &fs[..1], rand::random_range(10..=99)),
        ),
        (
            "phone",
            if de {
                format!(
                    "+49 {} {:07}",
                    pick(&[30, 40, 69, 89, 221, 351]),
                    rand::random_range(100_000..=9_999_999u32)
                )
            } else {
                format!(
                    "+1 (555) {:03}-{:04}",
                    rand::random_range(0..=999),
                    rand::random_range(0..=9999)
                )
            },
        ),
        (
            "street",
            if de {
                format!("{street} {}", rand::random_range(1..=180))
            } else {
                format!("{} {street}", rand::random_range(1..=9999))
            },
        ),
        ("zip", zip.to_string()),
        ("city", city.to_string()),
        (
            "country",
            if de { "Germany" } else { "United States" }.to_string(),
        ),
        (
            "company",
            if de {
                format!("{company} GmbH")
            } else {
                format!("{company} Inc.")
            },
        ),
        ("job_title", pick(JOBS).to_string()),
        ("birth_date", birth.to_string()),
        ("id", uuid::Uuid::new_v4().to_string()),
    ]
}

pub(crate) fn sql_quote(v: &str) -> String {
    format!("'{}'", v.replace('\'', "''"))
}

pub(crate) fn fake_data(a: &Args) -> Result<String> {
    let count = n(a, "count", 5);
    if !(1..=500).contains(&count) {
        bail!("quantity must be between 1 and 500");
    }
    let locale = match s(a, "locale", "en") {
        l @ ("en" | "de") => l,
        other => bail!("unknown locale: {other}"),
    };
    let people: Vec<_> = (0..count).map(|_| person(locale)).collect();
    let keys: Vec<&str> = people[0].iter().map(|(k, _)| *k).collect();

    Ok(match s(a, "format", "JSON") {
        "JSON" => {
            let rows: Vec<Value> = people
                .iter()
                .map(|p| {
                    Value::Object(
                        p.iter()
                            .map(|(k, v)| ((*k).to_string(), Value::String(v.clone())))
                            .collect(),
                    )
                })
                .collect();
            serde_json::to_string_pretty(&rows)?
        }
        "CSV" => {
            // The csv crate, not a join: a street or company can contain a comma.
            let mut w = csv::Writer::from_writer(Vec::new());
            w.write_record(&keys)?;
            for p in &people {
                w.write_record(p.iter().map(|(_, v)| v.as_str()))?;
            }
            String::from_utf8(w.into_inner()?)?
        }
        "SQL" => {
            let mut out = String::new();
            for p in &people {
                let values: Vec<String> = p.iter().map(|(_, v)| sql_quote(v)).collect();
                let _ = writeln!(
                    out,
                    "INSERT INTO people ({}) VALUES ({});",
                    keys.join(", "),
                    values.join(", ")
                );
            }
            out
        }
        "Text" => {
            let mut out = String::new();
            for (i, p) in people.iter().enumerate() {
                let _ = writeln!(out, "#{}", i + 1);
                for (k, v) in p {
                    let _ = writeln!(out, "  {k}: {v}");
                }
                out.push('\n');
            }
            out
        }
        other => bail!("unknown format: {other}"),
    })
}

pub static TOOL: Tool = Tool {
    id: "fake-data-generator",
    name: "Fake data generator",
    description: "Realistic people — names, emails, phones, addresses — as JSON, CSV, SQL or text.",
    category: Category::Data,
    keywords: &[
        "fake", "faker", "mock", "test", "data", "seed", "name", "email", "person", "address",
        "dummy", "sample", "fixture",
    ],
    fields: &[
        Field::number("count", "How many", "5"),
        Field::select("locale", "Locale", &["en", "de"], "en"),
        Field::select("format", "Output", &["JSON", "CSV", "SQL", "Text"], "JSON"),
    ],
    kind: Kind::Pure,
    run: fake_data,
};
