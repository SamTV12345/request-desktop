//! `chmod-calculator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::Result;

/// The permission axes, in the order [`chmod_forms`] indexes them. `pub` because
/// the app's chmod panel draws a grid on exactly these axes, and a reorder here
/// that the panel did not follow would transpose every answer it gives.
pub const CHMOD_GROUPS: [&str; 3] = ["owner", "group", "public"];
pub const CHMOD_SCOPES: [(&str, u8); 3] = [("read", 4), ("write", 2), ("execute", 1)];

/// Octal and symbolic form of a permission matrix, indexed
/// `[owner, group, public][read, write, execute]` — the order of
/// [`CHMOD_GROUPS`] and [`CHMOD_SCOPES`].
///
/// Public so the app's chmod panel, which owns nine checkboxes rather than nine
/// text fields, computes the answer with this and not a second copy of the bit
/// weights.
pub fn chmod_forms(bits: [[bool; 3]; 3]) -> (String, String) {
    let digits = bits.map(|group| {
        group
            .iter()
            .zip(CHMOD_SCOPES)
            .filter(|(on, _)| **on)
            .map(|(_, (_, bit))| bit)
            .sum::<u8>()
    });
    let symbolic: String = digits
        .iter()
        .flat_map(|d| {
            [
                if d & 4 != 0 { 'r' } else { '-' },
                if d & 2 != 0 { 'w' } else { '-' },
                if d & 1 != 0 { 'x' } else { '-' },
            ]
        })
        .collect();
    (digits.iter().map(|d| d.to_string()).collect(), symbolic)
}

/// setuid, setgid, sticky — the fourth octal digit, in the order it is written.
const SPECIAL: [(char, u8, &str); 3] = [
    (
        's',
        4,
        "setuid: the file runs with its owner's identity, not the caller's",
    ),
    (
        's',
        2,
        "setgid: the file runs with its group, and new files in a directory \
         inherit that group",
    ),
    (
        't',
        1,
        "sticky: in a directory, only the owner of a file may delete or rename it",
    ),
];

/// Read `755`, `4755`, `rwxr-xr-x`, `rwsr-xr-t` or `u=rwx,go=rx` into the bit
/// grid (it-tools PR #964).
///
/// Starts from `bits`/`special` so the `+`/`-` clauses compose with whatever the
/// checkbox grid already says; `=` and the two absolute forms replace it.
fn parse_permissions(
    text: &str,
    bits: &mut [[bool; 3]; 3],
    special: &mut [bool; 3],
) -> Result<(), String> {
    let text = text.trim();
    // Octal: three digits, or four with the special bits in front.
    if !text.is_empty()
        && text.len() <= 4
        && text.chars().all(|c| ('0'..='7').contains(&c))
        && let Ok(v) = u16::from_str_radix(text, 8)
    {
        for (i, s) in special.iter_mut().enumerate() {
            *s = text.len() == 4 && (v >> (11 - i)) & 1 != 0;
        }
        for (g, group) in bits.iter_mut().enumerate() {
            let digit = (v >> (6 - g * 3)) as u8 & 7;
            for (b, on) in group.iter_mut().enumerate() {
                *on = digit & CHMOD_SCOPES[b].1 != 0;
            }
        }
        return Ok(());
    }
    // Symbolic, nine characters: `rwxr-xr-x`, with s/S/t/T carrying a special bit.
    let chars: Vec<char> = text.chars().collect();
    if chars.len() == 9 {
        for (g, group) in bits.iter_mut().enumerate() {
            for (b, on) in group.iter_mut().enumerate() {
                let c = chars[g * 3 + b];
                let want = ['r', 'w', 'x'][b];
                // The execute slot doubles as the special flag: lowercase s/t
                // means the execute bit is set too, uppercase means it is not.
                let (exec, flag) = match (b, c) {
                    (2, 's' | 't') => (true, true),
                    (2, 'S' | 'T') => (false, true),
                    _ => (c == want, false),
                };
                if b == 2 {
                    special[g] = flag;
                    *on = exec;
                } else if c == want {
                    *on = true;
                } else if c == '-' {
                    *on = false;
                } else {
                    return Err(format!("{c:?} is not {want:?} or '-'"));
                }
            }
        }
        return Ok(());
    }
    // Clause form: `u=rwx,go=rx`, `a+x`, `o-w`.
    for clause in text.split(',').map(str::trim).filter(|c| !c.is_empty()) {
        let op = clause
            .find(['=', '+', '-'])
            .ok_or_else(|| format!("{clause:?} has no =, + or -"))?;
        let (who, rest) = clause.split_at(op);
        let (op, perms) = rest.split_at(1);
        let who: Vec<usize> = if who.is_empty() || who.contains('a') {
            vec![0, 1, 2]
        } else {
            who.chars()
                .map(|c| match c {
                    'u' => Ok(0),
                    'g' => Ok(1),
                    'o' => Ok(2),
                    other => Err(format!("{other:?} is not one of u, g, o, a")),
                })
                .collect::<Result<_, _>>()?
        };
        for g in who {
            if op == "=" {
                bits[g] = [false; 3];
                special[g] = false;
            }
            for c in perms.chars() {
                let set = op != "-";
                match c {
                    'r' => bits[g][0] = set,
                    'w' => bits[g][1] = set,
                    'x' => bits[g][2] = set,
                    's' if g < 2 => special[g] = set,
                    't' if g == 2 => special[2] = set,
                    'X' => bits[g][2] = set, // only-if-directory; treated as x here
                    other => return Err(format!("{other:?} is not one of r, w, x, s, t")),
                }
            }
        }
    }
    Ok(())
}

/// it-tools is a 3x3 permission checkbox grid with the octal and the symbolic
/// form both always on screen — there is nothing to parse and nothing to fail.
/// The text field is it-tools PR #964: the same state, typed instead of clicked.
pub(crate) fn chmod_calculator(args: &Args) -> Result<String> {
    let t = tool("chmod-calculator");
    let mut bits = CHMOD_GROUPS
        .map(|g| CHMOD_SCOPES.map(|(scope, _)| arg(args, t, &format!("{g}_{scope}")) == "true"));
    let mut special = [false; 3];
    let typed = arg(args, t, "permissions");
    let error = if typed.trim().is_empty() {
        None
    } else {
        parse_permissions(typed, &mut bits, &mut special).err()
    };
    let (octal, mut symbolic) = chmod_forms(bits);
    // The special bits ride on the three execute slots, so `chmod_forms` — whose
    // signature the app's chmod pane depends on — stays a plain 3x3 function and
    // the overlay happens here.
    let mut sym: Vec<char> = symbolic.chars().collect();
    for (g, on) in special.iter().enumerate() {
        if *on {
            let upper = sym[g * 3 + 2] != 'x';
            let c = SPECIAL[g].0;
            sym[g * 3 + 2] = if upper { c.to_ascii_uppercase() } else { c };
        }
    }
    symbolic = sym.into_iter().collect();
    let high: u8 = special
        .iter()
        .zip(SPECIAL)
        .filter(|(on, _)| **on)
        .map(|(_, (_, bit, _))| bit)
        .sum();
    let octal = if high == 0 {
        octal
    } else {
        format!("{high}{octal}")
    };

    let mut out = String::new();
    if let Some(e) = error {
        // A half-typed value must not blank the grid the user can still see.
        out.push_str(&format!("Note: cannot read that permission text ({e})\n"));
    }
    out.push_str(&format!(
        "Octal: {octal}\nSymbolic: {symbolic}\nCommand: chmod {octal} path\n"
    ));
    for (on, (_, _, meaning)) in special.iter().zip(SPECIAL) {
        if *on {
            out.push_str(&format!("Special bit: {meaning}\n"));
        }
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "chmod-calculator",
    name: "Chmod calculator",
    description: "Convert between octal and symbolic Unix file permissions.",
    category: Category::Development,
    keywords: &[
        "chmod",
        "calculator",
        "file",
        "permission",
        "octal",
        "symbolic",
        "unix",
    ],
    // The 3x3 grid, read across then down like it-tools' table.
    fields: &[
        Field::bool("owner_read", "Owner (u) read (4)", "false"),
        Field::bool("group_read", "Group (g) read (4)", "false"),
        Field::bool("public_read", "Public (o) read (4)", "false"),
        Field::bool("owner_write", "Owner (u) write (2)", "false"),
        Field::bool("group_write", "Group (g) write (2)", "false"),
        Field::bool("public_write", "Public (o) write (2)", "false"),
        Field::bool("owner_execute", "Owner (u) execute (1)", "false"),
        Field::bool("group_execute", "Group (g) execute (1)", "false"),
        Field::bool("public_execute", "Public (o) execute (1)", "false"),
        // Empty by default: the grid stays the source of truth until something
        // is typed here, which is what keeps the tool's output on open unchanged.
        Field::text(
            "permissions",
            "…or type it: 755, 4755, rwxr-xr-x, u=rwx,go=rx",
        ),
    ],
    kind: Kind::Pure,
    run: chmod_calculator,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    fn typed(text: &str) -> String {
        ok("chmod-calculator", &[("permissions", text)])
    }

    #[test]
    fn octal_symbolic_and_clauses_all_land_on_the_same_bits() {
        for text in ["755", "rwxr-xr-x", "u=rwx,go=rx", "a=rx,u+w"] {
            let out = typed(text);
            assert!(out.contains("Octal: 755"), "{text}: {out}");
            assert!(out.contains("Symbolic: rwxr-xr-x"), "{text}: {out}");
            assert!(out.contains("Command: chmod 755 path"), "{text}: {out}");
        }
        assert!(typed("640").contains("Symbolic: rw-r-----"));
        assert!(typed("o-rwx,g=r,u=rw").contains("Octal: 640"));
    }

    #[test]
    fn special_bits_are_read_and_explained() {
        let out = typed("4755");
        assert!(out.contains("Octal: 4755"), "{out}");
        assert!(out.contains("Symbolic: rwsr-xr-x"), "{out}");
        assert!(out.contains("Special bit: setuid"), "{out}");
        // Uppercase S: the special bit without the execute bit under it.
        let out = typed("4655");
        assert!(out.contains("Symbolic: rwSr-xr-x"), "{out}");
        // All three special bits at once; `s`/`t` also carry the execute bit.
        assert!(typed("rwsr-sr-t").contains("Octal: 7755"));
        let out = typed("1777");
        assert!(out.contains("Symbolic: rwxrwxrwt"), "{out}");
        assert!(out.contains("only the owner of a file may delete"), "{out}");
        assert!(typed("2755").contains("Special bit: setgid"), "2755");
        assert!(typed("u+s,o+t").contains("Octal: 5000"));
    }

    #[test]
    fn unreadable_text_keeps_the_grid_visible() {
        let out = typed("zzz");
        assert!(out.contains("Note: cannot read"), "{out}");
        assert!(out.contains("Octal: 000"), "{out}");
        // The checkboxes still win when nothing is typed.
        let out = ok(
            "chmod-calculator",
            &[("owner_read", "true"), ("permissions", "")],
        );
        assert!(out.contains("Octal: 400"), "{out}");
    }
}
