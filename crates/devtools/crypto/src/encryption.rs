//! `encryption`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha2::Digest;

// it-tools uses crypto-js, whose `Algo.encrypt(text, password)` emits the
// OpenSSL envelope: base64("Salted__" + 8 salt bytes + ciphertext), with key
// and IV from EVP_BytesToKey (MD5, iteration count 1).
//
// ponytail: Rabbit is in it-tools' list but has no Rust crate, so it is simply
// absent from ENCRYPT_ALGOS. Mislabelling another stream cipher as Rabbit would
// silently produce undecryptable output. Add it if someone ports the cipher.
//
// AES-GCM is likewise absent, and stays absent: GCM is not a block mode this
// crate can assemble from `aes` and `cbc` — it needs GHASH (the `ghash`/
// `aes-gcm` crates), which would be a new dependency. Hand-rolling a GF(2^128)
// multiply for an authenticated mode is exactly the code that must not be
// written twice, so CBC it remains.
pub(crate) const ENCRYPT_ALGOS: &[&str] = &["AES", "TripleDES", "RC4"];
pub(crate) const OPENSSL_MAGIC: &[u8] = b"Salted__";

/// crypto-js `EvpKDF` defaults / OpenSSL `EVP_BytesToKey`: MD5, one iteration.
/// Returns `key_len + iv_len` bytes, key first.
pub(crate) fn evp_bytes_to_key(pass: &[u8], salt: &[u8], key_len: usize, iv_len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(key_len + iv_len);
    let mut block = Vec::new();
    while out.len() < key_len + iv_len {
        let mut h = md5::Md5::new();
        h.update(&block);
        h.update(pass);
        h.update(salt);
        block = h.finalize().to_vec();
        out.extend_from_slice(&block);
    }
    out.truncate(key_len + iv_len);
    out
}

/// `(key_len, iv_len)` from the crypto-js cipher configs: AES keySize 256/32
/// with a 128 bit block, TripleDES 192/32 with a 64 bit block, RC4 256/32 and
/// no IV.
pub(crate) fn algo_sizes(algo: &str) -> Result<(usize, usize)> {
    Ok(match algo {
        "AES" => (32, 16),
        "TripleDES" => (24, 8),
        "RC4" => (32, 0),
        other => bail!("unknown algorithm: {other}"),
    })
}

/// CBC with PKCS#7 for one concrete block cipher. `cbc` is built without its
/// `alloc` feature here, hence the explicit buffer (block size is <= 16).
macro_rules! cbc_apply {
    ($cipher:ty, $key:expr, $iv:expr, $data:expr, $encrypt:expr) => {{
        use aes::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7};
        let mut buf = vec![0u8; $data.len() + 16];
        let bad_key = || anyhow::anyhow!("derived key or IV has the wrong length");
        let n = if $encrypt {
            <cbc::Encryptor<$cipher>>::new_from_slices($key, $iv)
                .map_err(|_| bad_key())?
                .encrypt_padded_b2b::<Pkcs7>($data, &mut buf)
                .map_err(|_| anyhow::anyhow!("output buffer too small"))?
                .len()
        } else {
            <cbc::Decryptor<$cipher>>::new_from_slices($key, $iv)
                .map_err(|_| bad_key())?
                .decrypt_padded_b2b::<Pkcs7>($data, &mut buf)
                .map_err(|_| anyhow::anyhow!("wrong password, or corrupted ciphertext"))?
                .len()
        };
        buf.truncate(n);
        buf
    }};
}

pub(crate) fn cipher_apply(
    algo: &str,
    key: &[u8],
    iv: &[u8],
    data: &[u8],
    encrypt: bool,
) -> Result<Vec<u8>> {
    Ok(match algo {
        "AES" => cbc_apply!(aes::Aes256, key, iv, data, encrypt),
        "TripleDES" => cbc_apply!(des::TdesEde3, key, iv, data, encrypt),
        // RC4 is a stream cipher: keystream XOR, same both directions, no IV.
        "RC4" => {
            let mut c = <rc4::Rc4 as rc4::KeyInit>::new_from_slice(key)
                .map_err(|_| anyhow::anyhow!("derived key has the wrong length"))?;
            let mut buf = data.to_vec();
            rc4::StreamCipher::apply_keystream(&mut c, &mut buf);
            buf
        }
        other => bail!("unknown algorithm: {other}"),
    })
}

/// How the secret is spelled, and optionally the IV to use.
///
/// it-tools PRs #1157 (key encoding), #373 and #1585 (bring your own IV). Both
/// cards carry one of these, which is why it is a struct rather than three more
/// positional `&str`s.
#[derive(Clone, Copy)]
pub(crate) struct KeySpec<'a> {
    pub secret: &'a str,
    pub encoding: &'a str,
    /// Blank means "derive it", i.e. exactly the previous behaviour.
    pub iv: &'a str,
}

impl KeySpec<'_> {
    /// The secret as bytes, plus the IV if one was given.
    ///
    /// With an IV the secret stops being a password: crypto-js'
    /// `encrypt(text, keyWordArray, { iv })` takes the key as-is, derives
    /// nothing and emits no salt, so the key has to be exactly the right length
    /// and the output has no `Salted__` envelope.
    fn resolve(&self, algo: &str) -> Result<(Vec<u8>, Option<Vec<u8>>)> {
        let (key_len, iv_len) = algo_sizes(algo)?;
        let key = decode_key("secret key", self.secret, self.encoding)?;
        if self.iv.trim().is_empty() {
            return Ok((key, None));
        }
        // The IV is read with the same encoding select as the key: two
        // encodings for one card is a knob nobody wants to keep in sync.
        let iv = decode_key("initialisation vector", self.iv, self.encoding)?;
        if iv_len == 0 {
            bail!("{algo} is a stream cipher and takes no IV; leave the field empty");
        }
        if iv.len() != iv_len {
            bail!("{algo} needs a {iv_len} byte IV, got {}", iv.len());
        }
        if key.len() != key_len {
            bail!(
                "with an IV set the secret is the raw key, not a password: \
                 {algo} needs {key_len} bytes, got {}",
                key.len()
            );
        }
        Ok((key, Some(iv)))
    }
}

pub(crate) fn crypto_js_encrypt(algo: &str, spec: KeySpec, plain: &str) -> Result<String> {
    let (secret, explicit_iv) = spec.resolve(algo)?;
    if let Some(iv) = explicit_iv {
        return Ok(STANDARD.encode(cipher_apply(algo, &secret, &iv, plain.as_bytes(), true)?));
    }
    let (key_len, iv_len) = algo_sizes(algo)?;
    let salt: [u8; 8] = std::array::from_fn(|_| rand::random());
    let kdf = evp_bytes_to_key(&secret, &salt, key_len, iv_len);
    let mut out = OPENSSL_MAGIC.to_vec();
    out.extend_from_slice(&salt);
    out.extend_from_slice(&cipher_apply(
        algo,
        &kdf[..key_len],
        &kdf[key_len..],
        plain.as_bytes(),
        true,
    )?);
    Ok(STANDARD.encode(out))
}

pub(crate) fn crypto_js_decrypt(algo: &str, spec: KeySpec, payload: &str) -> Result<String> {
    let (secret, explicit_iv) = spec.resolve(algo)?;
    let compact: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    let raw = STANDARD
        .decode(&compact)
        .context("ciphertext is not valid base64")?;
    let plain = match explicit_iv {
        Some(iv) => {
            if raw.starts_with(OPENSSL_MAGIC) {
                bail!(
                    "this payload carries a \"Salted__\" header, so its key and IV come \
                     from the password — clear the IV field"
                );
            }
            cipher_apply(algo, &secret, &iv, &raw, false)?
        }
        None => {
            if raw.len() < 16 || &raw[..8] != OPENSSL_MAGIC {
                bail!("not a crypto-js/OpenSSL payload: expected a \"Salted__\" header");
            }
            let (key_len, iv_len) = algo_sizes(algo)?;
            let kdf = evp_bytes_to_key(&secret, &raw[8..16], key_len, iv_len);
            cipher_apply(algo, &kdf[..key_len], &kdf[key_len..], &raw[16..], false)?
        }
    };
    String::from_utf8(plain)
        .context("decrypted bytes are not valid UTF-8: wrong password or algorithm")
}

// The `.vue` has no direction toggle: "Encrypt" and "Decrypt" are two separate
// cards with their own text, secret and algorithm, both live at once. So there
// is no `mode` field, and a failing decrypt (the common case, since the box
// usually holds a half-pasted payload) only reports inside its own section.
pub(crate) fn run_encryption(a: &Args) -> Result<String> {
    Ok(format!(
        "{}\n{}\n",
        crate::section(
            "Encrypt",
            crypto_js_encrypt(
                s(a, "algo", "AES"),
                KeySpec {
                    secret: s(a, "secret", ""),
                    encoding: s(a, "key_encoding", "Text"),
                    iv: s(a, "iv", ""),
                },
                s(a, "text", "")
            ),
        ),
        crate::section(
            "Decrypt",
            crypto_js_decrypt(
                s(a, "decrypt_algo", "AES"),
                KeySpec {
                    secret: s(a, "decrypt_secret", ""),
                    encoding: s(a, "decrypt_key_encoding", "Text"),
                    iv: s(a, "decrypt_iv", ""),
                },
                s(a, "decrypt_text", ""),
            ),
        )
    ))
}

pub static TOOL: Tool = Tool {
    id: "encryption",
    name: "Encrypt / decrypt text",
    description: "Encrypt and decrypt text with AES, TripleDES or RC4 (crypto-js compatible).",
    category: Category::Crypto,
    keywords: &["encrypt", "decrypt", "aes", "tripledes", "rc4", "cypher"],
    // Encrypt and Decrypt are separate cards upstream, each with its own
    // text, secret and algorithm — no direction toggle.
    fields: &[
        Field::area("text", "Your text (encrypt)"),
        Field::text("secret", "Your secret key (encrypt)"),
        Field::select(
            "key_encoding",
            "Key and IV encoding (encrypt)",
            KEY_ENCODINGS,
            "Text",
        ),
        Field::text("iv", "IV, blank to derive one (encrypt)"),
        Field::select(
            "algo",
            "Encryption algorithm (encrypt)",
            ENCRYPT_ALGOS,
            "AES",
        ),
        Field::area("decrypt_text", "Your encrypted text (decrypt)"),
        Field::text("decrypt_secret", "Your secret key (decrypt)"),
        Field::select(
            "decrypt_key_encoding",
            "Key and IV encoding (decrypt)",
            KEY_ENCODINGS,
            "Text",
        ),
        Field::text("decrypt_iv", "IV, blank to derive one (decrypt)"),
        Field::select(
            "decrypt_algo",
            "Encryption algorithm (decrypt)",
            ENCRYPT_ALGOS,
            "AES",
        ),
    ],
    kind: Kind::Pure,
    run: run_encryption,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    // Interop vectors: produced by crypto-js 4.2 itself (the library it-tools
    // uses) via `CryptoJS.<Algo>.encrypt('Lorem ipsum dolor sit amet', 'my
    // secret key').toString()`. The AES and TripleDES ones were then confirmed
    // byte-for-byte against the OpenSSL CLI, e.g.
    //   echo "<b64>" | openssl enc -d -aes-256-cbc -a -md md5 -k "my secret key"
    // RC4 cannot be checked that way: OpenSSL's `enc -rc4` derives a 16 byte
    // key, crypto-js a 32 byte one, so crypto-js is the sole authority there.
    const CJS_TEXT: &str = "Lorem ipsum dolor sit amet";
    const CJS_SECRET: &str = "my secret key";

    /// The historical call: a text password, no IV.
    fn pw(secret: &str) -> KeySpec<'_> {
        KeySpec {
            secret,
            encoding: "Text",
            iv: "",
        }
    }
    const CJS_VECTORS: [(&str, &str); 3] = [
        (
            "AES",
            "U2FsdGVkX193Qa6b5hgK6cEkMG5p5tSla0zqq+1A6SyuNZ0KaaoeifnJNT5ZkeLd",
        ),
        (
            "TripleDES",
            "U2FsdGVkX1/F97NFaiJVK9aT19wISeGLPJJK1pswgxHekBrkAyjN1BSuWrr8cFWV",
        ),
        (
            "RC4",
            "U2FsdGVkX1+Dxw/H5I7eOjggKuZDy6Fl5S9CUDG5yRaMuoxJD8i6x7J0",
        ),
    ];

    #[test]
    fn decrypts_crypto_js_ciphertext() {
        for (algo, payload) in CJS_VECTORS {
            assert_eq!(
                crypto_js_decrypt(algo, pw(CJS_SECRET), payload).unwrap(),
                CJS_TEXT,
                "{algo}"
            );
            // ...and through the tool, whose Decrypt panel is always live.
            let out = run_encryption(&args(&[
                ("decrypt_algo", algo),
                ("decrypt_text", payload),
                ("decrypt_secret", CJS_SECRET),
            ]))
            .unwrap();
            assert!(
                out.contains(&format!("Decrypt:\n{CJS_TEXT}")),
                "{algo}: {out}"
            );
        }
    }

    /// Both cards are always rendered, and a failing one only reports itself.
    #[test]
    fn encryption_panels_are_independent() {
        let out = run_encryption(&args(&[
            ("text", "hello"),
            ("secret", "pw"),
            ("decrypt_text", "not base64 !!!"),
            ("decrypt_secret", "pw"),
        ]))
        .unwrap();
        assert!(out.contains("Encrypt:\nU2FsdGVkX1"), "{out}");
        assert!(
            out.contains("Decrypt:\n  (ciphertext is not valid base64"),
            "{out}"
        );
    }

    #[test]
    fn evp_bytes_to_key_vector() {
        // Same derivation crypto-js's EvpKDF performs for the salt 0102030405060708.
        let kdf = evp_bytes_to_key(CJS_SECRET.as_bytes(), &[1, 2, 3, 4, 5, 6, 7, 8], 32, 16);
        assert_eq!(
            hex_encode(&kdf),
            "64a425c9fac4cfbfc73042e5005633b3ec4bc35ac7f20af686faf24823c006c5\
             8d858c00f23d97879b0faf84de65603a"
        );
    }

    #[test]
    fn encryption_round_trips() {
        for algo in ENCRYPT_ALGOS {
            for text in [
                "",
                "a",
                CJS_TEXT,
                &"x".repeat(1000),
                "unicode: héllo · 世界",
            ] {
                let ct = crypto_js_encrypt(algo, pw("pw"), text).unwrap();
                assert!(ct.starts_with("U2FsdGVkX1"), "{algo}: {ct}");
                assert_eq!(
                    crypto_js_decrypt(algo, pw("pw"), &ct).unwrap(),
                    text,
                    "{algo}"
                );
            }
        }
    }

    #[test]
    fn encryption_rejects_bad_input() {
        let decrypt =
            |algo: &str, text: &str, secret: &str| crypto_js_decrypt(algo, pw(secret), text);
        // Wrong password on the fixed vectors: an Err, never a panic.
        for (algo, payload) in CJS_VECTORS {
            assert!(
                decrypt(algo, payload, "not the password").is_err(),
                "{algo}"
            );
        }
        assert!(decrypt("AES", "not base64 !!!", CJS_SECRET).is_err());
        assert!(decrypt("AES", "", CJS_SECRET).is_err());
        // valid base64 but no Salted__ header
        assert!(decrypt("AES", &STANDARD.encode([0u8; 32]), CJS_SECRET).is_err());
        // truncated ciphertext -> not a whole number of blocks
        assert!(decrypt("AES", &CJS_VECTORS[0].1[..24], CJS_SECRET).is_err());
        // Rabbit is in it-tools' list but has no Rust crate; it must not be
        // silently swapped for another cipher.
        assert!(crypto_js_encrypt("Rabbit", pw("pw"), "x").is_err());
        assert!(!ENCRYPT_ALGOS.contains(&"Rabbit"));
    }

    /// The same binary key, spelled two ways, is the same key — and neither is
    /// the UTF-8 mangling that "Text" would produce for the same code points.
    #[test]
    fn key_encodings_agree() {
        let key = [0x00u8, 0xff, 0x10, 0x20];
        let hex = KeySpec {
            secret: &hex_encode(&key),
            encoding: "Hexadecimal",
            iv: "",
        };
        let b64 = KeySpec {
            secret: &STANDARD.encode(key),
            encoding: "Base64",
            iv: "",
        };
        let ct = crypto_js_encrypt("AES", hex, CJS_TEXT).unwrap();
        assert_eq!(crypto_js_decrypt("AES", hex, &ct).unwrap(), CJS_TEXT);
        assert_eq!(crypto_js_decrypt("AES", b64, &ct).unwrap(), CJS_TEXT);
        // "Text" reads U+00FF as two UTF-8 bytes; that is the mangling the
        // select exists to avoid. (Asserted on the decoder, not on a failed
        // decrypt: a wrong key still unpads by luck once in ~256 tries.)
        assert_eq!(
            decode_key("k", &hex_encode(&key), "Hexadecimal").unwrap(),
            key
        );
        assert_eq!(decode_key("k", "\u{ff}", "Text").unwrap(), vec![0xc3, 0xbf]);
        // A key that is not what it claims to be reports itself.
        assert!(
            crypto_js_encrypt(
                "AES",
                KeySpec {
                    secret: "zz",
                    encoding: "Hexadecimal",
                    iv: ""
                },
                "x"
            )
            .is_err()
        );
    }

    /// A supplied IV means "raw key, no salt", the crypto-js
    /// `encrypt(text, key, { iv })` shape.
    #[test]
    fn explicit_iv_round_trips_and_is_checked() {
        let spec = KeySpec {
            secret: &hex_encode(&[0x11u8; 32]),
            encoding: "Hexadecimal",
            iv: &hex_encode(&[0x22u8; 16]),
        };
        let ct = crypto_js_encrypt("AES", spec, CJS_TEXT).unwrap();
        // No salt is generated, so this path is reproducible...
        assert!(!ct.starts_with("U2FsdGVkX1"), "{ct}");
        assert_eq!(crypto_js_encrypt("AES", spec, CJS_TEXT).unwrap(), ct);
        assert_eq!(crypto_js_decrypt("AES", spec, &ct).unwrap(), CJS_TEXT);

        let with_iv = |algo: &str, iv: &str, key_bytes: usize| {
            crypto_js_encrypt(
                algo,
                KeySpec {
                    secret: &hex_encode(&vec![0x11u8; key_bytes]),
                    encoding: "Hexadecimal",
                    iv,
                },
                "x",
            )
        };
        // Wrong IV length names the length it wants.
        let e = with_iv("AES", &hex_encode(&[0u8; 8]), 32)
            .unwrap_err()
            .to_string();
        assert!(e.contains("AES needs a 16 byte IV, got 8"), "{e}");
        let e = with_iv("TripleDES", &hex_encode(&[0u8; 16]), 24)
            .unwrap_err()
            .to_string();
        assert!(e.contains("TripleDES needs a 8 byte IV, got 16"), "{e}");
        // RC4 has no IV at all.
        let e = with_iv("RC4", &hex_encode(&[0u8; 8]), 32)
            .unwrap_err()
            .to_string();
        assert!(e.contains("stream cipher"), "{e}");
        // ...and with an IV the secret must be the raw key, not a password.
        let e = with_iv("AES", &hex_encode(&[0u8; 16]), 8)
            .unwrap_err()
            .to_string();
        assert!(e.contains("AES needs 32 bytes, got 8"), "{e}");
        // A salted payload with an IV set is a user mix-up, not a padding error.
        let e = crypto_js_decrypt("AES", spec, CJS_VECTORS[0].1)
            .unwrap_err()
            .to_string();
        assert!(e.contains("clear the IV field"), "{e}");
    }

    /// Blank IV and Text encoding are the declared defaults, so the tool still
    /// opens on the old behaviour.
    #[test]
    fn defaults_are_the_old_behaviour() {
        let out = run_encryption(&args(&[
            ("decrypt_algo", "AES"),
            ("decrypt_text", CJS_VECTORS[0].1),
            ("decrypt_secret", CJS_SECRET),
        ]))
        .unwrap();
        assert!(out.contains(&format!("Decrypt:\n{CJS_TEXT}")), "{out}");
        assert!(out.contains("Encrypt:\nU2FsdGVkX1"), "{out}");
    }
}
