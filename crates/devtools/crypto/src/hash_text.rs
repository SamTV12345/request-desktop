//! `hash-text`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::Result;

/// What a row is called in the output.
///
/// Only `SHA3` differs from its algorithm key, and deliberately so: it-tools
/// hashes with crypto-js, whose `SHA3` is Keccak-512 with the pre-standard
/// padding, *not* FIPS SHA3-512. The digests differ, so a user checking a row
/// against `sha3sum` needs the row itself to say which one it is — a comment in
/// `hash_bytes` does not reach them.
///
/// it-tools PR #986 adds the four FIPS variants; they live next to this row and
/// need no label, because their names already say which standard they follow.
pub(crate) fn hash_row_label(algo: &str) -> &str {
    match algo {
        "SHA3" => "SHA3 (Keccak-512, crypto-js, not FIPS SHA3-512)",
        other => other,
    }
}

pub(crate) fn run_hash_text(a: &Args) -> Result<String> {
    let text = s(a, "text", "");
    let encoding = s(a, "encoding", "Hex");
    let mut out = String::new();
    for algo in HASH_ALGOS {
        let digest = encode_digest(&hash_bytes(algo, text.as_bytes())?, encoding)?;
        out.push_str(&format!("{}: {digest}\n", hash_row_label(algo)));
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "hash-text",
    name: "Hash text",
    description: "Hash a text with MD5, SHA1, SHA2, SHA3 (Keccak) and RIPEMD160.",
    category: Category::Crypto,
    keywords: &[
        "hash", "md5", "sha", "sha1", "sha256", "sha512", "ripemd", "digest",
    ],
    fields: &[
        Field::area("text", "Your text to hash"),
        Field::select("encoding", "Digest encoding", ENCODINGS, "Hex"),
    ],
    kind: Kind::Pure,
    run: run_hash_text,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn convert_hex_to_bin_vectors() {
        assert_eq!(convert_hex_to_bin("").unwrap(), "");
        assert_eq!(convert_hex_to_bin("FF").unwrap(), "11111111");
        assert_eq!(
            convert_hex_to_bin(&"F".repeat(200)).unwrap(),
            "1111".repeat(200)
        );
        assert_eq!(
            convert_hex_to_bin("2123006AD00F694CE120").unwrap(),
            "00100001001000110000000001101010110100000000111101101001010011001110000100100000"
        );
        assert!(convert_hex_to_bin("zz").is_err());
    }

    #[test]
    fn known_digests_of_hello() {
        let h = |algo| hex_encode(&hash_bytes(algo, b"Hello").unwrap());
        assert_eq!(h("MD5"), "8b1a9953c4611296a827abf8c47804d7");
        assert_eq!(h("SHA1"), "f7ff9e8b7bb2e09b70935a5d785e0cc5d9d0abf0");
        assert_eq!(
            h("SHA256"),
            "185f8db32271fe25f561a6fc938b2e264306ec304eda518007d1764826381969"
        );
        assert_eq!(
            h("SHA224"),
            "4149da18aa8bfc2b1e382c6c26556d01a92c261b6436dad5e3be3fcc"
        );
        assert_eq!(
            h("SHA512"),
            "3615f80c9d293ed7402687f94b22d58e529b8cc7916f8fac7fddf7fbd5af4cf777d3d795a7a00a16bf7e7f3fb9561ee9baae480da9fe7a18769e71886b03f315"
        );
        assert_eq!(h("RIPEMD160"), "d44426aca8ae0a69cdbc4021c64fa5ad68ca32fe");
        assert!(hash_bytes("SHA3", b"Hello").unwrap().len() == 64);
        assert!(hash_bytes("nope", b"Hello").is_err());
    }

    #[test]
    fn empty_string_digests() {
        assert_eq!(
            hex_encode(&hash_bytes("MD5", b"").unwrap()),
            "d41d8cd98f00b204e9800998ecf8427e"
        );
        assert_eq!(
            hex_encode(&hash_bytes("SHA1", b"").unwrap()),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        // Keccak-512 of the empty string (crypto-js SHA3 default).
        assert_eq!(
            hex_encode(&hash_bytes("SHA3", b"").unwrap()),
            "0eab42de4c3ceb9235fc91acffe746b29c29a8c366b7c60e4e67c466f36a4304c00fa9caf9d87976ba469bcbe06713b435f091ef2769fb160cdab33d3670680e"
        );
        // …and FIPS SHA3-512 of the same input, which is a different digest.
        // That difference is the whole reason both rows exist (PR #986).
        assert_eq!(
            hex_encode(&hash_bytes("SHA3-512", b"").unwrap()),
            "a69f73cca23a9ac5c8b567dc185a756e97c982164fe25859e0d1dcc1475c80a615b2123af1f5f94c11e3e9402c3ac558f500199d95b6d3e301758586281dcd26"
        );
        assert_ne!(
            hash_bytes("SHA3", b"").unwrap(),
            hash_bytes("SHA3-512", b"").unwrap()
        );
        // NIST's own vectors for the shorter FIPS variants.
        assert_eq!(
            hex_encode(&hash_bytes("SHA3-256", b"").unwrap()),
            "a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a"
        );
        assert_eq!(
            hex_encode(&hash_bytes("SHA3-224", b"abc").unwrap()),
            "e642824c3f8cf24ad09234ee7d3c766fc9a3a5168d0c94ad73b46fdf"
        );
    }

    #[test]
    fn hash_text_encodings() {
        let out = run_hash_text(&args(&[("text", "Hello"), ("encoding", "Base64")])).unwrap();
        assert!(out.contains("MD5: ixqZU8RhEpaoJ6v4xHgE1w=="), "{out}");
        let out = run_hash_text(&args(&[("text", "Hello"), ("encoding", "Base64url")])).unwrap();
        assert!(out.contains("MD5: ixqZU8RhEpaoJ6v4xHgE1w"), "{out}");
        let out = run_hash_text(&args(&[("text", "Hello"), ("encoding", "Bin")])).unwrap();
        assert!(out.contains("MD5: 10001011000110101001"), "{out}");
        assert!(run_hash_text(&args(&[("encoding", "rot13")])).is_err());
        // every algorithm is listed
        let out = run_hash_text(&args(&[])).unwrap();
        assert_eq!(out.lines().count(), HASH_ALGOS.len());
    }

    /// The row must not let anyone mistake crypto-js' SHA3 for FIPS SHA3-512.
    #[test]
    fn sha3_row_names_the_variant() {
        let out = run_hash_text(&args(&[("text", "Hello")])).unwrap();
        assert!(
            out.contains("SHA3 (Keccak-512, crypto-js, not FIPS SHA3-512): "),
            "{out}"
        );
        assert_eq!(hash_row_label("SHA256"), "SHA256");
    }
}
