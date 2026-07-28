//! Crypto & security tools. Owned by the crypto batch.
//!
//! Ported from it-tools `src/tools/*`. Behaviour (and quirks — see
//! `base32_to_hex`) is kept identical so the ported test vectors hold.

pub use rp_devtools_common::*;

mod basic_auth_generator;
mod bcrypt;
mod bip39_generator;
mod common;
mod encryption;
mod hash_text;
mod hmac_generator;
mod hotp_generator;
mod htpasswd_generator;
mod jwt_parser;
mod otp_code_generator_and_validator;
mod password_strength_analyser;
mod pdf_signature_checker;
mod rsa_encryption;
mod rsa_key_pair_generator;
mod string_obfuscator;
mod testing;
mod token_generator;
mod ulid_generator;
mod uuid_generator;

// Every tool file opens with `use crate::*;`, so the crate root is this
// crate's prelude: the shared types plus any helper one tool lends another.
#[allow(unused_imports)]
pub(crate) use {
    basic_auth_generator::*, bcrypt::*, bip39_generator::*, common::*, encryption::*, hash_text::*,
    hmac_generator::*, hotp_generator::*, htpasswd_generator::*, jwt_parser::*,
    otp_code_generator_and_validator::*, password_strength_analyser::*, pdf_signature_checker::*,
    rsa_encryption::*, rsa_key_pair_generator::*, string_obfuscator::*, token_generator::*,
    ulid_generator::*, uuid_generator::*,
};

/// Reached for by name from `crates/app`'s bespoke panes.
pub use bcrypt::{bcrypt_hash_with_rounds, bcrypt_verify};

/// Every tool in this crate, in registry order.
pub static TOOLS: &[&Tool] = &[
    &hash_text::TOOL,
    &hmac_generator::TOOL,
    &bcrypt::TOOL,
    &encryption::TOOL,
    &token_generator::TOOL,
    &password_strength_analyser::TOOL,
    &otp_code_generator_and_validator::TOOL,
    &hotp_generator::TOOL,
    &bip39_generator::TOOL,
    &rsa_key_pair_generator::TOOL,
    &rsa_encryption::TOOL,
    &uuid_generator::TOOL,
    &ulid_generator::TOOL,
    &jwt_parser::TOOL,
    &basic_auth_generator::TOOL,
    &htpasswd_generator::TOOL,
    &string_obfuscator::TOOL,
    &pdf_signature_checker::TOOL,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    // ---- hmac

    #[test]
    fn hmac_rfc_vectors() {
        // RFC 2202 test case 1
        let key = vec![0x0bu8; 16];
        assert_eq!(
            hex_encode(&hmac_bytes("MD5", &key, b"Hi There").unwrap()),
            "9294727a3638bb1c13f48ef8158bfc9d"
        );
        // RFC 4231 test case 2
        assert_eq!(
            hex_encode(&hmac_bytes("SHA256", b"Jefe", b"what do ya want for nothing?").unwrap()),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert_eq!(
            hex_encode(&hmac_bytes("SHA1", b"Jefe", b"what do ya want for nothing?").unwrap()),
            "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"
        );
    }

    #[test]
    fn hmac_tool() {
        let out = run_hmac(&args(&[
            ("text", "what do ya want for nothing?"),
            ("secret", "Jefe"),
            ("algo", "SHA256"),
        ]))
        .unwrap();
        assert_eq!(
            out,
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert!(run_hmac(&args(&[("algo", "nope")])).is_err());
    }

    // ---- rsa

    #[test]
    fn rsa_guard_does_no_work_when_generate_is_unset() {
        for pairs in [vec![], vec![("bits", "4096")], vec![("generate", "false")]] {
            let out = run_rsa(&args(&pairs)).unwrap();
            assert_eq!(out, "set generate to true to produce a key pair");
        }
    }

    /// Sections, keyed by label — the panels are what the UI copies out.
    fn panels(out: &str) -> std::collections::BTreeMap<&str, String> {
        split_sections(out)
            .into_iter()
            .map(|(label, value)| (label, value.trim().to_string()))
            .collect()
    }

    #[test]
    fn rsa_generates_parseable_pem() {
        use rsa::pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey};
        use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
        // 512 bits only: anything larger makes the test suite crawl.
        let out = run_rsa(&args(&[("bits", "512"), ("generate", "true")])).unwrap();
        // Public key panel first, as upstream.
        assert!(
            out.starts_with("Public key:\n-----BEGIN PUBLIC KEY-----"),
            "{out}"
        );
        let p = panels(&out);
        let key = rsa::RsaPrivateKey::from_pkcs8_pem(&p["Private key"]).unwrap();
        assert_eq!(rsa::traits::PublicKeyParts::size(&key) * 8, 512);
        assert_eq!(
            rsa::RsaPublicKey::from_public_key_pem(&p["Public key"]).unwrap(),
            key.to_public_key()
        );

        // PKCS#1 is the same key pair in the older, RSA-only encoding.
        let out = run_rsa(&args(&[
            ("bits", "512"),
            ("generate", "true"),
            ("format", "PKCS#1"),
        ]))
        .unwrap();
        let p = panels(&out);
        assert!(
            p["Private key"].starts_with("-----BEGIN RSA PRIVATE KEY-----"),
            "{out}"
        );
        let key = rsa::RsaPrivateKey::from_pkcs1_pem(&p["Private key"]).unwrap();
        assert_eq!(
            rsa::RsaPublicKey::from_pkcs1_pem(&p["Public key"]).unwrap(),
            key.to_public_key()
        );
        // The OpenSSH line is the same key again, hand-encoded.
        assert_eq!(
            p["Public key (OpenSSH)"],
            openssh_public_key(&key.to_public_key())
        );

        assert!(run_rsa(&args(&[("bits", "777"), ("generate", "true")])).is_err());
        assert!(
            run_rsa(&args(&[
                ("bits", "512"),
                ("generate", "true"),
                ("format", "DER")
            ]))
            .is_err()
        );
    }

    /// The OpenSSH wire encoding is hand-written, so decode it back and check
    /// every field — including the leading zero byte an mpint needs when its
    /// top bit is set, which is the one rule that is easy to get wrong.
    #[test]
    fn openssh_public_key_wire_format() {
        use base64::Engine;
        use rsa::traits::PublicKeyParts;

        let key = rsa::RsaPrivateKey::new(&mut RsaRng, 512)
            .unwrap()
            .to_public_key();
        let line = openssh_public_key(&key);
        // "ssh-rsa" as a length-prefixed string always base64s to this prefix.
        assert!(line.starts_with("ssh-rsa AAAAB3NzaC1yc2E"), "{line}");
        let blob = base64::engine::general_purpose::STANDARD
            .decode(line.trim_start_matches("ssh-rsa "))
            .unwrap();

        let mut rest = &blob[..];
        let mut take = || {
            let len = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
            let (field, tail) = rest[4..].split_at(len);
            rest = tail;
            field.to_vec()
        };
        assert_eq!(take(), b"ssh-rsa");
        let e = take();
        let n = take();
        assert!(rest.is_empty(), "trailing bytes in the blob");
        assert_eq!(rsa::BigUint::from_bytes_be(&e), *key.e());
        assert_eq!(rsa::BigUint::from_bytes_be(&n), *key.n());
        // A 512 bit modulus always has its top bit set, so the pad is required.
        assert_eq!(n.len(), 65);
        assert_eq!(n[0], 0);
    }

    // ---- otp / totp

    #[test]
    fn hex_to_bytes_vectors() {
        assert_eq!(hex_to_bytes("1").unwrap(), vec![1]);
        assert_eq!(hex_to_bytes("ffffff").unwrap(), vec![255, 255, 255]);
        assert_eq!(hex_to_bytes("000000000").unwrap(), vec![0, 0, 0, 0, 0]);
        assert_eq!(
            hex_to_bytes("a3218bcef89").unwrap(),
            vec![163, 33, 139, 206, 248, 9]
        );
        assert_eq!(hex_to_bytes("063679ca").unwrap(), vec![6, 54, 121, 202]);
        assert_eq!(
            hex_to_bytes("0102030405060708090a0b0c0d0e0f").unwrap(),
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
        );
        assert!(hex_to_bytes("zz").is_err());
    }

    #[test]
    fn base32_to_hex_vectors() {
        assert_eq!(base32_to_hex("ABCDEF").unwrap(), "00443205");
        assert_eq!(base32_to_hex("7777").unwrap(), "ffff0f");
        assert_eq!(
            base32_to_hex("JBSWY3DPEHPK3PXP").unwrap(),
            "48656c6c6f21deadbeef"
        );
        assert_eq!(base32_to_hex("ABC").unwrap(), base32_to_hex("abc").unwrap());
        assert!(base32_to_hex("1").is_err());
    }

    const OTP_KEY: &str = "JBSWY3DPEHPK3PXP";
    const HOTP_CODES: [&str; 5] = ["282760", "996554", "602287", "143627", "960129"];

    #[test]
    fn generate_hotp_vectors() {
        for (counter, code) in HOTP_CODES.iter().enumerate() {
            assert_eq!(&generate_hotp(OTP_KEY, counter as u64).unwrap(), code);
        }
    }

    #[test]
    fn totp_vectors_at_fixed_timestamps() {
        for (token, now) in [
            ("282760", 0i64),
            ("341128", 1_465_324_707_000),
            ("089029", 1_365_324_707_000),
        ] {
            let out = run_otp(&args(&[
                ("secret", OTP_KEY),
                ("timestamp_ms", &now.to_string()),
            ]))
            .unwrap();
            assert!(out.contains(&format!("current:  {token}")), "{out}");
        }
    }

    /// The three codes shown side by side are the counter's neighbours — this is
    /// how the upstream UI lets you validate a code you were given.
    #[test]
    fn totp_shows_previous_current_next() {
        let out = run_otp(&args(&[("secret", OTP_KEY), ("timestamp_ms", "30000")])).unwrap();
        assert!(
            out.contains(&format!("previous: {}", HOTP_CODES[0])),
            "{out}"
        );
        assert!(
            out.contains(&format!("current:  {}", HOTP_CODES[1])),
            "{out}"
        );
        assert!(
            out.contains(&format!("next:     {}", HOTP_CODES[2])),
            "{out}"
        );
        assert!(out.contains("Iteration count:\n  1"), "{out}");
        assert!(
            out.contains("Iteration count, padded hex:\n  0000000000000001"),
            "{out}"
        );
        assert!(out.contains("Epoch:\n  30"), "{out}");
        assert!(
            out.contains("Secret in hexadecimal:\n48656c6c6f21deadbeef"),
            "{out}"
        );
    }

    #[test]
    fn otp_key_uri_and_empty_secret() {
        let out = run_otp(&args(&[("secret", OTP_KEY)])).unwrap();
        assert!(
            out.contains(
                "otpauth://totp/IT-Tools:demo-user?issuer=IT-Tools&\
                 secret=JBSWY3DPEHPK3PXP&algorithm=SHA1&digits=6&period=30"
            ),
            "{out}"
        );
        assert_eq!(
            build_key_uri(OTP_KEY, "IT-Tools", "demo-user", 6, 30),
            "otpauth://totp/IT-Tools:demo-user?issuer=IT-Tools&secret=JBSWY3DPEHPK3PXP&algorithm=SHA1&digits=6&period=30"
        );
        // No secret: nothing at all is computable, so this one is a real Err.
        assert!(run_otp(&args(&[])).is_err());
        // A non-base32 secret still shows the key URI, epoch and counter.
        let out = run_otp(&args(&[("secret", "111"), ("timestamp_ms", "0")])).unwrap();
        assert!(out.contains("Codes:\n  (not a base32 character"), "{out}");
        assert!(out.contains("secret=111"), "{out}");
    }

    // ---- bip39

    #[test]
    fn bip39_known_vectors() {
        // BIP39 English test vector: all-zero entropy.
        let out = run_bip39(&args(&[("entropy", "00000000000000000000000000000000")])).unwrap();
        assert!(
            out.contains(
                "mnemonic: abandon abandon abandon abandon abandon abandon abandon abandon \
                 abandon abandon abandon about"
            ),
            "{out}"
        );
        let out = run_bip39(&args(&[(
            "mnemonic",
            "legal winner thank year wave sausage worth useful legal winner thank yellow",
        )]))
        .unwrap();
        assert!(
            out.contains("entropy: 7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f"),
            "{out}"
        );
    }

    #[test]
    fn bip39_random_and_errors() {
        // empty args -> random 16 byte entropy -> 12 words
        let out = run_bip39(&args(&[])).unwrap();
        let mnemonic = out.lines().nth(1).unwrap().trim_start_matches("mnemonic: ");
        assert_eq!(mnemonic.split_whitespace().count(), 12);
        assert!(run_bip39(&args(&[("entropy", "aabb")])).is_err());
        assert!(run_bip39(&args(&[("entropy", "zz")])).is_err());
        assert!(run_bip39(&args(&[("mnemonic", "not a real mnemonic")])).is_err());
    }

    // ---- uuid/ulid

    #[test]
    fn uuid_versions() {
        assert_eq!(
            run_uuid(&args(&[("version", "NIL")])).unwrap().trim(),
            NIL_UUID
        );
        let out = run_uuid(&args(&[("count", "5")])).unwrap();
        assert_eq!(out.lines().count(), 5);
        assert!(out.lines().all(|l| uuid::Uuid::parse_str(l).is_ok()));
        // RFC 4122 name-based vectors for the DNS namespace and "example.com"
        let dns = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
        assert_eq!(
            uuid_name_based(dns, "example.com", 3).unwrap(),
            "9073926b-929f-31c2-abc9-fad77ae3e8eb"
        );
        assert_eq!(
            uuid_name_based(dns, "example.com", 5).unwrap(),
            "cfbff0d1-9375-5685-968c-48ce8b15ae17"
        );
        assert!(run_uuid(&args(&[("version", "v3"), ("namespace", "nope")])).is_err());
        assert!(run_uuid(&args(&[("version", "v9")])).is_err());
        // it-tools PR #1441 adds v6 and v7; there is still no max UUID.
        assert!(run_uuid(&args(&[("version", "max")])).is_err());
        assert!(run_uuid(&args(&[("count", "0")])).is_err());
        assert!(run_uuid(&args(&[("count", "51")])).is_err());
        // The version nibble is what tells v6 and v7 apart from v1/v4 — the
        // sub-millisecond ordering is not asserted, since `uuid`'s NoContext
        // fills the low bits with randomness rather than a counter.
        for (v, num) in [("v6", 6), ("v7", 7)] {
            let out = run_uuid(&args(&[("version", v), ("count", "3")])).unwrap();
            assert_eq!(out.lines().count(), 3);
            for line in out.lines() {
                let id = uuid::Uuid::parse_str(line).unwrap();
                assert_eq!(id.get_version_num(), num, "{v}: {line}");
            }
        }
        for v in ["v1", "v4"] {
            let id = run_uuid(&args(&[("version", v)])).unwrap();
            assert!(uuid::Uuid::parse_str(id.trim()).is_ok(), "{v}: {id}");
        }
    }

    #[test]
    fn ulid_formats() {
        let out = run_ulid(&args(&[("count", "3")])).unwrap();
        assert_eq!(out.lines().count(), 3);
        assert!(out.lines().all(|l| ulid::Ulid::from_string(l).is_ok()));
        let json = run_ulid(&args(&[("count", "2"), ("format", "JSON")])).unwrap();
        let parsed: Vec<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 2);
        assert!(run_ulid(&args(&[("format", "XML")])).is_err());
        assert!(run_ulid(&args(&[("count", "1000")])).is_err());
    }

    // ---- jwt

    #[test]
    fn jwt_decode() {
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
                   eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.\
                   SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let out = run_jwt(&args(&[("jwt", jwt)])).unwrap();
        // Every claim, with the IANA/RFC 7518 human-readable names the table shows.
        assert!(
            out.contains("alg (Algorithm): HS256 (HMAC using SHA-256)"),
            "{out}"
        );
        assert!(out.contains("typ (Type): JWT"), "{out}");
        assert!(out.contains("sub (Subject): 1234567890"), "{out}");
        assert!(out.contains("name (Full name): John Doe"), "{out}");
        assert!(
            out.contains("iat (Issued At): 1516239022 (2018-01-18 01:30:22 UTC)"),
            "{out}"
        );
        assert!(out.starts_with("Header:\n"), "{out}");
        assert!(out.contains("\nPayload:\n"), "{out}");
        assert!(run_jwt(&args(&[("jwt", "garbage")])).is_err());
        assert!(run_jwt(&args(&[])).is_err());
        // An unparseable segment reports itself in its own panel.
        let out = run_jwt(&args(&[("jwt", "aaaa.bbbb.cccc")])).unwrap();
        assert!(out.contains("Header:\n  ("), "{out}");
        assert!(out.contains("Payload:\n  ("), "{out}");
        // An unknown claim is still listed, just without a description.
        let out = run_jwt(&args(&[(
            "jwt",
            "eyJhbGciOiJub25lIn0.eyJteV9jbGFpbSI6eyJhIjpbMV19fQ.",
        )]))
        .unwrap();
        assert!(out.contains("my_claim: {"), "{out}");
        assert!(
            out.contains("alg (Algorithm): none (No digital signature or MAC performed)"),
            "{out}"
        );
    }

    // ---- basic auth

    #[test]
    fn basic_auth_header() {
        assert_eq!(
            run_basic_auth(&args(&[
                ("username", "Aladdin"),
                ("password", "open sesame")
            ]))
            .unwrap(),
            "Authorization: Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
        assert_eq!(
            run_basic_auth(&args(&[])).unwrap(),
            "Authorization: Basic Og=="
        );
    }

    // ---- registry

    #[test]
    fn custom_tools_explain_themselves() {
        for tool in TOOLS.iter().filter(|t| t.kind == Kind::Custom) {
            let err = (tool.run)(&Args::new()).unwrap_err().to_string();
            assert!(err.contains("needs bespoke UI"), "{}: {err}", tool.id);
        }
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = TOOLS.iter().map(|t| t.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate tool id");
    }

    #[test]
    fn no_tool_panics_on_empty_args() {
        for tool in TOOLS {
            let _ = (tool.run)(&Args::new());
            // and on garbage in every declared field
            let garbage: Args = tool
                .fields
                .iter()
                .map(|f| (f.key.to_string(), "\u{fffd}!!".into()))
                .collect();
            let _ = (tool.run)(&garbage);
        }
    }
}
