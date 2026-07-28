//! `bip39-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Context, Result, bail};

// ponytail: English only — the other wordlists are cargo features on `bip39`
// and Cargo.toml is off-limits here.
pub(crate) fn run_bip39(a: &Args) -> Result<String> {
    let mnemonic_in = s(a, "mnemonic", "").trim();
    let mnemonic = if !mnemonic_in.is_empty() {
        bip39::Mnemonic::parse(mnemonic_in).context("invalid mnemonic")?
    } else {
        let entropy = match s(a, "entropy", "").trim() {
            "" => (0..16).map(|_| rand::random::<u8>()).collect(),
            hex => {
                if hex.len() % 2 != 0 {
                    bail!("entropy must be an even-length hexadecimal string");
                }
                hex_to_bytes(hex)?
            }
        };
        if !(16..=32).contains(&entropy.len()) || entropy.len() % 4 != 0 {
            bail!("entropy length should be >= 16, <= 32 and be a multiple of 4 bytes");
        }
        bip39::Mnemonic::from_entropy(&entropy).context("invalid entropy")?
    };
    Ok(format!(
        "entropy: {}\nmnemonic: {mnemonic}\n",
        hex_encode(&mnemonic.to_entropy())
    ))
}

pub static TOOL: Tool = Tool {
    id: "bip39-generator",
    name: "Bip39 passphrase generator",
    description: "Convert between a BIP39 entropy (seed) and its English mnemonic.",
    category: Category::Crypto,
    keywords: &[
        "bip39",
        "mnemonic",
        "passphrase",
        "seed",
        "entropy",
        "wallet",
    ],
    fields: &[
        Field::text("entropy", "Entropy (hex, empty = random)"),
        Field::text("mnemonic", "Mnemonic (overrides entropy)"),
    ],
    kind: Kind::Pure,
    run: run_bip39,
};
