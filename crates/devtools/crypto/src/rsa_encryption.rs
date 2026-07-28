//! `rsa-encryption` — it-tools PR #1013.
//!
//! The companion to the ported `rsa-key-pair-generator`: that one makes the
//! PEMs, this one uses them.
//!
//! Padding is PKCS#1 v1.5. OAEP would be the better scheme, but it is
//! parameterised by a digest and `rsa` 0.9 wants a `digest` 0.10 hash for that,
//! while this crate is on `sha2` 0.11 (`digest` 0.11) — wiring the two together
//! means a new dependency. PKCS#1 v1.5 needs no digest at all, and it is what
//! the openssl CLI (`rsautl -encrypt`) and the JS libraries behind the upstream
//! PR default to, so pasted ciphertext interoperates.

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, section};
use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use rsa::pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::traits::PublicKeyParts;
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey};

/// PKCS#1 v1.5 spends 11 bytes on padding, so this is all that fits.
fn max_plaintext(key_bytes: usize) -> usize {
    key_bytes.saturating_sub(11)
}

/// Accept either PEM the ecosystem hands out: SPKI (`BEGIN PUBLIC KEY`, what
/// the generator here writes) or PKCS#1 (`BEGIN RSA PUBLIC KEY`, openssl's
/// `-RSAPublicKey_out`).
fn public_key(pem: &str) -> Result<RsaPublicKey> {
    let pem = pem.trim();
    if pem.is_empty() {
        bail!("paste the recipient's public key, in PEM form");
    }
    RsaPublicKey::from_public_key_pem(pem)
        .or_else(|_| RsaPublicKey::from_pkcs1_pem(pem))
        .context("not a readable RSA public key: expected a PEM block")
}

/// Likewise PKCS#8 (`BEGIN PRIVATE KEY`) or PKCS#1 (`BEGIN RSA PRIVATE KEY`).
fn private_key(pem: &str) -> Result<RsaPrivateKey> {
    let pem = pem.trim();
    if pem.is_empty() {
        bail!("paste your private key, in PEM form");
    }
    RsaPrivateKey::from_pkcs8_pem(pem)
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(pem))
        .context("not a readable RSA private key: expected a PEM block")
}

fn encrypt(a: &Args) -> Result<String> {
    let key = public_key(s(a, "public_key", ""))?;
    let text = s(a, "plaintext", "");
    let max = max_plaintext(key.size());
    if text.len() > max {
        bail!(
            "the message is {} bytes; a {}-bit key can only carry {max} bytes with PKCS#1 v1.5 \
             padding. Encrypt a symmetric key with RSA and the message with that key instead.",
            text.len(),
            key.size() * 8
        );
    }
    let out = key
        .encrypt(&mut RsaRng, Pkcs1v15Encrypt, text.as_bytes())
        .context("RSA encryption failed")?;
    Ok(STANDARD.encode(out))
}

fn decrypt(a: &Args) -> Result<String> {
    let key = private_key(s(a, "private_key", ""))?;
    // Ciphertext arrives wrapped by whatever pasted it, so ignore all layout.
    let b64: String = s(a, "ciphertext", "")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if b64.is_empty() {
        bail!("paste the base64 ciphertext to decrypt");
    }
    let raw = STANDARD
        .decode(&b64)
        .context("the ciphertext is not valid base64")?;
    let out = key
        .decrypt(Pkcs1v15Encrypt, &raw)
        .context("decryption failed: wrong key, or the ciphertext is not for this key")?;
    String::from_utf8(out).context("the decrypted bytes are not UTF-8 text")
}

pub(crate) fn run_rsa_encryption(a: &Args) -> Result<String> {
    if s(a, "public_key", "").trim().is_empty() && s(a, "private_key", "").trim().is_empty() {
        bail!(
            "paste a public key to encrypt with, or a private key to decrypt with; \
             the RSA key pair generator produces both in the PEM form expected here"
        );
    }
    Ok([
        section("Encrypted (base64)", encrypt(a)),
        section("Decrypted", decrypt(a)),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "rsa-encryption",
    name: "RSA encryption",
    description: "Encrypt a message with an RSA public key, or decrypt one with the private key.",
    category: Category::Crypto,
    keywords: &[
        "rsa", "encrypt", "decrypt", "public", "private", "pem", "pkcs1",
    ],
    fields: &[
        Field::area("public_key", "Public key (PEM)"),
        Field::area("private_key", "Private key (PEM)"),
        Field::area("plaintext", "Message to encrypt"),
        Field::area("ciphertext", "Ciphertext to decrypt (base64)"),
    ],
    kind: Kind::Pure,
    run: run_rsa_encryption,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;
    use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};

    /// 512 bits: large enough to carry a short message, small enough that the
    /// test suite does not crawl. Generated per run — there is no published RSA
    /// encryption test vector that does not also pin the padding randomness.
    fn keys() -> (String, String) {
        let private = RsaPrivateKey::new(&mut RsaRng, 512).unwrap();
        (
            private
                .to_public_key()
                .to_public_key_pem(LineEnding::LF)
                .unwrap(),
            private.to_pkcs8_pem(LineEnding::LF).unwrap().to_string(),
        )
    }

    fn panel<'a>(out: &'a str, label: &str) -> &'a str {
        crate::split_sections(out)
            .into_iter()
            .find(|(l, _)| *l == label)
            .unwrap_or_else(|| panic!("no {label} panel in {out}"))
            .1
            .trim()
    }

    #[test]
    fn round_trip_through_both_panels() {
        let (public, private) = keys();
        let out = run_rsa_encryption(&args(&[
            ("public_key", &public),
            ("plaintext", "attack at dawn"),
        ]))
        .unwrap();
        let ciphertext = panel(&out, "Encrypted (base64)");
        let out = run_rsa_encryption(&args(&[
            ("private_key", &private),
            ("ciphertext", ciphertext),
        ]))
        .unwrap();
        assert_eq!(panel(&out, "Decrypted"), "attack at dawn");
        // Line-wrapped ciphertext, as a mail client would deliver it.
        let wrapped = ciphertext
            .as_bytes()
            .chunks(20)
            .map(|c| String::from_utf8_lossy(c).into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        let out = run_rsa_encryption(&args(&[
            ("private_key", &private),
            ("ciphertext", &wrapped),
        ]))
        .unwrap();
        assert_eq!(panel(&out, "Decrypted"), "attack at dawn");
    }

    #[test]
    fn a_message_too_long_for_the_key_says_so() {
        let (public, _) = keys();
        // 64 byte key - 11 bytes of padding = 53 usable.
        assert_eq!(max_plaintext(64), 53);
        let out = run_rsa_encryption(&args(&[
            ("public_key", &public),
            ("plaintext", &"x".repeat(54)),
        ]))
        .unwrap();
        assert!(out.contains("can only carry 53 bytes"), "{out}");
        // And a megabyte is refused just as calmly.
        let out = run_rsa_encryption(&args(&[
            ("public_key", &public),
            ("plaintext", &"x".repeat(1 << 20)),
        ]))
        .unwrap();
        assert!(
            out.contains("Encrypted (base64):\n  (the message is 1048576"),
            "{out}"
        );
    }

    #[test]
    fn bad_keys_and_ciphertext_stay_inside_their_panel() {
        let (public, private) = keys();
        assert!(run_rsa_encryption(&args(&[])).is_err());
        assert!(run_rsa_encryption(&args(&[("plaintext", "hi")])).is_err());
        let out = run_rsa_encryption(&args(&[
            (
                "public_key",
                "-----BEGIN PUBLIC KEY-----\nnope\n-----END PUBLIC KEY-----",
            ),
            ("private_key", &private),
            ("ciphertext", "not base64!!"),
        ]))
        .unwrap();
        assert!(
            out.contains("Encrypted (base64):\n  (not a readable RSA public key"),
            "{out}"
        );
        assert!(
            out.contains("Decrypted:\n  (the ciphertext is not valid base64"),
            "{out}"
        );
        // Right key, wrong ciphertext: an error, never a panic.
        let out = run_rsa_encryption(&args(&[
            ("public_key", &public),
            ("private_key", &private),
            ("ciphertext", &STANDARD.encode([0u8; 64])),
        ]))
        .unwrap();
        assert!(out.contains("Decrypted:\n  (decryption failed"), "{out}");
        // A megabyte of junk in every field is still just two failed panels.
        let big = "\u{fffd}".repeat(1 << 18);
        let out = run_rsa_encryption(&args(&[
            ("public_key", &big),
            ("private_key", &big),
            ("ciphertext", &big),
        ]))
        .unwrap();
        assert!(out.contains("not a readable RSA"), "{out}");
    }
}
