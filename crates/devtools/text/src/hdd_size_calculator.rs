//! `hdd-size-calculator` — it-tools PR #1022.
//!
//! Why a "1 TB" disk shows up as 931 GB. The label is decimal (10^12), the file
//! manager is binary (2^40), and the filesystem takes another slice on top. All
//! three numbers are shown at once so the gap is visible rather than argued
//! about; the filesystem overhead is a documented rule of thumb, not a
//! measurement.

use crate::*;
use anyhow::{Result, bail};

pub(crate) const SIZE_UNITS: &[&str] = &["TB", "GB", "TiB", "GiB", "MB", "MiB"];

pub(crate) const FILESYSTEMS: &[&str] = &["none", "ext4", "NTFS", "APFS", "exFAT", "FAT32"];

fn unit_bytes(unit: &str) -> Result<f64> {
    Ok(match unit.trim() {
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        "MiB" => 1_048_576.0,
        "GiB" => 1_073_741_824.0,
        "TiB" => 1_099_511_627_776.0,
        other => bail!("unknown unit `{other}`"),
    })
}

/// Percent of the formatted volume that never becomes user data, and where the
/// number comes from. These are order-of-magnitude figures for a large, mostly
/// empty volume — the real cost depends on the file size distribution, so they
/// are labelled an estimate everywhere they appear.
fn fs_overhead(fs: &str) -> Result<(f64, &'static str)> {
    Ok(match fs.trim() {
        "none" => (0.0, "raw block device, nothing reserved"),
        "ext4" => (
            6.5,
            "5 % reserved for root plus roughly 1.5 % inodes and journal",
        ),
        "NTFS" => (1.0, "$MFT and metadata; the 12.5 % MFT zone is reclaimable"),
        "APFS" => (0.5, "container metadata and checkpoints"),
        "exFAT" => (0.2, "allocation table only, no journal"),
        "FAT32" => (0.3, "two allocation tables"),
        other => bail!("unknown filesystem `{other}`"),
    })
}

fn r3(v: f64) -> String {
    fmt_f((v * 1e3).round() / 1e3)
}

fn hdd(a: &Args) -> Result<String> {
    let size = f(a, "size", 1.0);
    if !size.is_finite() || size <= 0.0 {
        bail!("enter a positive disk size, for example 1 with the unit TB.");
    }
    let bytes = size * unit_bytes(s(a, "unit", "TB"))?;
    if !bytes.is_finite() {
        bail!("that size is too large to represent in bytes.");
    }
    let (overhead, why) = fs_overhead(s(a, "filesystem", "ext4"))?;

    let tb = bytes / 1e12;
    let tib = bytes / 1_099_511_627_776.0;
    // Same bytes, two yardsticks: the shortfall is a property of the units, not
    // of the disk, so it is constant at 1 - 10^12 / 2^40.
    let shortfall = (1.0 - tib / tb) * 100.0;
    let usable = bytes * (1.0 - overhead / 100.0);

    Ok([
        section(
            "Decimal, what the label says",
            Ok([
                row("Bytes", Ok(format!("{:.0}", bytes.trunc()))),
                row("MB", Ok(r3(bytes / 1e6))),
                row("GB", Ok(r3(bytes / 1e9))),
                row("TB", Ok(r3(tb))),
            ]
            .concat()),
        ),
        section(
            "Binary, what the operating system shows",
            Ok([
                row("MiB", Ok(r3(bytes / 1_048_576.0))),
                row("GiB", Ok(r3(bytes / 1_073_741_824.0))),
                row("TiB", Ok(r3(tib))),
            ]
            .concat()),
        ),
        section(
            "The difference",
            Ok(row(
                "Missing",
                Ok(format!(
                    "{} % — {} TB of label is {} TiB on screen",
                    r3(shortfall),
                    r3(tb),
                    r3(tib)
                )),
            )),
        ),
        section(
            "Usable space (estimate)",
            Ok([
                row("Filesystem", Ok(s(a, "filesystem", "ext4").to_string())),
                row("Overhead", Ok(format!("about {overhead} % — {why}"))),
                row(
                    "Usable",
                    Ok(format!(
                        "{} GB / {} GiB (estimate, not measured)",
                        r3(usable / 1e9),
                        r3(usable / 1_073_741_824.0)
                    )),
                ),
            ]
            .concat()),
        ),
    ]
    .join("\n"))
}

pub static TOOL: Tool = Tool {
    id: "hdd-size-calculator",
    name: "Hard drive size calculator",
    description: "Why a 1 TB disk shows up as 931 GiB: decimal against binary units, plus filesystem overhead.",
    category: Category::Measurement,
    keywords: &[
        "hdd",
        "disk",
        "storage",
        "size",
        "gigabyte",
        "gibibyte",
        "binary",
        "decimal",
        "filesystem",
    ],
    fields: &[
        Field::number("size", "Advertised size", "1"),
        Field::select("unit", "Unit", SIZE_UNITS, "TB"),
        Field::select("filesystem", "Filesystem", FILESYSTEMS, "ext4"),
    ],
    kind: Kind::Pure,
    run: hdd,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn one_terabyte_is_the_famous_931() {
        let out = ok("hdd-size-calculator", &[]);
        assert!(out.contains("Bytes: 1000000000000"), "{out}");
        assert!(out.contains("GiB: 931.323"), "{out}");
        assert!(out.contains("TiB: 0.909"), "{out}");
        // 1 - 10^12 / 2^40 = 9.0505 %.
        assert!(out.contains("Missing: 9.051 %"), "{out}");
        // 1 TB minus 6.5 % ext4 overhead = 935 GB.
        assert!(out.contains("Usable: 935 GB"), "{out}");
        assert!(out.contains("estimate, not measured"), "{out}");
    }

    #[test]
    fn units_and_filesystems() {
        let out = ok(
            "hdd-size-calculator",
            &[("size", "512"), ("unit", "GB"), ("filesystem", "none")],
        );
        assert!(out.contains("GiB: 476.837"), "{out}");
        assert!(out.contains("Usable: 512 GB"), "{out}");
        // A binary input comes out round on the binary side.
        assert!(
            ok(
                "hdd-size-calculator",
                &[("unit", "TiB"), ("filesystem", "none")]
            )
            .contains("TiB: 1")
        );
        assert!(run("hdd-size-calculator", &[("unit", "PB")]).is_err());
        assert!(run("hdd-size-calculator", &[("filesystem", "btrfs")]).is_err());
        assert!(run("hdd-size-calculator", &[("size", "0")]).is_err());
        assert!(run("hdd-size-calculator", &[("size", "-4")]).is_err());
    }

    #[test]
    fn adversarial_inputs_are_answered_not_crashed() {
        for v in [
            i64::MIN.to_string(),
            u64::MAX.to_string(),
            "9".repeat(400),
            f64::MAX.to_string(),
            "\u{0}\u{1b}".to_string(),
            "x".repeat(1_000_000),
        ] {
            let _ = run("hdd-size-calculator", &[("size", v.as_str())]);
        }
        // An exabyte is still finite; `fmt_f` renders a non-finite value as a
        // dash, so the numbers must not be dashes here.
        let out = ok(
            "hdd-size-calculator",
            &[("size", "1000000"), ("unit", "TB")],
        );
        assert!(!out.contains("inf") && !out.contains("NaN"), "{out}");
        assert!(out.contains("TiB: 909494.702"), "{out}");
    }
}
