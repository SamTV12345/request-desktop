//! `rsa-key-pair-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Context, Result, bail};

pub(crate) const RSA_KEY_SIZES: &[&str] = &["512", "1024", "2048", "4096"];

/// it-tools PR #888. PKCS#8 (`BEGIN PRIVATE KEY` / `BEGIN PUBLIC KEY`) is what
/// the tool has always emitted and stays the default; PKCS#1
/// (`BEGIN RSA PRIVATE KEY`) is the older, RSA-only encoding that ssh-keygen,
/// Apache and a lot of embedded tooling still expect.
pub(crate) const RSA_FORMATS: &[&str] = &["PKCS#8", "PKCS#1"];

/// The public key in OpenSSH `authorized_keys` form.
///
/// The wire format is just length-prefixed fields — `string "ssh-rsa"`, then
/// `mpint e`, `mpint n` (RFC 4253 §6.6) — so it is 15 lines here rather than an
/// ssh-key crate. An mpint is big-endian, minimal length, and gets a leading
/// zero byte when the top bit is set, or it would read as negative.
pub(crate) fn openssh_public_key(key: &rsa::RsaPublicKey) -> String {
    use base64::Engine;
    use rsa::traits::PublicKeyParts;

    fn field(out: &mut Vec<u8>, bytes: &[u8]) {
        out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(bytes);
    }
    fn mpint(out: &mut Vec<u8>, value: &rsa::BigUint) {
        let mut be = value.to_bytes_be();
        if be.first().is_some_and(|b| b & 0x80 != 0) {
            be.insert(0, 0);
        }
        field(out, &be);
    }

    let mut blob = Vec::new();
    field(&mut blob, b"ssh-rsa");
    mpint(&mut blob, key.e());
    mpint(&mut blob, key.n());
    format!(
        "ssh-rsa {}",
        base64::engine::general_purpose::STANDARD.encode(&blob)
    )
}

/// `rand` 0.10 bridged onto the `rand_core` 0.6 that `rsa` 0.9 expects.
// ponytail: six trait methods instead of turning on rsa's `getrandom` feature,
// because Cargo.toml is off-limits. `rand::random` is a CSPRNG, so claiming
// `CryptoRng` is honest.
pub(crate) struct RsaRng;
impl rsa::rand_core::RngCore for RsaRng {
    fn next_u32(&mut self) -> u32 {
        rand::random()
    }
    fn next_u64(&mut self) -> u64 {
        rand::random()
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        dest.fill_with(rand::random);
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rsa::rand_core::Error> {
        dest.fill_with(rand::random);
        Ok(())
    }
}
impl rsa::rand_core::CryptoRng for RsaRng {}

pub(crate) fn run_rsa(a: &Args) -> Result<String> {
    // ponytail: generation is CPU-bound and blocking — 4096 bits takes seconds
    // to a minute — while the UI calls `run` on every keystroke. So nothing is
    // computed until `generate` is explicitly switched on. Upgrade path: move
    // the tool off Kind::Pure onto a background task.
    if !b(a, "generate", false) {
        return Ok("set generate to true to produce a key pair".into());
    }
    let bits = s(a, "bits", "2048").trim();
    if !RSA_KEY_SIZES.contains(&bits) {
        bail!("key size must be one of {}", RSA_KEY_SIZES.join(", "));
    }
    let bits: usize = bits.parse().expect("RSA_KEY_SIZES are numbers");
    let private =
        rsa::RsaPrivateKey::new(&mut RsaRng, bits).context("RSA key generation failed")?;
    let public = rsa::RsaPublicKey::from(&private);
    use rsa::pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey};
    use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
    let format = s(a, "format", "PKCS#8").trim();
    let (public_pem, private_pem) = match format {
        "PKCS#8" => (
            public
                .to_public_key_pem(LineEnding::LF)
                .context("cannot encode the public key"),
            private
                .to_pkcs8_pem(LineEnding::LF)
                .context("cannot encode the private key")
                .map(|pem| pem.to_string()),
        ),
        "PKCS#1" => (
            public
                .to_pkcs1_pem(LineEnding::LF)
                .context("cannot encode the public key"),
            private
                .to_pkcs1_pem(LineEnding::LF)
                .context("cannot encode the private key")
                .map(|pem| pem.to_string()),
        ),
        other => bail!("unknown format: {other}"),
    };
    // it-tools shows the public key panel above the private one. The OpenSSH
    // line is the format you actually paste into authorized_keys, so it sits
    // next to the PEM rather than replacing it.
    // A PEM already ends in a newline, the OpenSSH line does not — and a
    // section header is only recognised after a blank line, so the separators
    // differ. `split_sections_loses_nothing_for_every_tool` is the check.
    Ok(format!(
        "{}\n{}\n\n{}",
        crate::section("Public key", public_pem),
        crate::section("Public key (OpenSSH)", Ok(openssh_public_key(&public))),
        crate::section("Private key", private_pem)
    ))
}

pub static TOOL: Tool = Tool {
    id: "rsa-key-pair-generator",
    name: "RSA key pair generator",
    description: "Generate a new RSA private and public key pair, PKCS#8 or PKCS#1, plus OpenSSH.",
    category: Category::Crypto,
    keywords: &["rsa", "key", "pair", "private", "public", "pem", "ssh"],
    fields: &[
        Field::select("bits", "Key size (bits)", RSA_KEY_SIZES, "2048"),
        Field::select("format", "PEM format", RSA_FORMATS, "PKCS#8"),
        Field::bool("generate", "Generate (slow, off by default)", "false"),
    ],
    kind: Kind::Pure,
    run: run_rsa,
};
