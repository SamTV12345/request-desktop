//! `random-port-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Result, bail};
use std::fmt::Write as _;

pub(crate) fn random_port_generator(args: &Args) -> Result<String> {
    let count: usize = arg(args, tool("random-port-generator"), "quantity")
        .trim()
        .parse()
        .unwrap_or(1);
    if !(1..=1000).contains(&count) {
        bail!("quantity must be between 1 and 1000");
    }
    Ok((0..count).fold(String::new(), |mut s, _| {
        let _ = writeln!(s, "{}", rand::random_range(1024..=65535u32));
        s
    }))
}

pub static TOOL: Tool = Tool {
    id: "random-port-generator",
    name: "Random port generator",
    description: "Generate random port numbers outside of the range of the known ports (0-1023).",
    category: Category::Network,
    keywords: &[
        "system",
        "port",
        "lan",
        "generator",
        "random",
        "development",
        "computer",
    ],
    fields: &[Field::number("quantity", "Quantity", "1")],
    kind: Kind::Pure,
    run: random_port_generator,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    #[test]
    fn random_ports_are_in_range() {
        let out = run("random-port-generator", &[("quantity", "20")]).unwrap();
        assert_eq!(out.lines().count(), 20);
        for line in out.lines() {
            let p: u32 = line.parse().unwrap();
            assert!((1024..=65535).contains(&p), "{p}");
        }
        assert!(run("random-port-generator", &[("quantity", "9999")]).is_err());
    }
}
