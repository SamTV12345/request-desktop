//! `jwt-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, section};
use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};

// it-tools PR #1453 — the counterpart to the ported `jwt-parser`.

pub(crate) const DEFAULT_CLAIMS: &str =
    r#"{"sub": "1234567890", "name": "John Doe", "admin": true}"#;
pub(crate) const DEFAULT_SECRET: &str = "a-string-secret-at-least-256-bits-long";

pub(crate) fn b64url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn jwt_generator(a: &Args) -> Result<String> {
    let alg = s(a, "algorithm", "HS256");
    let digest = match alg {
        "HS256" => "SHA256",
        "HS384" => "SHA384",
        "HS512" => "SHA512",
        other => bail!("unsupported algorithm: {other} (HMAC only)"),
    };
    let secret = s(a, "secret", DEFAULT_SECRET);
    let claims: Value = serde_json::from_str(s(a, "payload", DEFAULT_CLAIMS).trim())
        .context("the payload is not valid JSON")?;
    // Compact, key order preserved — the token must be reproducible from what
    // the user typed, so no injected `iat`.
    let header = json!({ "alg": alg, "typ": "JWT" });
    let signing_input = format!(
        "{}.{}",
        b64url(serde_json::to_string(&header)?.as_bytes()),
        b64url(serde_json::to_string(&claims)?.as_bytes())
    );
    let sig = hmac_bytes(digest, secret.as_bytes(), signing_input.as_bytes())?;
    let token = format!("{signing_input}.{}", b64url(&sig));
    Ok([
        section("Token", Ok(token)),
        section(
            "Header",
            serde_json::to_string_pretty(&header).map_err(Into::into),
        ),
        section(
            "Payload",
            serde_json::to_string_pretty(&claims).map_err(Into::into),
        ),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "jwt-generator",
    name: "JWT generator",
    description: "Sign a payload into a JSON Web Token with an HMAC secret.",
    category: Category::Crypto,
    keywords: &["jwt", "token", "sign", "hmac", "auth", "bearer"],
    fields: &[
        Field::area("payload", "Payload (JSON)").with_default(DEFAULT_CLAIMS),
        Field::text("secret", "Secret").with_default(DEFAULT_SECRET),
        Field::select(
            "algorithm",
            "Algorithm",
            &["HS256", "HS384", "HS512"],
            "HS256",
        ),
    ],
    kind: Kind::Pure,
    run: jwt_generator,
};
