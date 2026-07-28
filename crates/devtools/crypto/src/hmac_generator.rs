//! `hmac-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::Result;

pub(crate) fn run_hmac(a: &Args) -> Result<String> {
    // it-tools PR #1156: an HMAC key is a byte string, and a binary one cannot
    // be typed. Default "Text" is the previous UTF-8 behaviour.
    let key = decode_key(
        "secret key",
        s(a, "secret", ""),
        s(a, "keyEncoding", "Text"),
    )?;
    let mac = hmac_bytes(s(a, "algo", "SHA256"), &key, s(a, "text", "").as_bytes())?;
    encode_digest(&mac, s(a, "encoding", "Hex"))
}

pub static TOOL: Tool = Tool {
    id: "hmac-generator",
    name: "Hmac generator",
    description: "Compute a hash-based message authentication code (HMAC) of a text.",
    category: Category::Crypto,
    keywords: &["hmac", "hash", "mac", "secret", "signature"],
    fields: &[
        Field::area("text", "Plain text to compute the hash"),
        Field::text("secret", "Secret key"),
        Field::select("keyEncoding", "Secret key encoding", KEY_ENCODINGS, "Text"),
        Field::select("algo", "Hashing function", HMAC_ALGOS, "SHA256"),
        Field::select("encoding", "Output encoding", ENCODINGS, "Hex"),
    ],
    kind: Kind::Pure,
    run: run_hmac,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    /// The same key spelled three ways must give the same MAC.
    #[test]
    fn hmac_key_encodings_agree() {
        // "Jefe" = 4a656665 = SmVmZQ==; RFC 4231 test case 2.
        const EXPECTED: &str = "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843";
        for (secret, encoding) in [
            ("Jefe", "Text"),
            ("4a656665", "Hexadecimal"),
            ("SmVmZQ==", "Base64"),
        ] {
            let out = run_hmac(&args(&[
                ("text", "what do ya want for nothing?"),
                ("secret", secret),
                ("keyEncoding", encoding),
            ]))
            .unwrap();
            assert_eq!(out, EXPECTED, "{encoding}");
        }
        // A key that is not what it claims to be reports itself.
        assert!(run_hmac(&args(&[("secret", "zz"), ("keyEncoding", "Hexadecimal")])).is_err());
        assert!(run_hmac(&args(&[("secret", "x"), ("keyEncoding", "Base64")])).is_err());
        // A binary key is no longer mangled through UTF-8: 0xff stays one byte.
        let hex = run_hmac(&args(&[("secret", "ff"), ("keyEncoding", "Hexadecimal")])).unwrap();
        let text = run_hmac(&args(&[("secret", "\u{ff}"), ("keyEncoding", "Text")])).unwrap();
        assert_ne!(hex, text);
    }
}
