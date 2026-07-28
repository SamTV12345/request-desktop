//! `geo-distance-computer`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Result, bail};

// it-tools PR #1456.

pub(crate) fn geo_distance(a: &Args) -> Result<String> {
    let (lat1, lon1) = (f(a, "lat1", 48.8584), f(a, "lon1", 2.2945));
    let (lat2, lon2) = (f(a, "lat2", 52.5163), f(a, "lon2", 13.3777));
    for (name, v, limit) in [
        ("latitude 1", lat1, 90.0),
        ("latitude 2", lat2, 90.0),
        ("longitude 1", lon1, 180.0),
        ("longitude 2", lon2, 180.0),
    ] {
        if !v.is_finite() || v.abs() > limit {
            bail!("{name} must be between -{limit} and {limit}");
        }
    }
    // Mean earth radius; haversine, so a great circle over a sphere.
    const R_KM: f64 = 6371.0088;
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = ((lat2 - lat1).to_radians(), (lon2 - lon1).to_radians());
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    let km = 2.0 * R_KM * h.sqrt().asin();

    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    let bearing = (y.atan2(x).to_degrees() + 360.0) % 360.0;
    const COMPASS: &[&str] = &["N", "NE", "E", "SE", "S", "SW", "W", "NW"];

    Ok(format!(
        "distance: {:.3} km\n          {:.3} miles\n          {:.3} nautical miles\n\
         initial bearing: {bearing:.1}° ({})\n",
        km,
        km / 1.609_344,
        km / 1.852,
        COMPASS[(((bearing + 22.5) % 360.0) / 45.0) as usize],
    ))
}

pub static TOOL: Tool = Tool {
    id: "geo-distance-computer",
    name: "Geo distance computer",
    description: "Great-circle distance and bearing between two coordinates.",
    category: Category::Measurement,
    keywords: &[
        "geo",
        "distance",
        "haversine",
        "latitude",
        "longitude",
        "bearing",
        "gps",
    ],
    fields: &[
        Field::number("lat1", "Latitude A", "48.8584"),
        Field::number("lon1", "Longitude A", "2.2945"),
        Field::number("lat2", "Latitude B", "52.5163"),
        Field::number("lon2", "Longitude B", "13.3777"),
    ],
    kind: Kind::Pure,
    run: geo_distance,
};
