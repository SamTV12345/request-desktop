//! `raid-calculator` — it-tools PR #1218.
//!
//! The question before buying disks is always the same: how much of what I paid
//! for do I actually get, and how many failures does it survive. The level
//! select is a choice of layout, not a direction, so it is a legitimate select
//! under the no-mode-toggle rule.

use crate::*;
use anyhow::{Result, bail};

pub(crate) const RAID_LEVELS: &[&str] = &[
    "RAID 0", "RAID 1", "RAID 5", "RAID 6", "RAID 10", "RAID 50", "RAID 60", "JBOD",
];

pub(crate) const DISK_UNITS: &[&str] = &["GB", "TB", "GiB", "TiB"];

fn unit_bytes(unit: &str) -> Result<f64> {
    Ok(match unit.trim() {
        "GB" => 1e9,
        "TB" => 1e12,
        "GiB" => 1_073_741_824.0,
        "TiB" => 1_099_511_627_776.0,
        other => bail!("unknown disk size unit `{other}`; pick one of GB, TB, GiB or TiB"),
    })
}

pub(crate) fn raid_min_disks(level: &str) -> Result<u32> {
    Ok(match level {
        "RAID 0" | "RAID 1" => 2,
        "RAID 5" => 3,
        "RAID 6" | "RAID 10" => 4,
        "RAID 50" => 6,
        "RAID 60" => 8,
        "JBOD" => 1,
        other => bail!("unknown RAID level `{other}`"),
    })
}

/// Capacity in whole disks, failures survived, and the one line about speed that
/// people actually choose on. RAID 50/60 are modelled as two groups, which is
/// what a controller builds by default and the only assumption that makes the
/// numbers well defined without a group-count field.
fn layout(level: &str, disks: u32) -> Result<(f64, String, &'static str)> {
    let min = raid_min_disks(level)?;
    if disks < min {
        bail!(
            "{level} needs at least {min} disks, and you entered {disks}; raise the disk count or pick a level with a lower minimum."
        );
    }
    let even = |group: &str| -> Result<()> {
        if !disks.is_multiple_of(2) {
            bail!(
                "{level} is striped over two {group} groups, so it needs an even number of disks; you entered {disks}."
            );
        }
        Ok(())
    };
    Ok(match level {
        "RAID 0" => (
            disks as f64,
            "none — losing one disk loses the array".into(),
            "Fastest reads and writes, and no redundancy at all.",
        ),
        "RAID 1" => (
            1.0,
            format!("{}", disks - 1),
            "Reads scale with the mirrors, writes stay at single-disk speed.",
        ),
        "RAID 5" => (
            (disks - 1) as f64,
            "1".into(),
            "Fast reads; every write pays a read-modify-write for one parity block.",
        ),
        "RAID 6" => (
            (disks - 2) as f64,
            "2".into(),
            "Fast reads; writes pay two parity blocks, so they are slower than RAID 5.",
        ),
        "RAID 10" => {
            even("mirror")?;
            (
                disks as f64 / 2.0,
                format!(
                    "1 guaranteed, up to {} if no two failures share a mirror",
                    disks / 2
                ),
                "Fast reads and fast writes with no parity penalty — the usual database choice.",
            )
        }
        "RAID 50" => {
            even("RAID 5")?;
            (
                (disks - 2) as f64,
                "1 guaranteed, 2 if the failures fall in different groups".into(),
                "RAID 5 write penalty, halved by striping over two groups.",
            )
        }
        "RAID 60" => {
            even("RAID 6")?;
            (
                (disks - 4) as f64,
                "2 guaranteed, 4 if the failures are spread over both groups".into(),
                "RAID 6 write penalty, halved by striping over two groups.",
            )
        }
        "JBOD" => (
            disks as f64,
            "none — each disk stands alone, so a failure loses only its own data".into(),
            "Single-disk speed; the capacities are simply added up.",
        ),
        other => bail!("unknown RAID level `{other}`"),
    })
}

/// Both spellings, because the invoice is decimal and the mount point is binary.
fn capacity(bytes: f64) -> String {
    let r = |v: f64| fmt_f((v * 1e3).round() / 1e3);
    if bytes >= 1e12 {
        format!(
            "{} TB / {} TiB",
            r(bytes / 1e12),
            r(bytes / 1_099_511_627_776.0)
        )
    } else {
        format!("{} GB / {} GiB", r(bytes / 1e9), r(bytes / 1_073_741_824.0))
    }
}

fn raid(a: &Args) -> Result<String> {
    let level = s(a, "level", "RAID 5").trim();
    let disks = n(a, "disks", 4);
    if !(1..=1024).contains(&disks) {
        bail!("the number of disks must be between 1 and 1024; you entered {disks}.");
    }
    let size = f(a, "size", 4.0);
    if !size.is_finite() || size <= 0.0 {
        bail!("the disk size must be a positive number.");
    }
    let unit = s(a, "unit", "TB");
    let per_disk = size * unit_bytes(unit)?;
    let disks = disks as u32;
    let (usable_disks, tolerated, note) = layout(level, disks)?;

    let raw = disks as f64 * per_disk;
    let usable = usable_disks * per_disk;
    let overhead = (raw - usable) / raw * 100.0;
    Ok([
        row("Usable capacity", Ok(capacity(usable))),
        row("Raw capacity", Ok(capacity(raw))),
        row(
            "Redundancy overhead",
            Ok(format!("{} %", fmt_f((overhead * 1e2).round() / 1e2))),
        ),
        row("Disks that may fail", Ok(tolerated)),
        row(
            "Minimum disks for this level",
            Ok(format!("{} (you have {disks})", raid_min_disks(level)?)),
        ),
        row("Read/write", Ok(note.to_string())),
    ]
    .concat())
}

pub static TOOL: Tool = Tool {
    id: "raid-calculator",
    name: "RAID calculator",
    description: "Usable capacity, overhead and failure tolerance for a RAID array.",
    category: Category::Measurement,
    keywords: &[
        "raid",
        "storage",
        "disk",
        "array",
        "capacity",
        "redundancy",
        "parity",
        "jbod",
    ],
    fields: &[
        Field::select("level", "RAID level", RAID_LEVELS, "RAID 5"),
        Field::number("disks", "Number of disks", "4"),
        Field::number("size", "Size of one disk", "4"),
        Field::select("unit", "Disk size unit", DISK_UNITS, "TB"),
    ],
    kind: Kind::Pure,
    run: raid,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn four_by_four_terabytes_of_raid5() {
        let out = ok("raid-calculator", &[]);
        // 3 of 4 disks usable, 12 TB of 16 TB, 25 % lost to parity.
        assert!(out.contains("Usable capacity: 12 TB / 10.914 TiB"), "{out}");
        assert!(out.contains("Raw capacity: 16 TB / 14.552 TiB"), "{out}");
        assert!(out.contains("Redundancy overhead: 25 %"), "{out}");
        assert!(out.contains("Disks that may fail: 1"), "{out}");
    }

    #[test]
    fn levels_and_their_arithmetic() {
        let usable = |level: &str, disks: &str| {
            ok(
                "raid-calculator",
                &[("level", level), ("disks", disks), ("size", "1")],
            )
        };
        assert!(usable("RAID 0", "4").contains("Usable capacity: 4 TB"));
        assert!(usable("RAID 1", "4").contains("Usable capacity: 1 TB"));
        assert!(usable("RAID 6", "6").contains("Usable capacity: 4 TB"));
        assert!(usable("RAID 10", "6").contains("Usable capacity: 3 TB"));
        assert!(usable("RAID 50", "6").contains("Usable capacity: 4 TB"));
        assert!(usable("RAID 60", "8").contains("Usable capacity: 4 TB"));
        assert!(usable("JBOD", "3").contains("Usable capacity: 3 TB"));
        // Mirrors survive everything but the last disk.
        assert!(usable("RAID 1", "4").contains("Disks that may fail: 3"));
        // Units are a unit choice, not a direction toggle.
        assert!(
            ok(
                "raid-calculator",
                &[
                    ("level", "JBOD"),
                    ("disks", "1"),
                    ("size", "1"),
                    ("unit", "TiB")
                ]
            )
            .contains("Usable capacity: 1.1 TB / 1 TiB")
        );
    }

    #[test]
    fn too_few_disks_says_what_to_do() {
        let e = run("raid-calculator", &[("level", "RAID 6"), ("disks", "3")])
            .unwrap_err()
            .to_string();
        assert!(e.contains("at least 4 disks"), "{e}");
        assert!(e.contains("raise the disk count"), "{e}");
        assert!(
            run("raid-calculator", &[("level", "RAID 10"), ("disks", "5")])
                .unwrap_err()
                .to_string()
                .contains("even number of disks")
        );
    }

    #[test]
    fn adversarial_inputs_are_answered_not_crashed() {
        for (k, v) in [
            ("disks", i64::MIN.to_string()),
            ("disks", u64::MAX.to_string()),
            ("disks", "9".repeat(400)),
            ("size", format!("{}", f64::MAX)),
            ("size", "-1".to_string()),
            ("size", "\u{1b}[0m".to_string()),
            ("unit", "PB".to_string()),
            ("level", "RAID 7".to_string()),
        ] {
            // Either an answer or a message; a panic or an overflow is not.
            let _ = run("raid-calculator", &[(k, v.as_str())]);
        }
        // A huge but valid array still reports finite numbers.
        let out = ok(
            "raid-calculator",
            &[("disks", "1024"), ("size", "1000"), ("unit", "TiB")],
        );
        assert!(!out.contains("inf") && !out.contains("NaN"), "{out}");
    }
}
