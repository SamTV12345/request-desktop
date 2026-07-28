//! `jwt-parser`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

pub(crate) fn jwt_part(part: &str) -> Result<serde_json::Value> {
    let raw = URL_SAFE_NO_PAD
        .decode(part.trim_end_matches('='))
        .context("JWT segment is not valid base64url")?;
    serde_json::from_slice(&raw).context("JWT segment is not valid JSON")
}

pub(crate) fn friendly_date(v: &serde_json::Value) -> Option<String> {
    let secs = v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))?;
    chrono::DateTime::from_timestamp(secs, 0).map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
}

/// it-tools' `ALGORITHM_DESCRIPTIONS`, from RFC 7518 section 3.1.
pub(crate) const ALGORITHM_DESCRIPTIONS: &[(&str, &str)] = &[
    ("HS256", "HMAC using SHA-256"),
    ("HS384", "HMAC using SHA-384"),
    ("HS512", "HMAC using SHA-512"),
    ("RS256", "RSASSA-PKCS1-v1_5 using SHA-256"),
    ("RS384", "RSASSA-PKCS1-v1_5 using SHA-384"),
    ("RS512", "RSASSA-PKCS1-v1_5 using SHA-512"),
    ("ES256", "ECDSA using P-256 and SHA-256"),
    ("ES384", "ECDSA using P-384 and SHA-384"),
    ("ES512", "ECDSA using P-521 and SHA-512"),
    ("PS256", "RSASSA-PSS using SHA-256 and MGF1 with SHA-256"),
    ("PS384", "RSASSA-PSS using SHA-384 and MGF1 with SHA-384"),
    ("PS512", "RSASSA-PSS using SHA-512 and MGF1 with SHA-512"),
    ("none", "No digital signature or MAC performed"),
];

/// it-tools' `CLAIM_DESCRIPTIONS`, the IANA JWT claim registry. The table in
/// the `.vue` prints this next to every claim it recognises.
pub(crate) const CLAIM_DESCRIPTIONS: &[(&str, &str)] = &[
    ("typ", "Type"),
    ("alg", "Algorithm"),
    ("iss", "Issuer"),
    ("sub", "Subject"),
    ("aud", "Audience"),
    ("exp", "Expiration Time"),
    ("nbf", "Not Before"),
    ("iat", "Issued At"),
    ("jti", "JWT ID"),
    ("name", "Full name"),
    ("given_name", "Given name(s) or first name(s)"),
    ("family_name", "Surname(s) or last name(s)"),
    ("middle_name", "Middle name(s)"),
    ("nickname", "Casual name"),
    (
        "preferred_username",
        "Shorthand name by which the End-User wishes to be referred to",
    ),
    ("profile", "Profile page URL"),
    ("picture", "Profile picture URL"),
    ("website", "Web page or blog URL"),
    ("email", "Preferred e-mail address"),
    (
        "email_verified",
        "True if the e-mail address has been verified; otherwise false",
    ),
    ("gender", "Gender"),
    ("birthdate", "Birthday"),
    ("zoneinfo", "Time zone"),
    ("locale", "Locale"),
    ("phone_number", "Preferred telephone number"),
    (
        "phone_number_verified",
        "True if the phone number has been verified; otherwise false",
    ),
    ("address", "Preferred postal address"),
    ("updated_at", "Time the information was last updated"),
    (
        "azp",
        "Authorized party - the party to which the ID Token was issued",
    ),
    (
        "nonce",
        "Value used to associate a Client session with an ID Token",
    ),
    ("auth_time", "Time when the authentication occurred"),
    ("at_hash", "Access Token hash value"),
    ("c_hash", "Code hash value"),
    ("acr", "Authentication Context Class Reference"),
    ("amr", "Authentication Methods References"),
    (
        "sub_jwk",
        "Public key used to check the signature of an ID Token",
    ),
    ("cnf", "Confirmation"),
    ("sip_from_tag", "SIP From tag header field parameter value"),
    ("sip_date", "SIP Date header field value"),
    ("sip_callid", "SIP Call-Id header field value"),
    (
        "sip_cseq_num",
        "SIP CSeq numeric header field parameter value",
    ),
    (
        "sip_via_branch",
        "SIP Via branch header field parameter value",
    ),
    ("orig", "Originating Identity String"),
    ("dest", "Destination Identity String"),
    ("mky", "Media Key Fingerprint String"),
    ("events", "Security Events"),
    ("toe", "Time of Event"),
    ("txn", "Transaction Identifier"),
    ("rph", "Resource Priority Header Authorization"),
    ("sid", "Session ID"),
    ("vot", "Vector of Trust value"),
    ("vtm", "Vector of Trust trustmark URL"),
    ("attest", "Attestation level as defined in SHAKEN framework"),
    (
        "origid",
        "Originating Identifier as defined in SHAKEN framework",
    ),
    ("act", "Actor"),
    ("scope", "Scope Values"),
    ("client_id", "Client Identifier"),
    (
        "may_act",
        "Authorized Actor - the party that is authorized to become the actor",
    ),
    ("jcard", "jCard data"),
    (
        "at_use_nbr",
        "Number of API requests for which the access token can be used",
    ),
    ("div", "Diverted Target of a Call"),
    ("opt", "Original PASSporT (in Full Form)"),
    (
        "vc",
        "Verifiable Credential as specified in the W3C Recommendation",
    ),
    (
        "vp",
        "Verifiable Presentation as specified in the W3C Recommendation",
    ),
    ("sph", "SIP Priority header field"),
    (
        "ace_profile",
        "ACE profile a token is supposed to be used with.",
    ),
    ("cnonce", "Client nonce"),
    ("exi", "Expires in"),
    ("roles", "Roles"),
    ("groups", "Groups"),
    ("entitlements", "Entitlements"),
    ("token_introspection", "Token introspection response"),
];

pub(crate) fn describe(
    table: &'static [(&'static str, &'static str)],
    key: &str,
) -> Option<&'static str> {
    // ponytail: linear scan over ~70 static entries, once per claim.
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// it-tools' `getFriendlyValue`: a readable date for the time claims, and the
/// RFC 7518 name for `alg`.
pub(crate) fn friendly_value(claim: &str, value: &serde_json::Value) -> Option<String> {
    match claim {
        "exp" | "nbf" | "iat" => friendly_date(value),
        "alg" => describe(ALGORITHM_DESCRIPTIONS, value.as_str()?).map(str::to_owned),
        _ => None,
    }
}

/// One `Header` / `Payload` table body. Every claim present is listed, with its
/// human-readable name where the registry has one.
pub(crate) fn jwt_claims(value: &serde_json::Value) -> Result<String> {
    let map = value
        .as_object()
        .context("this JWT segment is not a JSON object")?;
    let mut out = String::new();
    for (claim, value) in map {
        let name = describe(CLAIM_DESCRIPTIONS, claim)
            .map(|d| format!(" ({d})"))
            .unwrap_or_default();
        // Objects and arrays are printed as indented JSON, like the `.vue`.
        let formatted = match value {
            serde_json::Value::String(s) => s.clone(),
            v @ (serde_json::Value::Object(_) | serde_json::Value::Array(_)) => {
                serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
            }
            other => other.to_string(),
        };
        out.push_str(&format!("  {claim}{name}: {formatted}"));
        if let Some(friendly) = friendly_value(claim, value) {
            out.push_str(&format!(" ({friendly})"));
        }
        out.push('\n');
    }
    Ok(out)
}

pub(crate) fn run_jwt(a: &Args) -> Result<String> {
    let jwt = s(a, "jwt", "").trim();
    let parts: Vec<&str> = jwt.split('.').collect();
    // Nothing at all is decodable without two non-empty segments, so this one
    // case is a real Err — the `.vue` likewise hides the table and shows
    // "Invalid JWT". A segment that decodes reports itself even if the other
    // does not.
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!("invalid JWT: expected header.payload.signature");
    }
    Ok(format!(
        "{}\n{}",
        crate::section("Header", jwt_part(parts[0]).and_then(|v| jwt_claims(&v))),
        crate::section("Payload", jwt_part(parts[1]).and_then(|v| jwt_claims(&v)))
    ))
}

pub static TOOL: Tool = Tool {
    id: "jwt-parser",
    name: "JWT parser",
    description: "Decode the header and payload claims of a JSON Web Token.",
    category: Category::Crypto,
    keywords: &["jwt", "token", "json", "web", "decode", "parser", "claims"],
    fields: &[Field::area("jwt", "JWT to decode")],
    kind: Kind::Pure,
    run: run_jwt,
};
