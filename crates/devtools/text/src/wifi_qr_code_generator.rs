//! `wifi-qr-code-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

/// it-tools PRs #1439 and #1326 add the two WPA3 entries. They are options of
/// the *security* select, not a direction toggle.
pub(crate) const WIFI_ENCRYPTIONS: &[&str] =
    &["nopass", "WPA", "WEP", "WPA2-EAP", "WPA3", "WPA2/WPA3"];

/// The `T:` value the WIFI URI scheme wants for each option.
///
/// WPA3-Personal is SAE, which is what Android and iOS look for. A network in
/// WPA2/WPA3 transition mode has to advertise itself as `WPA`: a phone that
/// only knows WPA2 must still be able to join, and one that knows SAE will
/// negotiate it anyway.
pub(crate) fn wifi_type(encryption: &str) -> &str {
    match encryption {
        "WPA3" => "SAE",
        "WPA2/WPA3" => "WPA",
        other => other,
    }
}

/// The EAP method list of `useQRCode.ts`, in its order.
pub(crate) const EAP_METHODS: &[&str] = &[
    "MD5", "POTP", "GTC", "TLS", "IKEv2", "SIM", "AKA", "AKA'", "TTLS", "PWD", "LEAP", "PSK",
    "FAST", "TEAP", "EKE", "NOOB", "PEAP",
];

pub(crate) const EAP_PHASE2_METHODS: &[&str] = &["None", "MSCHAPV2"];

/// `\`, `;`, `,`, `:` and `"` are meta-characters in the WIFI URI scheme.
pub(crate) fn wifi_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | ';' | ',' | ':' | '"') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Port of `getQrCodeText` in `wifi-qr-code-generator/useQRCode.ts`, field order
/// and all — SSID first, then type, password, EAP details, hidden flag. Upstream
/// returns `null` (and renders no QR code at all) for every incomplete
/// combination; those are the `bail!`s here.
// One parameter per form field, in upstream's `GetQrCodeTextOptions` order; a
// struct here would only be the same eight fields with more ceremony.
#[allow(clippy::too_many_arguments)]
pub(crate) fn wifi_payload(
    encryption: &str,
    ssid: &str,
    password: &str,
    hidden: bool,
    eap_method: &str,
    eap_anonymous: bool,
    eap_identity: &str,
    eap_phase2: &str,
) -> anyhow::Result<String> {
    if !WIFI_ENCRYPTIONS.contains(&encryption) {
        anyhow::bail!("unknown encryption `{encryption}`");
    }
    nonempty("SSID", ssid)?;
    let ssid = wifi_escape(ssid);
    // No password means the type can be omitted entirely, which keeps the symbol
    // small — upstream's comment, and its exact output.
    if encryption == "nopass" {
        return Ok(format!("WIFI:S:{ssid};;"));
    }
    nonempty("password", password)?;
    let hidden = if hidden { "H:true" } else { "" };
    let password = wifi_escape(password);
    if encryption != "WPA2-EAP" {
        let t = wifi_type(encryption);
        return Ok(format!("WIFI:S:{ssid};T:{t};P:{password};{hidden};"));
    }
    if !EAP_METHODS.contains(&eap_method) {
        anyhow::bail!("pick an EAP method");
    }
    if eap_identity.trim().is_empty() && !eap_anonymous {
        anyhow::bail!("WPA2-EAP needs an identity, or the anonymous flag");
    }
    if eap_method == "PEAP" && eap_phase2 == "None" {
        anyhow::bail!("the PEAP method needs a phase 2 method");
    }
    let identity = if eap_anonymous {
        "A:anon".to_string()
    } else {
        format!("I:{}", wifi_escape(eap_identity.trim()))
    };
    let phase2 = if eap_phase2 == "None" {
        String::new()
    } else {
        format!("PH2:{eap_phase2};")
    };
    Ok(format!(
        "WIFI:S:{ssid};T:WPA2-EAP;P:{password};E:{eap_method};{phase2}{identity};{hidden};"
    ))
}

pub(crate) fn wifi_qr_code_generator(args: &Args) -> anyhow::Result<String> {
    let id = "wifi-qr-code-generator";
    let payload = wifi_payload(
        a(args, id, "encryption"),
        a(args, id, "ssid"),
        a(args, id, "password"),
        truthy(a(args, id, "hidden")),
        a(args, id, "eap_method"),
        truthy(a(args, id, "eap_anonymous")),
        a(args, id, "eap_identity"),
        a(args, id, "eap_phase2"),
    )?;
    // it-tools PR #1104 makes the level a choice here too; #1479 puts the payload
    // on the page, because the string is what you paste into a router's guest
    // page or a ticket, and re-scanning your own QR code to read it back is silly.
    Ok([
        crate::section("WiFi payload", Ok(payload.clone())),
        crate::section(
            "QR code (SVG)",
            qr_svg(
                &payload,
                a(args, id, "fg_color"),
                a(args, id, "bg_color"),
                a(args, id, "ec_level"),
            ),
        ),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "wifi-qr-code-generator",
    name: "WiFi QR Code generator",
    description: "Generate a QR code for quick connection to a WiFi network.",
    category: Category::Images,
    keywords: &["wifi", "qr", "qrcode", "generator", "network", "ssid"],
    fields: &[
        Field::select("encryption", "Encryption method", WIFI_ENCRYPTIONS, "WPA"),
        Field::text("ssid", "SSID"),
        Field::bool("hidden", "Hidden SSID", "false"),
        // Upstream hides the password unless the encryption needs one, and the
        // three EAP rows unless it is WPA2-EAP; a static field list cannot, so
        // they are documented as ignored instead of silently doing nothing.
        Field::text("password", "Password (ignored for `nopass`)"),
        Field::select(
            "eap_method",
            "EAP method (WPA2-EAP only)",
            EAP_METHODS,
            "TTLS",
        ),
        Field::bool("eap_anonymous", "Anonymous EAP identity", "false"),
        Field::text("eap_identity", "EAP identity (WPA2-EAP only)"),
        Field::select(
            "eap_phase2",
            "EAP phase 2 method (WPA2-EAP only)",
            EAP_PHASE2_METHODS,
            "None",
        ),
        text_default("fg_color", "Foreground color", "#000000"),
        text_default("bg_color", "Background color", "#ffffff"),
        // Medium is what upstream hard-coded, so the default keeps the old symbol.
        Field::select("ec_level", "Error resistance", QR_EC_LEVELS, "medium"),
    ],
    kind: Kind::Pure,
    run: wifi_qr_code_generator,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn wifi_payload_vectors() {
        // Field order, the omitted type for `nopass` and the omitted `H:` when the
        // SSID is not hidden: `getQrCodeText` in useQRCode.ts, verbatim.
        assert_eq!(
            wifi_payload("WPA", "MyNet", "s3cret", false, "", false, "", "None").unwrap(),
            "WIFI:S:MyNet;T:WPA;P:s3cret;;"
        );
        assert_eq!(
            wifi_payload("WPA", "MyNet", "s3cret", true, "", false, "", "None").unwrap(),
            "WIFI:S:MyNet;T:WPA;P:s3cret;H:true;"
        );
        assert_eq!(
            wifi_payload("nopass", "Guest", "", true, "", false, "", "None").unwrap(),
            "WIFI:S:Guest;;"
        );
        // Every meta-character gets a backslash, in SSID and password alike.
        assert_eq!(
            wifi_payload(
                "WEP",
                r#"a;b:c\d,e"f"#,
                r#"p;q:r\s,t"u"#,
                false,
                "",
                false,
                "",
                "None"
            )
            .unwrap(),
            r#"WIFI:S:a\;b\:c\\d\,e\"f;T:WEP;P:p\;q\:r\\s\,t\"u;;"#
        );
        assert_eq!(
            wifi_payload(
                "WPA2-EAP", "Corp", "pw", false, "TTLS", false, "user:1", "MSCHAPV2"
            )
            .unwrap(),
            r"WIFI:S:Corp;T:WPA2-EAP;P:pw;E:TTLS;PH2:MSCHAPV2;I:user\:1;;"
        );
        // A `None` phase 2 drops the PH2 tag; anonymous replaces `I:` with `A:`.
        assert_eq!(
            wifi_payload("WPA2-EAP", "Corp", "pw", true, "TTLS", true, "", "None").unwrap(),
            "WIFI:S:Corp;T:WPA2-EAP;P:pw;E:TTLS;A:anon;H:true;"
        );
        // EAP fields are ignored unless the encryption asks for them.
        assert_eq!(
            wifi_payload("WPA", "Corp", "pw", false, "TTLS", false, "who", "MSCHAPV2").unwrap(),
            "WIFI:S:Corp;T:WPA;P:pw;;"
        );

        // WPA3 is SAE on the wire; the transition mode has to say WPA so a
        // WPA2-only phone can still join. it-tools PRs #1439 / #1326.
        assert_eq!(
            wifi_payload("WPA3", "MyNet", "s3cret", false, "", false, "", "None").unwrap(),
            "WIFI:S:MyNet;T:SAE;P:s3cret;;"
        );
        assert_eq!(
            wifi_payload("WPA2/WPA3", "MyNet", "s3cret", false, "", false, "", "None").unwrap(),
            "WIFI:S:MyNet;T:WPA;P:s3cret;;"
        );

        // Every combination for which upstream renders no QR code at all.
        assert!(wifi_payload("WPA", "", "pw", false, "", false, "", "None").is_err());
        assert!(wifi_payload("WPA4", "net", "pw", false, "", false, "", "None").is_err());
        assert!(wifi_payload("WPA", "net", "", false, "", false, "", "None").is_err());
        assert!(wifi_payload("WPA2-EAP", "net", "pw", false, "", false, "me", "None").is_err());
        assert!(wifi_payload("WPA2-EAP", "net", "pw", false, "TTLS", false, "", "None").is_err());
        assert!(wifi_payload("WPA2-EAP", "net", "pw", false, "PEAP", true, "", "None").is_err());
    }

    #[test]
    fn wifi_qr_code_svg_output() {
        let out = run(
            "wifi-qr-code-generator",
            &[("ssid", "MyNet"), ("password", "s3cret")],
        )
        .unwrap();
        // Two panels now (PR #1479): the payload is copyable on its own, so the
        // output no longer starts with the markup.
        assert!(
            out.starts_with(
                "WiFi payload:\nWIFI:S:MyNet;T:WPA;P:s3cret;;\n\nQR code (SVG):\n<svg "
            ),
            "{out:.120}"
        );
        assert!(out.ends_with("</svg>"));
        // The level is a field here too (PR #1104), and a bad one fails the QR
        // panel without taking the payload down with it.
        let bad = run(
            "wifi-qr-code-generator",
            &[("ssid", "n"), ("password", "p"), ("ec_level", "nope")],
        )
        .unwrap();
        assert!(bad.contains("WiFi payload:\nWIFI:S:n;T:WPA;P:p;;"), "{bad}");
        assert!(bad.contains("unknown error correction level"), "{bad}");
        assert!(
            run(
                "wifi-qr-code-generator",
                &[("ssid", "n"), ("password", "p"), ("ec_level", "high")]
            )
            .unwrap()
            .contains("<svg ")
        );

        assert!(run("wifi-qr-code-generator", &[]).is_err()); // no SSID
        // A password past the QR capacity keeps the payload panel; only the
        // symbol is impossible, and saying so beats blanking the page.
        let huge = run(
            "wifi-qr-code-generator",
            &[("ssid", "n"), ("password", &"p".repeat(5000))],
        )
        .unwrap();
        assert!(huge.contains("cannot encode QR code"), "{huge:.200}");
        assert!(
            huge.starts_with("WiFi payload:\nWIFI:S:n;T:WPA;P:ppp"),
            "{huge:.60}"
        );
    }
}
