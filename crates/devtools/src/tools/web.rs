//! Web & network tools. Owned by the web batch.
//!
//! Ported from it-tools (`src/tools/<name>/`). Every tool is `fields in ->
//! text out`; output is a plain `Label: value` block because the UI renders it
//! verbatim.
use crate::{Args, Category, Field, Kind, Tool, arg, section};
use anyhow::{Context, Result, bail};
use std::fmt::Write as _;
use std::net::Ipv4Addr;

/// Look a tool up by id so `run` bodies stop depending on the registry order.
fn tool(id: &str) -> &'static Tool {
    TOOLS
        .iter()
        .find(|t| t.id == id)
        .expect("run fns only ask for their own tool")
}

// ---------------------------------------------------------------- url-encoder

/// JS `encodeURIComponent` leaves `-_.!~*'()` alone on top of alphanumerics.
const URI_COMPONENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

/// `decodeURIComponent` semantics: `percent_decode_str` is lenient about `%zz`,
/// JS throws.
fn decode_uri_component(input: &str) -> Result<String> {
    let bytes = input.as_bytes();
    for (i, _) in input.char_indices().filter(|(_, c)| *c == '%') {
        let ok = bytes
            .get(i + 1..i + 3)
            .is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit));
        if !ok {
            bail!("Impossible to parse this string");
        }
    }
    percent_encoding::percent_decode_str(input)
        .decode_utf8()
        .map(|s| s.into_owned())
        .context("Impossible to parse this string")
}

/// it-tools shows an "Encode" and a "Decode" card side by side — no mode toggle.
fn url_encoder(args: &Args) -> Result<String> {
    let t = tool("url-encoder");
    let encoded =
        percent_encoding::utf8_percent_encode(arg(args, t, "encode_input"), URI_COMPONENT)
            .to_string();
    Ok(format!(
        "{}\n{}\n",
        section("Encoded", Ok(encoded)),
        section(
            "Decoded",
            decode_uri_component(arg(args, t, "decode_input"))
        ),
    ))
}

// ----------------------------------------------------------------- url-parser

struct Url {
    protocol: String,
    username: String,
    password: String,
    hostname: String,
    port: String,
    path: String,
    query: String,
    fragment: String,
}

/// ponytail: hand-rolled instead of the `url` crate — not in the dep list and
/// the scheme://user:pass@host:port/path?q#frag split is 30 lines. Upgrade to
/// `url` if anyone needs IDNA or full WHATWG normalisation.
fn parse_url(raw: &str) -> Result<Url> {
    let s = raw.trim();
    if s.is_empty() {
        bail!("empty url");
    }
    let (scheme, rest) = s.split_once(':').context("missing scheme, e.g. https://")?;
    if scheme.is_empty()
        || !scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    {
        bail!("invalid scheme {scheme:?}");
    }
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let (rest, fragment) = match rest.split_once('#') {
        Some((a, b)) => (a, b.to_string()),
        None => (rest, String::new()),
    };
    let (rest, query) = match rest.split_once('?') {
        Some((a, b)) => (a, format!("?{b}")),
        None => (rest, String::new()),
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    let (userinfo, host) = match authority.rsplit_once('@') {
        Some((u, h)) => (u, h),
        None => ("", authority),
    };
    let (username, password) = match userinfo.split_once(':') {
        Some((u, p)) => (u.to_string(), p.to_string()),
        None => (userinfo.to_string(), String::new()),
    };
    // IPv6 literals keep their brackets and hide the colons.
    let (hostname, port) = if let Some(end) = host.find(']') {
        (
            host[..=end].to_string(),
            host[end + 1..].trim_start_matches(':').to_string(),
        )
    } else {
        match host.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.to_string()),
            None => (host.to_string(), String::new()),
        }
    };
    if hostname.is_empty() {
        bail!("missing host");
    }
    if !port.is_empty() && port.parse::<u16>().is_err() {
        bail!("invalid port {port:?}");
    }
    Ok(Url {
        protocol: format!("{scheme}:"),
        username,
        password,
        hostname,
        port,
        path,
        query,
        fragment,
    })
}

fn decode_form_component(s: &str) -> String {
    let plus_free = s.replace('+', " ");
    percent_encoding::percent_decode_str(&plus_free)
        .decode_utf8_lossy()
        .into_owned()
}

fn url_parser(args: &Args) -> Result<String> {
    let u = parse_url(arg(args, tool("url-parser"), "url"))?;
    let mut out = String::new();
    let _ = writeln!(out, "Protocol: {}", u.protocol);
    let _ = writeln!(out, "Username: {}", u.username);
    let _ = writeln!(out, "Password: {}", u.password);
    let _ = writeln!(out, "Hostname: {}", u.hostname);
    let _ = writeln!(out, "Port: {}", u.port);
    let _ = writeln!(out, "Path: {}", u.path);
    let _ = writeln!(out, "Params: {}", u.query);
    let _ = writeln!(out, "Hash: {}", u.fragment);
    let pairs = u.query.trim_start_matches('?');
    if !pairs.is_empty() {
        let _ = writeln!(out, "\nQuery parameters:");
        for pair in pairs.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let _ = writeln!(
                out,
                "  {} = {}",
                decode_form_component(k),
                decode_form_component(v)
            );
        }
    }
    Ok(out)
}

// -------------------------------------------------------------- html-entities

/// it-tools shows an "Escape" and an "Unescape" card at once — no mode toggle.
fn html_entities(args: &Args) -> Result<String> {
    let t = tool("html-entities");
    let escaped = html_escape::encode_quoted_attribute(arg(args, t, "escape_input")).into_owned();
    let unescaped = html_escape::decode_html_entities(arg(args, t, "unescape_input")).into_owned();
    Ok(format!(
        "{}\n{}\n",
        section("Escaped", Ok(escaped)),
        section("Unescaped", Ok(unescaped)),
    ))
}

// ------------------------------------------------------------ safelink-decoder

fn safelink_decoder(args: &Args) -> Result<String> {
    let input = arg(args, tool("safelink-decoder"), "url").trim();
    if !input.contains(".safelinks.protection.outlook.com") {
        bail!("Invalid SafeLinks URL provided");
    }
    let query = input
        .split_once('?')
        .context("Invalid SafeLinks URL provided")?
        .1;
    // `&amp;` shows up when the link was copied out of rendered HTML.
    let target = query
        .split(['&', ';'])
        .map(|p| p.trim_start_matches("amp;"))
        .find_map(|p| p.strip_prefix("url="))
        .context("no `url` parameter in SafeLinks URL")?;
    Ok(percent_encoding::percent_decode_str(target)
        .decode_utf8()
        .context("`url` parameter is not valid percent-encoded UTF-8")?
        .into_owned())
}

// ----------------------------------------------------------- user-agent-parser

/// Version digits following `token` in the user agent, e.g. `WebKit/537.36`.
fn ua_version_after(ua: &str, token: &str) -> Option<String> {
    let i = ua.find(token)? + token.len();
    let v: String = ua[i..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    (!v.is_empty()).then_some(v)
}

/// ponytail: woothee reports no engine and no CPU, but it-tools' ua-parser-js
/// does and shows both as rows, so read the usual tokens straight off the
/// string. Upgrade path: a real ua-parser-js port, if the rows ever need to be
/// exact for exotic clients.
fn ua_engine(ua: &str) -> Option<(&'static str, Option<String>)> {
    if ua.contains("Chrome/") || ua.contains("Chromium/") || ua.contains("Edg/") {
        return Some(("Blink", ua_version_after(ua, "Chrome/")));
    }
    for (token, name) in [
        ("Trident/", "Trident"),
        ("Presto/", "Presto"),
        ("EdgeHTML/", "EdgeHTML"),
    ] {
        if ua.contains(token) {
            return Some((name, ua_version_after(ua, token)));
        }
    }
    if ua.contains("Gecko/") && ua.contains("rv:") {
        return Some(("Gecko", ua_version_after(ua, "rv:")));
    }
    if ua.contains("AppleWebKit/") {
        return Some(("WebKit", ua_version_after(ua, "AppleWebKit/")));
    }
    None
}

fn ua_cpu(ua: &str) -> Option<&'static str> {
    let u = ua.to_lowercase();
    [
        ("x86_64", "amd64"),
        ("win64", "amd64"),
        ("wow64", "amd64"),
        ("amd64", "amd64"),
        ("x64", "amd64"),
        ("aarch64", "arm64"),
        ("arm64", "arm64"),
        ("armv", "arm"),
        ("i686", "ia32"),
        ("i386", "ia32"),
        ("ppc64", "ppc64"),
    ]
    .into_iter()
    .find(|(needle, _)| u.contains(needle))
    .map(|(_, arch)| arch)
}

/// woothee spells "no idea" several ways; it-tools shows a per-row fallback.
fn ua_row(label: &str, value: Option<&str>, fallback: &str) -> String {
    let v = match value.map(str::trim) {
        Some(s) if !s.is_empty() && s != "UNKNOWN" => s,
        _ => fallback,
    };
    format!("  {label}: {v}\n")
}

fn user_agent_parser(args: &Args) -> Result<String> {
    let ua = arg(args, tool("user-agent-parser"), "user_agent").trim();
    if ua.is_empty() {
        bail!("empty user agent");
    }
    // An unparseable UA still yields engine and CPU rows, so no `?` here.
    let parsed = woothee::parser::Parser::new().parse(ua);
    let p = parsed.as_ref();
    let engine = ua_engine(ua);
    let mut out = String::new();
    out.push_str(&section(
        "Browser",
        Ok(format!(
            "{}{}",
            ua_row("Name", p.map(|r| r.name), "No browser name available"),
            ua_row(
                "Version",
                p.map(|r| r.version),
                "No browser version available"
            ),
        )),
    ));
    out.push_str(&section(
        "Engine",
        Ok(format!(
            "{}{}",
            ua_row(
                "Name",
                engine.as_ref().map(|(n, _)| *n),
                "No engine name available"
            ),
            ua_row(
                "Version",
                engine.as_ref().and_then(|(_, v)| v.as_deref()),
                "No engine version available"
            ),
        )),
    ));
    out.push_str(&section(
        "OS",
        Ok(format!(
            "{}{}",
            ua_row("Name", p.map(|r| r.os), "No OS name available"),
            ua_row(
                "Version",
                p.map(|r| r.os_version.as_ref()),
                "No OS version available"
            ),
        )),
    ));
    out.push_str(&section(
        "Device",
        Ok(format!(
            "{}{}{}",
            // woothee has no device model; `category` is its device type.
            ua_row("Model", None, "No device model available"),
            ua_row("Type", p.map(|r| r.category), "No device type available"),
            ua_row("Vendor", p.map(|r| r.vendor), "No device vendor available"),
        )),
    ));
    out.push_str(&section(
        "CPU",
        Ok(ua_row(
            "Architecture",
            ua_cpu(ua),
            "No CPU architecture available",
        )),
    ));
    Ok(out)
}

// ----------------------------------------------------------------- mime-types

fn mime_to_extensions(mime: &str) -> Result<String> {
    if mime.is_empty() {
        bail!("nothing to look up");
    }
    let exts = mime_guess::get_mime_extensions_str(mime).context("unknown mime type")?;
    let list = exts
        .iter()
        .map(|e| format!(".{e}"))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(format!("  Extensions of {mime} files: {list}\n"))
}

fn extension_to_mime(ext: &str) -> Result<String> {
    if ext.is_empty() {
        bail!("nothing to look up");
    }
    let m = mime_guess::from_ext(ext)
        .first()
        .context("unknown extension")?;
    Ok(format!("  Mime type of .{ext} files: {m}\n"))
}

/// Mime type for a bare file extension (no leading dot), or `None` if
/// `mime_guess` does not know it.
///
/// Thin accessor over the same table the `mime-types` tool formats. The app's
/// base64-file pane consumes it so it does not need its own mime list.
pub fn mime_for_extension(ext: &str) -> Option<&'static str> {
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    // `.ts` is ambiguous and `mime_guess` resolves it to the MPEG transport
    // stream. In a developer tool it is TypeScript essentially always, so this
    // one deliberately overrides the table rather than falling back to it.
    if ext == "ts" {
        return Some("text/typescript");
    }
    mime_guess::from_ext(&ext)
        .first_raw()
        .or(match ext.as_str() {
            // Extensions the table has no entry for at all. Without these they
            // would be labelled opaque binary, which mislabels plain text.
            "lock" | "bru" | "http" | "tsx" | "cjs" => Some("text/plain"),
            "jsonc" => Some("application/json"),
            _ => None,
        })
}

/// Extensions (no leading dot) known for a mime type, most canonical first.
///
/// Same table, other direction. The app's base64-file pane uses it to suggest a
/// file name when saving a decoded `data:` URI.
pub fn extensions_for_mime(mime: &str) -> Vec<&'static str> {
    mime_guess::get_mime_extensions_str(mime)
        .unwrap_or(&[])
        .to_vec()
}

/// it-tools has one card per direction, both live at once — no mode toggle.
fn mime_types(args: &Args) -> Result<String> {
    let t = tool("mime-types");
    Ok(format!(
        "{}{}",
        section(
            "Mime type to extension",
            mime_to_extensions(arg(args, t, "mime_type").trim())
        ),
        section(
            "File extension to mime type",
            extension_to_mime(arg(args, t, "extension").trim().trim_start_matches('.'))
        ),
    ))
}

// ----------------------------------------------------------- http-status-codes

/// `(code, name, description, kind)` — ported verbatim from
/// `http-status-codes.constants.ts`.
static STATUS_CODES: &[(u16, &str, &str, &str)] = &[
    (
        100,
        "Continue",
        "Waiting for the client to emit the body of the request.",
        "HTTP",
    ),
    (
        101,
        "Switching Protocols",
        "The server has agreed to change protocol.",
        "HTTP",
    ),
    (
        102,
        "Processing",
        "The server is processing the request, but no response is available yet.",
        "WebDav",
    ),
    (
        103,
        "Early Hints",
        "The server returns some response headers before final HTTP message.",
        "HTTP",
    ),
    (
        200,
        "OK",
        "Standard response for successful HTTP requests.",
        "HTTP",
    ),
    (
        201,
        "Created",
        "The request has been fulfilled, resulting in the creation of a new resource.",
        "HTTP",
    ),
    (
        202,
        "Accepted",
        "The request has been accepted for processing, but the processing has not been completed.",
        "HTTP",
    ),
    (
        203,
        "Non-Authoritative Information",
        "The request has been successfully processed, but is returning information that may come from another source.",
        "HTTP",
    ),
    (
        204,
        "No Content",
        "The request has been successfully processed, but is not returning any content.",
        "HTTP",
    ),
    (
        205,
        "Reset Content",
        "The request has been successfully processed, but is not returning any content, and requires that the requester reset the document view.",
        "HTTP",
    ),
    (
        206,
        "Partial Content",
        "The server is delivering only part of the resource due to a range header sent by the client.",
        "HTTP",
    ),
    (
        207,
        "Multi-Status",
        "The message body that follows is an XML message and can contain a number of separate response codes.",
        "WebDav",
    ),
    (
        208,
        "Already Reported",
        "The members of a DAV binding have already been enumerated in a preceding part of the (multistatus) response.",
        "WebDav",
    ),
    (
        226,
        "IM Used",
        "The server has fulfilled a request for the resource, and the response is a representation of the result of one or more instance-manipulations applied to the current instance.",
        "HTTP",
    ),
    (
        300,
        "Multiple Choices",
        "The request has more than one possible response.",
        "HTTP",
    ),
    (
        301,
        "Moved Permanently",
        "The URL of the requested resource has been changed permanently.",
        "HTTP",
    ),
    (
        302,
        "Found",
        "The URI of requested resource has been changed temporarily.",
        "HTTP",
    ),
    (
        303,
        "See Other",
        "The response to the request can be found under another URI using a GET method.",
        "HTTP",
    ),
    (
        304,
        "Not Modified",
        "The resource has not been modified since the version specified by the request headers.",
        "HTTP",
    ),
    (
        305,
        "Use Proxy",
        "The requested resource is available only through a proxy, the address for which is provided in the response.",
        "HTTP",
    ),
    (
        306,
        "Switch Proxy",
        "No longer used. Originally meant \"Subsequent requests should use the specified proxy.\"",
        "HTTP",
    ),
    (
        307,
        "Temporary Redirect",
        "The request should be repeated with another URI, but future requests should still use the original URI.",
        "HTTP",
    ),
    (
        308,
        "Permanent Redirect",
        "The request and all future requests should be repeated using another URI.",
        "HTTP",
    ),
    (
        400,
        "Bad Request",
        "The server cannot or will not process the request due to an apparent client error.",
        "HTTP",
    ),
    (
        401,
        "Unauthorized",
        "Authentication is required and has failed or has not yet been provided.",
        "HTTP",
    ),
    (
        402,
        "Payment Required",
        "Payment is required to access the requested resource.",
        "HTTP",
    ),
    (
        403,
        "Forbidden",
        "The client does not have the necessary permissions to access the resource.",
        "HTTP",
    ),
    (
        404,
        "Not Found",
        "The requested resource could not be found.",
        "HTTP",
    ),
    (
        405,
        "Method Not Allowed",
        "The request method is not supported by the requested resource.",
        "HTTP",
    ),
    (
        406,
        "Not Acceptable",
        "The requested resource is capable of generating only content not acceptable according to the Accept headers sent in the request.",
        "HTTP",
    ),
    (
        407,
        "Proxy Authentication Required",
        "The client must first authenticate itself with the proxy.",
        "HTTP",
    ),
    (
        408,
        "Request Timeout",
        "The server timed out waiting for the request.",
        "HTTP",
    ),
    (
        409,
        "Conflict",
        "The request could not be completed due to a conflict with the current state of the target resource.",
        "HTTP",
    ),
    (
        410,
        "Gone",
        "The requested resource is no longer available and will not be available again.",
        "HTTP",
    ),
    (
        411,
        "Length Required",
        "The request did not specify the length of its content, which is required by the requested resource.",
        "HTTP",
    ),
    (
        412,
        "Precondition Failed",
        "The server does not meet one of the preconditions that the requester put on the request.",
        "HTTP",
    ),
    (
        413,
        "Payload Too Large",
        "The request is larger than the server is willing or able to process.",
        "HTTP",
    ),
    (
        414,
        "URI Too Long",
        "The URI provided was too long for the server to process.",
        "HTTP",
    ),
    (
        415,
        "Unsupported Media Type",
        "The request entity has a media type which the server or resource does not support.",
        "HTTP",
    ),
    (
        416,
        "Range Not Satisfiable",
        "The client has asked for a portion of the file, but the server cannot supply that portion.",
        "HTTP",
    ),
    (
        417,
        "Expectation Failed",
        "The server cannot meet the requirements of the Expect request-header field.",
        "HTTP",
    ),
    (
        418,
        "I'm a teapot",
        "The server refuses the attempt to brew coffee with a teapot.",
        "HTTP",
    ),
    (
        421,
        "Misdirected Request",
        "The request was directed at a server that is not able to produce a response.",
        "HTTP",
    ),
    (
        422,
        "Unprocessable Entity",
        "The request was well-formed but was unable to be followed due to semantic errors.",
        "WebDav",
    ),
    (
        423,
        "Locked",
        "The resource that is being accessed is locked.",
        "WebDav",
    ),
    (
        424,
        "Failed Dependency",
        "The request failed because it depended on another request that failed.",
        "WebDav",
    ),
    (
        425,
        "Too Early",
        "The server is unwilling to risk processing a request that might be replayed.",
        "HTTP",
    ),
    (
        426,
        "Upgrade Required",
        "The client should switch to a different protocol.",
        "HTTP",
    ),
    (
        428,
        "Precondition Required",
        "The origin server requires the request to be conditional.",
        "HTTP",
    ),
    (
        429,
        "Too Many Requests",
        "The user has sent too many requests in a given amount of time.",
        "HTTP",
    ),
    (
        431,
        "Request Header Fields Too Large",
        "The server is unwilling to process the request because its header fields are too large.",
        "HTTP",
    ),
    (
        451,
        "Unavailable For Legal Reasons",
        "The server is denying access to the resource as a consequence of a legal demand.",
        "HTTP",
    ),
    (
        500,
        "Internal Server Error",
        "An unexpected condition was encountered by the server.",
        "HTTP",
    ),
    (
        501,
        "Not Implemented",
        "The server does not support the functionality required to fulfill the request.",
        "HTTP",
    ),
    (
        502,
        "Bad Gateway",
        "The server received an invalid response from the upstream server.",
        "HTTP",
    ),
    (
        503,
        "Service Unavailable",
        "The server is currently unable to handle the request.",
        "HTTP",
    ),
    (
        504,
        "Gateway Timeout",
        "The upstream server failed to send a request in the time allowed by the server.",
        "HTTP",
    ),
    (
        505,
        "HTTP Version Not Supported",
        "The HTTP version used in the request is not supported by the server.",
        "HTTP",
    ),
    (
        506,
        "Variant Also Negotiates",
        "Transparent content negotiation for the request results in a circular reference.",
        "HTTP",
    ),
    (
        507,
        "Insufficient Storage",
        "The server is unable to store the representation needed to complete the request.",
        "WebDav",
    ),
    (
        508,
        "Loop Detected",
        "The server detected an infinite loop while processing the request.",
        "HTTP",
    ),
    (
        510,
        "Not Extended",
        "Further extensions to the request are required for the server to fulfill it.",
        "HTTP",
    ),
    (
        511,
        "Network Authentication Required",
        "The client needs to authenticate to gain network access.",
        "HTTP",
    ),
];

/// it-tools' `codesByCategories` headings.
fn status_category(code: u16) -> &'static str {
    match code / 100 {
        1 => "1xx informational response",
        2 => "2xx success",
        3 => "3xx redirection",
        4 => "4xx client error",
        _ => "5xx server error",
    }
}

fn http_status_codes(args: &Args) -> Result<String> {
    let q = arg(args, tool("http-status-codes"), "query")
        .trim()
        .to_lowercase();
    let hits: Vec<_> = STATUS_CODES
        .iter()
        .filter(|(code, name, desc, _)| {
            q.is_empty()
                || code.to_string().starts_with(&q)
                || name.to_lowercase().contains(&q)
                || desc.to_lowercase().contains(&q)
        })
        .collect();
    // it-tools shows an empty "Search results" group rather than an error.
    if hits.is_empty() {
        return Ok(format!("Search results\n  no status code matches {q:?}\n"));
    }
    let mut out = String::new();
    let mut heading = "";
    for (code, name, desc, kind) in hits {
        // Unfiltered, it-tools groups by category; filtered, everything lands
        // under one "Search results" heading.
        let want = if q.is_empty() {
            status_category(*code)
        } else {
            "Search results"
        };
        if want != heading {
            heading = want;
            let _ = writeln!(out, "{heading}");
        }
        let suffix = if *kind == "HTTP" {
            String::new()
        } else {
            format!(" For {kind}.")
        };
        let _ = writeln!(out, "  {code} {name}\n    {desc}{suffix}");
    }
    Ok(out)
}

// ---------------------------------------------------------- meta-tag-generator

fn meta_tag_generator(args: &Args) -> Result<String> {
    let t = tool("meta-tag-generator");
    let get = |k: &str| arg(args, t, k).trim().to_string();
    let title = get("title");
    let description = get("description");
    let mut out = String::new();
    if !title.is_empty() {
        let _ = writeln!(
            out,
            "<title>{}</title>",
            html_escape::encode_quoted_attribute(&title)
        );
    }
    if !description.is_empty() {
        let _ = writeln!(
            out,
            "<meta name=\"description\" content=\"{}\" />",
            html_escape::encode_quoted_attribute(&description)
        );
    }
    let og = [
        ("og:type", get("type")),
        ("og:title", title.clone()),
        ("og:description", description.clone()),
        ("og:url", get("url")),
        ("og:image", get("image")),
        ("og:image:alt", get("image_alt")),
        ("og:image:width", get("image_width")),
        ("og:image:height", get("image_height")),
    ];
    for (prop, value) in og.iter().filter(|(_, v)| !v.is_empty()) {
        let _ = writeln!(
            out,
            "<meta property=\"{prop}\" content=\"{}\" />",
            html_escape::encode_quoted_attribute(value)
        );
    }
    let tw = [
        ("twitter:card", get("twitter_card")),
        ("twitter:site", get("twitter_site")),
        ("twitter:creator", get("twitter_creator")),
        ("twitter:title", title),
        ("twitter:description", description),
        ("twitter:image", get("image")),
    ];
    for (name, value) in tw.iter().filter(|(_, v)| !v.is_empty()) {
        let _ = writeln!(
            out,
            "<meta name=\"{name}\" content=\"{}\" />",
            html_escape::encode_quoted_attribute(value)
        );
    }
    if out.is_empty() {
        bail!("nothing to generate — fill in at least a title");
    }
    Ok(out)
}

// ----------------------------------------------------- ipv4-address-converter

fn parse_ipv4(raw: &str) -> Result<Ipv4Addr> {
    raw.trim()
        .parse::<Ipv4Addr>()
        .with_context(|| format!("invalid ipv4 address {:?}", raw.trim()))
}

fn ipv4_to_ipv6(ip: Ipv4Addr, prefix: &str) -> String {
    let o = ip.octets();
    format!("{prefix}{:02x}{:02x}:{:02x}{:02x}", o[0], o[1], o[2], o[3])
}

fn ipv4_address_converter(args: &Args) -> Result<String> {
    let ip = parse_ipv4(arg(args, tool("ipv4-address-converter"), "ip"))?;
    let n = u32::from(ip);
    Ok(format!(
        "Decimal: {n}\nHexadecimal: {n:X}\nBinary: {n:b}\nIpv6: {}\nIpv6 (short): {}\n",
        ipv4_to_ipv6(ip, "0000:0000:0000:0000:0000:ffff:"),
        ipv4_to_ipv6(ip, "::ffff:"),
    ))
}

// ------------------------------------------------------- ipv4-range-expander

struct RangeExpansion {
    old_size: u64,
    new_start: Ipv4Addr,
    new_end: Ipv4Addr,
    new_cidr: String,
    new_size: u64,
}

/// Port of `calculateCidr` — grow the range to the smallest enclosing CIDR by
/// finding the first differing bit of start and end.
fn calculate_cidr(start_ip: Ipv4Addr, end_ip: Ipv4Addr) -> Result<RangeExpansion> {
    let start = u32::from(start_ip);
    let end = u32::from(end_ip);
    if end < start {
        bail!("end address must not be lower than the start address");
    }
    let mask = (0..32u32)
        .find(|i| (start >> (31 - i)) & 1 != (end >> (31 - i)) & 1)
        .unwrap_or(32);
    let host_bits = 32 - mask;
    let (new_start, new_end) = if host_bits == 32 {
        (0u32, u32::MAX)
    } else {
        let lo = start & (!0u32 << host_bits);
        (lo, lo | ((1u32 << host_bits) - 1))
    };
    Ok(RangeExpansion {
        old_size: u64::from(end - start) + 1,
        new_start: Ipv4Addr::from(new_start),
        new_end: Ipv4Addr::from(new_end),
        new_cidr: format!("{}/{mask}", Ipv4Addr::from(new_start)),
        new_size: u64::from(new_end - new_start) + 1,
    })
}

fn ipv4_range_expander(args: &Args) -> Result<String> {
    let t = tool("ipv4-range-expander");
    let (start, end) = (
        parse_ipv4(arg(args, t, "start_ip"))?,
        parse_ipv4(arg(args, t, "end_ip"))?,
    );
    let r = calculate_cidr(start, end)?;
    // it-tools renders an old value / new value table with exactly these rows.
    let mut out = String::from("                     old value  ->  new value\n");
    for (label, old, new) in [
        ("Start address", start.to_string(), r.new_start.to_string()),
        ("End address", end.to_string(), r.new_end.to_string()),
        (
            "Addresses in range",
            r.old_size.to_string(),
            r.new_size.to_string(),
        ),
        ("CIDR", String::new(), r.new_cidr.clone()),
    ] {
        let _ = writeln!(out, "{label:<18}  {old:>11}  ->  {new}");
    }
    Ok(out)
}

// ---------------------------------------------------- ipv4-subnet-calculator

struct Subnet {
    net: Ipv4Net,
    size: u64,
    first: Ipv4Addr,
    last: Ipv4Addr,
    broadcast: Option<Ipv4Addr>,
}

use ipnet::Ipv4Net;

/// Accepts `1.2.3.4`, `1.2.3.4/24` and `1.2.3.4/255.255.255.0`, like
/// netmask.js does. No mask means /32.
fn parse_subnet(raw: &str) -> Result<Subnet> {
    let s = raw.trim();
    let (ip_part, mask_part) = match s.split_once('/') {
        Some((a, b)) => (a, b.trim()),
        None => (s, "32"),
    };
    let ip = parse_ipv4(ip_part)?;
    let prefix = if mask_part.contains('.') {
        let mask = u32::from(parse_ipv4(mask_part)?);
        if mask.leading_ones() + mask.trailing_zeros() != 32 {
            bail!("{mask_part} is not a contiguous netmask");
        }
        mask.leading_ones() as u8
    } else {
        mask_part
            .parse::<u8>()
            .with_context(|| format!("invalid prefix length {mask_part:?}"))?
    };
    let net = Ipv4Net::new(ip, prefix).context("prefix length must be 0..=32")?;
    let base = u32::from(net.network());
    let size = 1u64 << (32 - prefix);
    // netmask.js: /31 and /32 have no broadcast and no host reservation.
    let (first, last, broadcast) = if prefix <= 30 {
        (
            base + 1,
            base + size as u32 - 2,
            Some(Ipv4Addr::from(base + size as u32 - 1)),
        )
    } else {
        (base, base + (size as u32 - 1), None)
    };
    Ok(Subnet {
        net,
        size,
        first: Ipv4Addr::from(first),
        last: Ipv4Addr::from(last),
        broadcast,
    })
}

fn ip_class(ip: Ipv4Addr) -> &'static str {
    match ip.octets()[0] {
        0..=127 => "A",
        128..=191 => "B",
        192..=223 => "C",
        224..=239 => "D",
        _ => "E",
    }
}

fn ipv4_subnet_calculator(args: &Args) -> Result<String> {
    let s = parse_subnet(arg(args, tool("ipv4-subnet-calculator"), "ip"))?;
    let bits = s.net.prefix_len() as usize;
    let binary: String = ("1".repeat(bits) + &"0".repeat(32 - bits))
        .as_bytes()
        .chunks(8)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join(".");
    let mut out = String::new();
    let _ = writeln!(out, "Netmask: {}/{}", s.net.network(), bits);
    let _ = writeln!(out, "Network address: {}", s.net.network());
    let _ = writeln!(out, "Network mask: {}", s.net.netmask());
    let _ = writeln!(out, "Network mask in binary: {binary}");
    let _ = writeln!(out, "CIDR notation: /{bits}");
    let _ = writeln!(out, "Wildcard mask: {}", s.net.hostmask());
    let _ = writeln!(out, "Network size: {}", s.size);
    let _ = writeln!(out, "First address: {}", s.first);
    let _ = writeln!(out, "Last address: {}", s.last);
    let _ = writeln!(
        out,
        "Broadcast address: {}",
        s.broadcast
            .map(|b| b.to_string())
            .unwrap_or_else(|| "No broadcast address with this mask".into())
    );
    let _ = writeln!(out, "IP class: {}", ip_class(s.net.network()));
    Ok(out)
}

// -------------------------------------------------------- ipv6-ula-generator

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn parse_mac(raw: &str) -> Result<macaddr::MacAddr6> {
    raw.trim()
        .parse::<macaddr::MacAddr6>()
        .with_context(|| format!("invalid MAC address {:?}", raw.trim()))
}

fn ipv6_ula_generator(args: &Args) -> Result<String> {
    let mac = parse_mac(arg(args, tool("ipv6-ula-generator"), "mac"))?;
    // RFC 4193 method 1: sha1(timestamp + EUI-64/MAC), keep the low 40 bits.
    use sha1::Digest as _;
    let seed = format!("{}{}", chrono::Utc::now().timestamp_millis(), mac);
    let digest = hex(&sha1::Sha1::digest(seed.as_bytes()));
    let h = &digest[30..];
    let ula = format!("fd{}:{}:{}", &h[0..2], &h[2..6], &h[6..10]);
    Ok(format!(
        "IPv6 ULA: {ula}::/48\nFirst routable block: {ula}:0::/64\nLast routable block: {ula}:ffff::/64\n"
    ))
}

// ------------------------------------------------------ mac-address-generator

/// Port of `splitPrefix`: split on non-hex separators, or group by two when the
/// prefix is bare hex. Odd trailing nibbles get zero-padded on the left.
fn split_prefix(prefix: &str) -> Vec<String> {
    let pad = |s: &str| format!("{s:0>2}");
    if prefix.chars().all(|c| c.is_ascii_hexdigit()) {
        prefix
            .chars()
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|c| pad(&c.iter().collect::<String>()))
            .collect()
    } else {
        prefix
            .split(|c: char| !c.is_ascii_hexdigit())
            .filter(|s| !s.is_empty())
            .map(pad)
            .collect()
    }
}

fn mac_address_generator(args: &Args) -> Result<String> {
    let t = tool("mac-address-generator");
    let prefix = split_prefix(arg(args, t, "prefix").trim());
    if prefix.len() > 6 {
        bail!("prefix is longer than a MAC address");
    }
    let separator = arg(args, t, "separator");
    let uppercase = arg(args, t, "uppercase") == "true";
    let count: usize = arg(args, t, "quantity").trim().parse().unwrap_or(1);
    // it-tools caps the quantity input at 100.
    if !(1..=100).contains(&count) {
        bail!("quantity must be between 1 and 100");
    }
    let mut out = String::new();
    for _ in 0..count {
        let mut bytes = prefix.clone();
        while bytes.len() < 6 {
            bytes.push(format!("{:02x}", rand::random_range(0..=255u8)));
        }
        let mac = bytes.join(separator);
        let _ = writeln!(out, "{}", if uppercase { mac.to_uppercase() } else { mac });
    }
    Ok(out)
}

// --------------------------------------------------------- mac-address-lookup

// ponytail: a curated subset of the IEEE OUI registry — the real list is ~35k
// entries / several MB and has no business being compiled into the binary.
// Upgrade path if anyone actually needs full coverage: ship the IEEE `oui.csv`
// as a data file next to the app and load it lazily instead of growing this.
static OUI: &[(&str, &str)] = &[
    ("000000", "Xerox Corporation"),
    ("00000C", "Cisco Systems, Inc"),
    ("00005E", "IANA"),
    ("000142", "Cisco Systems, Inc"),
    ("00040B", "Cisco Systems, Inc"),
    ("00044B", "NVIDIA Corporation"),
    ("000255", "IBM Corporation"),
    ("000393", "Apple, Inc."),
    ("0003FF", "Microsoft Corporation"),
    ("000502", "Apple, Inc."),
    ("000569", "VMware, Inc."),
    ("0005C9", "LG Electronics"),
    ("000902", "Redline Communications"),
    ("00090F", "Fortinet Inc."),
    ("0009BF", "Nintendo Co., Ltd."),
    ("000A27", "Apple, Inc."),
    ("000AF7", "Broadcom Corporation"),
    ("000C29", "VMware, Inc."),
    ("000C41", "Cisco-Linksys, LLC"),
    ("000C42", "Routerboard.com (MikroTik)"),
    ("000D3A", "Microsoft Corporation"),
    ("000E58", "Sonos, Inc."),
    ("000E7F", "Hewlett Packard"),
    ("000FB5", "NETGEAR Inc."),
    ("001018", "Broadcom Corporation"),
    ("001124", "Apple, Inc."),
    ("0012FB", "Samsung Electronics"),
    ("001349", "Zyxel Communications Corporation"),
    ("0013A9", "Sony Corporation"),
    ("0013CE", "Intel Corporate"),
    ("001451", "Apple, Inc."),
    ("001517", "Intel Corporate"),
    ("00155D", "Microsoft Corporation (Hyper-V)"),
    ("0015B9", "Samsung Electronics"),
    ("001560", "Hewlett Packard"),
    ("001656", "Nintendo Co., Ltd."),
    ("001617", "Micro-Star INTL CO., LTD."),
    ("0016CB", "Apple, Inc."),
    ("001788", "Philips Lighting BV"),
    ("001839", "Cisco-Linksys, LLC"),
    ("00188B", "Dell Inc."),
    ("0019FD", "Nintendo Co., Ltd."),
    ("001A11", "Google, Inc."),
    ("001A64", "IBM Corporation"),
    ("001A70", "Cisco-Linksys, LLC"),
    ("001A80", "Sony Corporation"),
    ("001B21", "Intel Corporate"),
    ("001B2F", "NETGEAR Inc."),
    ("001B63", "Apple, Inc."),
    ("001B77", "Intel Corporate"),
    ("001BFC", "ASUSTek Computer Inc."),
    ("001C14", "VMware, Inc."),
    ("001C23", "Dell Inc."),
    ("001C25", "Hon Hai Precision Ind. Co., Ltd."),
    ("001C42", "Parallels, Inc."),
    ("001C4A", "AVM GmbH"),
    ("001C62", "LG Electronics"),
    ("001C73", "Arista Networks"),
    ("001CC4", "Hewlett Packard"),
    ("001CDF", "Belkin International Inc."),
    ("001CF0", "D-Link Corporation"),
    ("001D0F", "TP-LINK Technologies Co., Ltd."),
    ("001DB5", "Juniper Networks"),
    ("001DD8", "Microsoft Corporation"),
    ("001E10", "Huawei Technologies Co., Ltd"),
    ("001EC2", "Apple, Inc."),
    ("001EC9", "Dell Inc."),
    ("001FD0", "GIGA-BYTE Technology Co., Ltd."),
    ("002037", "Seagate Technology"),
    ("002215", "ASUSTek Computer Inc."),
    ("0022A9", "LG Electronics"),
    ("002332", "Apple, Inc."),
    ("00219B", "Dell Inc."),
    ("0021BD", "Nintendo Co., Ltd."),
    ("0021D1", "Samsung Electronics"),
    ("002436", "Apple, Inc."),
    ("002490", "Samsung Electronics"),
    ("002556", "Hon Hai Precision Ind. Co., Ltd."),
    ("002590", "Super Micro Computer, Inc."),
    ("00259C", "Cisco-Linksys, LLC"),
    ("00259E", "Huawei Technologies Co., Ltd"),
    ("002608", "Apple, Inc."),
    ("002722", "Ubiquiti Networks Inc."),
    ("003048", "Super Micro Computer, Inc."),
    ("00306E", "Hewlett Packard"),
    ("0050BA", "D-Link Corporation"),
    ("005056", "VMware, Inc."),
    ("0050F2", "Microsoft Corporation"),
    ("0080C2", "IEEE 802.1 Working Group"),
    ("009EC8", "Xiaomi Communications Co Ltd"),
    ("00A0C9", "Intel Corporation"),
    ("00E04C", "Realtek Semiconductor Corp."),
    ("00095B", "NETGEAR Inc."),
    ("000B86", "Aruba Networks"),
    ("00163E", "Xensource, Inc."),
    ("00037F", "Atheros Communications, Inc."),
    ("000FB0", "Compal Electronics, Inc."),
    ("040CCE", "Apple, Inc."),
    ("0418D6", "Ubiquiti Networks Inc."),
    ("080027", "PCS Systemtechnik GmbH (VirtualBox)"),
    ("080046", "Sony Corporation"),
    ("085B0E", "Fortinet Inc."),
    ("08863B", "Belkin International Inc."),
    ("0819A6", "Huawei Technologies Co., Ltd"),
    ("0C47C9", "Amazon Technologies Inc."),
    ("10683F", "LG Electronics"),
    ("14CC20", "TP-LINK Technologies Co., Ltd."),
    ("18B430", "Nest Labs Inc."),
    ("1C6F65", "GIGA-BYTE Technology Co., Ltd."),
    ("240AC4", "Espressif Inc."),
    ("246F28", "Espressif Inc."),
    ("24A43C", "Ubiquiti Networks Inc."),
    ("283CE4", "Huawei Technologies Co., Ltd"),
    ("286C07", "Xiaomi Communications Co Ltd"),
    ("2C56DC", "ASUSTek Computer Inc."),
    ("2C6BF5", "Juniper Networks"),
    ("30AEA4", "Espressif Inc."),
    ("30F9ED", "Sony Corporation"),
    ("3085A9", "ASUSTek Computer Inc."),
    ("347E5C", "Sonos, Inc."),
    ("34BE00", "Samsung Electronics"),
    ("3C0754", "Apple, Inc."),
    ("3C4A92", "Hewlett Packard"),
    ("3C5AB4", "Google, Inc."),
    ("3C8AB0", "Juniper Networks"),
    ("3CA62F", "AVM GmbH"),
    ("3CBDD8", "LG Electronics"),
    ("40B4CD", "Amazon Technologies Inc."),
    ("40D28A", "Nintendo Co., Ltd."),
    ("44650D", "Amazon Technologies Inc."),
    ("44D9E7", "Ubiquiti Networks Inc."),
    ("48DB50", "Huawei Technologies Co., Ltd"),
    ("4C5E0C", "Routerboard.com (MikroTik)"),
    ("4C8D79", "Apple, Inc."),
    ("4CCC6A", "Micro-Star INTL CO., LTD."),
    ("4CFCAA", "Tesla, Inc."),
    ("50465D", "ASUSTek Computer Inc."),
    ("525400", "QEMU / KVM virtual NIC"),
    ("5C63BF", "TP-LINK Technologies Co., Ltd."),
    ("5CCF7F", "Espressif Inc."),
    ("5CF3FC", "IBM Corporation"),
    ("5CF6DC", "Samsung Electronics"),
    ("640980", "Xiaomi Communications Co Ltd"),
    ("641666", "Nest Labs Inc."),
    ("6837E9", "Amazon Technologies Inc."),
    ("6C3B6B", "Routerboard.com (MikroTik)"),
    ("6CC217", "Hewlett Packard Enterprise"),
    ("748114", "Routerboard.com (MikroTik)"),
    ("74C246", "Amazon Technologies Inc."),
    ("74D435", "GIGA-BYTE Technology Co., Ltd."),
    ("781DBA", "Huawei Technologies Co., Ltd"),
    ("788A20", "Ubiquiti Networks Inc."),
    ("8425DB", "Samsung Electronics"),
    ("8CAAB5", "Espressif Inc."),
    ("8CBEBE", "Xiaomi Communications Co Ltd"),
    ("8CDCD4", "Hewlett Packard Enterprise"),
    ("906CAC", "Fortinet Inc."),
    ("944452", "Belkin International Inc."),
    ("98ED5C", "Tesla, Inc."),
    ("A020A6", "Espressif Inc."),
    ("A0E4CB", "Zyxel Communications Corporation"),
    ("A45E60", "Apple, Inc."),
    ("AC1F6B", "Super Micro Computer, Inc."),
    ("AC220B", "ASUSTek Computer Inc."),
    ("B0A737", "Roku, Inc."),
    ("B827EB", "Raspberry Pi Foundation"),
    ("B8AC6F", "Dell Inc."),
    ("B8E937", "Sonos, Inc."),
    ("CC6DA0", "Roku, Inc."),
    ("DC3A5E", "Roku, Inc."),
    ("DCA632", "Raspberry Pi Trading Ltd"),
    ("E45F01", "Raspberry Pi Trading Ltd"),
    ("E48D8C", "Routerboard.com (MikroTik)"),
    ("E8508B", "Samsung Electronics"),
    ("ECFABC", "Espressif Inc."),
    ("F09FC2", "Ubiquiti Networks Inc."),
    ("F0B429", "Xiaomi Communications Co Ltd"),
    ("F0272D", "Amazon Technologies Inc."),
    ("F4F5E8", "Google, Inc."),
    ("20E52A", "NETGEAR Inc."),
];

fn mac_address_lookup(args: &Args) -> Result<String> {
    let raw = arg(args, tool("mac-address-lookup"), "mac").trim();
    let digits: String = raw.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if digits.len() < 6 {
        bail!("need at least the 6 hex digits of the OUI prefix");
    }
    let prefix = digits[..6].to_uppercase();
    // it-tools prints "Unknown vendor for this address" in the card; a miss is
    // not an error.
    let vendor = OUI
        .iter()
        .find(|(p, _)| *p == prefix)
        .map_or("Unknown vendor for this address", |(_, v)| v);
    Ok(format!("OUI prefix: {prefix}\nVendor info: {vendor}\n"))
}

// ------------------------------------------------------ random-port-generator

fn random_port_generator(args: &Args) -> Result<String> {
    let count: usize = arg(args, tool("random-port-generator"), "quantity")
        .trim()
        .parse()
        .unwrap_or(1);
    if !(1..=1000).contains(&count) {
        bail!("quantity must be between 1 and 1000");
    }
    Ok((0..count).fold(String::new(), |mut s, _| {
        let _ = writeln!(s, "{}", rand::random_range(1024..=65535u32));
        s
    }))
}

// ------------------------------------------------- iban-validator-and-parser

/// `friendlyFormatIBAN`: groups of four, space separated.
fn friendly_iban(iban: &str) -> String {
    iban.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `isQRIBAN`: a Swiss or Liechtenstein IBAN whose institution id is a QR-IID.
fn is_qr_iban(iban: &str) -> bool {
    (iban.starts_with("CH") || iban.starts_with("LI"))
        && iban
            .get(4..9)
            .and_then(|iid| iid.parse::<u32>().ok())
            .is_some_and(|iid| (30000..=31999).contains(&iid))
}

/// it-tools shows every row for an invalid IBAN too — validity is just one of
/// them — so this never fails on a badly formed input, only on an empty one.
fn iban_validator(args: &Args) -> Result<String> {
    use iban::IbanLike as _;
    let raw: String = arg(args, tool("iban-validator-and-parser"), "iban")
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(char::to_uppercase)
        .collect();
    if raw.is_empty() {
        bail!("no IBAN provided");
    }
    let parsed = raw.parse::<iban::Iban>();
    let mut out = String::new();
    let _ = writeln!(out, "Is IBAN valid ?: {}", parsed.is_ok());
    if let Err(e) = &parsed {
        let _ = writeln!(out, "IBAN errors: {e}");
    }
    let _ = writeln!(out, "Is IBAN a QR-IBAN ?: {}", is_qr_iban(&raw));
    let _ = writeln!(
        out,
        "Country code: {}",
        parsed
            .as_ref()
            .map(|i| i.country_code().to_string())
            .unwrap_or_else(|_| raw.chars().take(2).collect())
    );
    let _ = writeln!(
        out,
        "BBAN: {}",
        parsed
            .as_ref()
            .map(|i| i.bban().to_string())
            .unwrap_or_else(|_| raw.chars().skip(4).collect())
    );
    let _ = writeln!(out, "IBAN friendly format: {}", friendly_iban(&raw));
    Ok(out)
}

// ------------------------------------------------ phone-parser-and-formatter

/// ponytail: libphonenumber is not available, so this is a deliberate subset —
/// strip formatting, split the calling code off with the table below, validate
/// the E.164 shape. It cannot tell mobile from fixed line or validate national
/// numbering plans. Upgrade path: a libphonenumber port, when someone needs it.
static CALLING_CODES: &[(&str, &str)] = &[
    ("1", "United States / Canada (NANP)"),
    ("7", "Russia / Kazakhstan"),
    ("20", "Egypt"),
    ("27", "South Africa"),
    ("30", "Greece"),
    ("31", "Netherlands"),
    ("32", "Belgium"),
    ("33", "France"),
    ("34", "Spain"),
    ("36", "Hungary"),
    ("39", "Italy"),
    ("40", "Romania"),
    ("41", "Switzerland"),
    ("43", "Austria"),
    ("44", "United Kingdom"),
    ("45", "Denmark"),
    ("46", "Sweden"),
    ("47", "Norway"),
    ("48", "Poland"),
    ("49", "Germany"),
    ("51", "Peru"),
    ("52", "Mexico"),
    ("53", "Cuba"),
    ("54", "Argentina"),
    ("55", "Brazil"),
    ("56", "Chile"),
    ("57", "Colombia"),
    ("58", "Venezuela"),
    ("60", "Malaysia"),
    ("61", "Australia"),
    ("62", "Indonesia"),
    ("63", "Philippines"),
    ("64", "New Zealand"),
    ("65", "Singapore"),
    ("66", "Thailand"),
    ("81", "Japan"),
    ("82", "South Korea"),
    ("84", "Vietnam"),
    ("86", "China"),
    ("90", "Turkey"),
    ("91", "India"),
    ("92", "Pakistan"),
    ("93", "Afghanistan"),
    ("94", "Sri Lanka"),
    ("95", "Myanmar"),
    ("98", "Iran"),
    ("212", "Morocco"),
    ("213", "Algeria"),
    ("216", "Tunisia"),
    ("218", "Libya"),
    ("220", "Gambia"),
    ("221", "Senegal"),
    ("234", "Nigeria"),
    ("237", "Cameroon"),
    ("251", "Ethiopia"),
    ("254", "Kenya"),
    ("255", "Tanzania"),
    ("256", "Uganda"),
    ("263", "Zimbabwe"),
    ("350", "Gibraltar"),
    ("351", "Portugal"),
    ("352", "Luxembourg"),
    ("353", "Ireland"),
    ("354", "Iceland"),
    ("355", "Albania"),
    ("356", "Malta"),
    ("357", "Cyprus"),
    ("358", "Finland"),
    ("359", "Bulgaria"),
    ("370", "Lithuania"),
    ("371", "Latvia"),
    ("372", "Estonia"),
    ("373", "Moldova"),
    ("374", "Armenia"),
    ("375", "Belarus"),
    ("376", "Andorra"),
    ("377", "Monaco"),
    ("378", "San Marino"),
    ("380", "Ukraine"),
    ("381", "Serbia"),
    ("382", "Montenegro"),
    ("385", "Croatia"),
    ("386", "Slovenia"),
    ("387", "Bosnia and Herzegovina"),
    ("389", "North Macedonia"),
    ("420", "Czechia"),
    ("421", "Slovakia"),
    ("423", "Liechtenstein"),
    ("501", "Belize"),
    ("502", "Guatemala"),
    ("503", "El Salvador"),
    ("504", "Honduras"),
    ("505", "Nicaragua"),
    ("506", "Costa Rica"),
    ("507", "Panama"),
    ("591", "Bolivia"),
    ("593", "Ecuador"),
    ("595", "Paraguay"),
    ("598", "Uruguay"),
    ("852", "Hong Kong"),
    ("853", "Macau"),
    ("855", "Cambodia"),
    ("856", "Laos"),
    ("880", "Bangladesh"),
    ("886", "Taiwan"),
    ("960", "Maldives"),
    ("961", "Lebanon"),
    ("962", "Jordan"),
    ("963", "Syria"),
    ("964", "Iraq"),
    ("965", "Kuwait"),
    ("966", "Saudi Arabia"),
    ("967", "Yemen"),
    ("968", "Oman"),
    ("970", "Palestine"),
    ("971", "United Arab Emirates"),
    ("972", "Israel"),
    ("973", "Bahrain"),
    ("974", "Qatar"),
    ("975", "Bhutan"),
    ("976", "Mongolia"),
    ("977", "Nepal"),
    ("992", "Tajikistan"),
    ("993", "Turkmenistan"),
    ("994", "Azerbaijan"),
    ("995", "Georgia"),
    ("996", "Kyrgyzstan"),
    ("998", "Uzbekistan"),
];

fn phone_parser(args: &Args) -> Result<String> {
    let t = tool("phone-parser-and-formatter");
    let raw = arg(args, t, "phone").trim();
    if raw.is_empty() {
        bail!("no phone number provided");
    }
    let default_code: String = arg(args, t, "default_calling_code")
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    let mut digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    let international = raw.trim_start().starts_with('+') || digits.starts_with("00");
    if digits.starts_with("00") {
        digits = digits[2..].to_string();
    }
    if !international {
        if default_code.is_empty() {
            bail!("no calling code: prefix the number with + or set a default calling code");
        }
        digits = format!("{default_code}{}", digits.trim_start_matches('0'));
    }
    if digits.is_empty() {
        bail!("no digits in {raw:?}");
    }
    let (code, country) = (1..=3)
        .rev()
        .filter(|n| digits.len() > *n)
        .find_map(|n| CALLING_CODES.iter().find(|(c, _)| *c == &digits[..n]))
        .copied()
        .context("unrecognised country calling code")?;
    let national = &digits[code.len()..];
    let valid = (8..=15).contains(&digits.len()) && !national.is_empty();
    let yes_no = |b: bool| if b { "Yes" } else { "No" };
    // it-tools' row set and order. Rows libphonenumber supplies and this subset
    // cannot say "Unknown", which is also what it-tools shows for a nil value.
    let mut out = String::new();
    let _ = writeln!(out, "Country: {country}");
    let _ = writeln!(out, "Country calling code: {code}");
    let _ = writeln!(out, "Is valid?: {}", yes_no(valid));
    let _ = writeln!(out, "Is possible?: {}", yes_no(!national.is_empty()));
    let _ = writeln!(out, "Type: Unknown");
    let _ = writeln!(out, "International format: +{code} {national}");
    let _ = writeln!(out, "National number: {national}");
    let _ = writeln!(out, "E.164 format: +{digits}");
    let _ = writeln!(out, "RFC3966 format: tel:+{digits}");
    Ok(out)
}

// ------------------------------------------------------------ email-normalizer

/// ponytail: provider rules only for the handful of hosts where the aliasing
/// behaviour is documented; everything else is just lower-cased and trimmed.
fn normalize_email(email: &str) -> Result<String> {
    let e = email.trim().to_lowercase();
    let (local, domain) = e.split_once('@').context("missing @")?;
    if local.is_empty() || domain.is_empty() || domain.contains('@') || !domain.contains('.') {
        bail!("not an email address");
    }
    let (local, domain) = match domain {
        "gmail.com" | "googlemail.com" => (
            local.split('+').next().unwrap_or(local).replace('.', ""),
            "gmail.com",
        ),
        "yahoo.com" | "yahoo.co.uk" | "ymail.com" | "rocketmail.com" => {
            (local.split('-').next().unwrap_or(local).to_string(), domain)
        }
        "outlook.com" | "hotmail.com" | "live.com" | "msn.com" | "icloud.com" | "me.com"
        | "protonmail.com" | "proton.me" | "pm.me" | "fastmail.com" | "zoho.com" | "gmx.com"
        | "gmx.de" | "yandex.com" | "yandex.ru" => {
            (local.split('+').next().unwrap_or(local).to_string(), domain)
        }
        _ => (local.to_string(), domain),
    };
    if local.is_empty() {
        bail!("empty local part after normalization");
    }
    Ok(format!("{local}@{domain}"))
}

fn email_normalizer(args: &Args) -> Result<String> {
    let input = arg(args, tool("email-normalizer"), "emails");
    if input.trim().is_empty() {
        bail!("no emails provided");
    }
    Ok(input
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            normalize_email(l).unwrap_or_else(|_| format!("Unable to parse email: {}", l.trim()))
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

// ------------------------------- docker-run-to-docker-compose-converter

/// Split a shell-ish command line, honouring single and double quotes.
fn shell_split(input: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in input.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (None, '\\') => {} // line continuations / escapes: drop
            (None, c) => {
                cur.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        bail!("unterminated quote");
    }
    if started {
        out.push(cur);
    }
    Ok(out)
}

/// ponytail: covers the docker run flags people actually paste. Anything else
/// is reported as "not translated" rather than silently dropped.
fn docker_run_to_compose(args: &Args) -> Result<String> {
    use serde_norway::{Mapping, Value};

    let raw = arg(
        args,
        tool("docker-run-to-docker-compose-converter"),
        "command",
    )
    .trim();
    if raw.is_empty() {
        bail!("no docker run command provided");
    }
    let mut tokens = shell_split(raw)?.into_iter().peekable();
    if tokens.peek().map(String::as_str) == Some("docker") {
        tokens.next();
    }
    match tokens.next().as_deref() {
        Some("run") | Some("create") => {}
        _ => bail!("expected a `docker run ...` command"),
    }

    // key -> yaml list key
    let list_flags: &[(&str, &str)] = &[
        ("-p", "ports"),
        ("--publish", "ports"),
        ("-v", "volumes"),
        ("--volume", "volumes"),
        ("-e", "environment"),
        ("--env", "environment"),
        ("--expose", "expose"),
        ("--cap-add", "cap_add"),
        ("--cap-drop", "cap_drop"),
        ("--device", "devices"),
        ("--dns", "dns"),
        ("--add-host", "extra_hosts"),
        ("-l", "labels"),
        ("--label", "labels"),
        ("--sysctl", "sysctls"),
        ("--tmpfs", "tmpfs"),
        ("--security-opt", "security_opt"),
        ("--network", "networks"),
        ("--net", "networks"),
        ("--link", "links"),
    ];
    let scalar_flags: &[(&str, &str)] = &[
        ("--restart", "restart"),
        ("-w", "working_dir"),
        ("--workdir", "working_dir"),
        ("-u", "user"),
        ("--user", "user"),
        ("--entrypoint", "entrypoint"),
        ("-h", "hostname"),
        ("--hostname", "hostname"),
        ("--ipc", "ipc"),
        ("--pid", "pid"),
        ("--platform", "platform"),
        ("--shm-size", "shm_size"),
        ("--stop-signal", "stop_signal"),
        ("--domainname", "domainname"),
        ("--mac-address", "mac_address"),
        ("-m", "mem_limit"),
        ("--memory", "mem_limit"),
    ];

    let mut lists: Vec<(&str, Vec<String>)> = Vec::new();
    let mut scalars: Vec<(&str, Value)> = Vec::new();
    let mut log_opts: Vec<(String, String)> = Vec::new();
    let mut log_driver: Option<String> = None;
    let mut name: Option<String> = None;
    let mut image: Option<String> = None;
    let mut command: Vec<String> = Vec::new();
    let mut untranslated: Vec<String> = Vec::new();

    let push_list = |lists: &mut Vec<(&str, Vec<String>)>, key: &'static str, v: String| match lists
        .iter_mut()
        .find(|(k, _)| *k == key)
    {
        Some((_, vals)) => vals.push(v),
        None => lists.push((key, vec![v])),
    };

    while let Some(tok) = tokens.next() {
        if image.is_some() {
            command.push(tok);
            continue;
        }
        if !tok.starts_with('-') {
            image = Some(tok);
            continue;
        }
        // --flag=value / -pvalue
        let (flag, inline) = match tok.split_once('=') {
            Some((f, v)) => (f.to_string(), Some(v.to_string())),
            None => (tok.clone(), None),
        };
        let mut take_value = |inline: Option<String>, flag: &str| -> Result<String> {
            match inline {
                Some(v) => Ok(v),
                None => tokens
                    .next()
                    .with_context(|| format!("{flag} expects a value")),
            }
        };

        if let Some((_, key)) = list_flags.iter().find(|(f, _)| *f == flag) {
            let v = take_value(inline, &flag)?;
            push_list(&mut lists, key, v);
            continue;
        }
        if let Some((_, key)) = scalar_flags.iter().find(|(f, _)| *f == flag) {
            let v = take_value(inline, &flag)?;
            scalars.push((key, Value::String(v)));
            continue;
        }
        match flag.as_str() {
            "--name" => name = Some(take_value(inline, "--name")?),
            "--log-driver" => log_driver = Some(take_value(inline, "--log-driver")?),
            "--log-opt" => {
                let v = take_value(inline, "--log-opt")?;
                let (k, val) = v.split_once('=').unwrap_or((v.as_str(), ""));
                log_opts.push((k.to_string(), val.to_string()));
            }
            "--privileged" => scalars.push(("privileged", Value::Bool(true))),
            "--read-only" => scalars.push(("read_only", Value::Bool(true))),
            "--init" => scalars.push(("init", Value::Bool(true))),
            "-d" | "--detach" => {} // compose has no equivalent, `up -d` does it
            "--rm" | "-a" | "--attach" | "--sig-proxy" | "--interactive-only" => {
                untranslated.push(flag)
            }
            "-i" | "--interactive" => scalars.push(("stdin_open", Value::Bool(true))),
            "-t" | "--tty" => scalars.push(("tty", Value::Bool(true))),
            "-it" | "-ti" => {
                scalars.push(("stdin_open", Value::Bool(true)));
                scalars.push(("tty", Value::Bool(true)));
            }
            // bare `-pVALUE` style short flags
            f if f.len() > 2 && !f.starts_with("--") => {
                let (short, v) = (&f[..2], f[2..].to_string());
                if let Some((_, key)) = list_flags.iter().find(|(fl, _)| *fl == short) {
                    push_list(&mut lists, key, v);
                } else if let Some((_, key)) = scalar_flags.iter().find(|(fl, _)| *fl == short) {
                    scalars.push((key, Value::String(v)));
                } else {
                    untranslated.push(f.to_string());
                }
            }
            f => untranslated.push(f.to_string()),
        }
    }

    let image = image.context("no image found in the docker run command")?;
    let service = name.clone().unwrap_or_else(|| {
        image
            .rsplit('/')
            .next()
            .unwrap_or(&image)
            .split(':')
            .next()
            .unwrap_or(&image)
            .to_string()
    });

    let mut svc = Mapping::new();
    svc.insert("image".into(), Value::String(image));
    if let Some(n) = name {
        svc.insert("container_name".into(), Value::String(n));
    }
    for (key, vals) in lists {
        svc.insert(
            key.into(),
            Value::Sequence(vals.into_iter().map(Value::String).collect()),
        );
    }
    for (key, v) in scalars {
        svc.insert(key.into(), v);
    }
    if log_driver.is_some() || !log_opts.is_empty() {
        let mut logging = Mapping::new();
        if let Some(d) = log_driver {
            logging.insert("driver".into(), Value::String(d));
        }
        if !log_opts.is_empty() {
            let mut opts = Mapping::new();
            for (k, v) in log_opts {
                opts.insert(k.into(), Value::String(v));
            }
            logging.insert("options".into(), Value::Mapping(opts));
        }
        svc.insert("logging".into(), Value::Mapping(logging));
    }
    if !command.is_empty() {
        svc.insert(
            "command".into(),
            Value::Sequence(command.into_iter().map(Value::String).collect()),
        );
    }

    let mut services = Mapping::new();
    services.insert(service.into(), Value::Mapping(svc));
    let mut root = Mapping::new();
    root.insert("services".into(), Value::Mapping(services));

    let mut out = serde_norway::to_string(&Value::Mapping(root)).context("could not emit YAML")?;
    if !untranslated.is_empty() {
        let _ = write!(
            out,
            "\n# not translated to docker-compose: {}\n",
            untranslated.join(", ")
        );
    }
    Ok(out)
}

// ------------------------------------------------------------ crontab-generator

/// The field diagram and the symbol table it-tools shows under the expression.
const CRON_HELP: &str = "\
Field order:
  ┌──────────── [optional] seconds (0 - 59)
  | ┌────────── minute (0 - 59)
  | | ┌──────── hour (0 - 23)
  | | | ┌────── day of month (1 - 31)
  | | | | ┌──── month (1 - 12) OR jan,feb,mar,apr ...
  | | | | | ┌── day of week (0 - 6, sunday=0) OR sun,mon ...
  | | | | | |
  * * * * * * command

Symbols:
  *          Any value                                          * * * *      Every minute
  -          Range of values                                    1-10 * * *   Minutes 1 through 10
  ,          List of values                                     1,10 * * *   At minutes 1 and 10
  /          Step values                                        */10 * * *   Every 10 minutes
  @yearly    Once every year at midnight of 1 January            @yearly      0 0 1 1 *
  @annually  Same as @yearly                                    @annually    0 0 1 1 *
  @monthly   Once a month at midnight on the first day          @monthly     0 0 1 * *
  @weekly    Once a week at midnight on Sunday morning          @weekly      0 0 * * 0
  @daily     Once a day at midnight                             @daily       0 0 * * *
  @midnight  Same as @daily                                     @midnight    0 0 * * *
  @hourly    Once an hour at the beginning of the hour          @hourly      0 * * * *
  @reboot    Run at startup
";

fn crontab_generator(args: &Args) -> Result<String> {
    use std::str::FromStr as _;
    let expr = arg(args, tool("crontab-generator"), "expression").trim();
    if expr.is_empty() {
        bail!("no cron expression provided");
    }
    let cron = croner::Cron::from_str(expr).map_err(|e| anyhow::anyhow!("invalid cron: {e}"))?;
    let mut out = String::new();
    let _ = writeln!(out, "Expression: {expr}");
    let _ = writeln!(out, "Meaning: {}", cron.describe());
    // ponytail: next occurrences are ours, not it-tools'; cheap and the obvious
    // sanity check that the expression means what the description says.
    let _ = writeln!(out, "\nNext occurrences (UTC):");
    for t in cron
        .iter_from(chrono::Utc::now(), croner::Direction::Forward)
        .take(5)
    {
        let _ = writeln!(out, "  {}", t.format("%Y-%m-%d %H:%M:%S"));
    }
    let _ = writeln!(out, "\n{CRON_HELP}");
    Ok(out)
}

// ------------------------------------------------------------ chmod-calculator

/// The permission axes, in the order [`chmod_forms`] indexes them. `pub` because
/// the app's chmod panel draws a grid on exactly these axes, and a reorder here
/// that the panel did not follow would transpose every answer it gives.
pub const CHMOD_GROUPS: [&str; 3] = ["owner", "group", "public"];
pub const CHMOD_SCOPES: [(&str, u8); 3] = [("read", 4), ("write", 2), ("execute", 1)];

/// Octal and symbolic form of a permission matrix, indexed
/// `[owner, group, public][read, write, execute]` — the order of
/// [`CHMOD_GROUPS`] and [`CHMOD_SCOPES`].
///
/// Public so the app's chmod panel, which owns nine checkboxes rather than nine
/// text fields, computes the answer with this and not a second copy of the bit
/// weights.
pub fn chmod_forms(bits: [[bool; 3]; 3]) -> (String, String) {
    let digits = bits.map(|group| {
        group
            .iter()
            .zip(CHMOD_SCOPES)
            .filter(|(on, _)| **on)
            .map(|(_, (_, bit))| bit)
            .sum::<u8>()
    });
    let symbolic: String = digits
        .iter()
        .flat_map(|d| {
            [
                if d & 4 != 0 { 'r' } else { '-' },
                if d & 2 != 0 { 'w' } else { '-' },
                if d & 1 != 0 { 'x' } else { '-' },
            ]
        })
        .collect();
    (digits.iter().map(|d| d.to_string()).collect(), symbolic)
}

/// it-tools is a 3x3 permission checkbox grid with the octal and the symbolic
/// form both always on screen — there is nothing to parse and nothing to fail.
fn chmod_calculator(args: &Args) -> Result<String> {
    let t = tool("chmod-calculator");
    let bits = CHMOD_GROUPS
        .map(|g| CHMOD_SCOPES.map(|(scope, _)| arg(args, t, &format!("{g}_{scope}")) == "true"));
    let (octal, symbolic) = chmod_forms(bits);
    Ok(format!(
        "Octal: {octal}\nSymbolic: {symbolic}\nCommand: chmod {octal} path\n"
    ))
}

// -------------------------------------------------------------- slugify-string

fn slugify_string(args: &Args) -> Result<String> {
    let input = arg(args, tool("slugify-string"), "input");
    if input.trim().is_empty() {
        bail!("nothing to slugify");
    }
    let s = slug::slugify(input);
    if s.is_empty() {
        bail!("nothing sluggable in this input");
    }
    Ok(s)
}

// --------------------------------------------------------- device-information

fn device_information(_args: &Args) -> Result<String> {
    bail!(
        "needs the device panel: the upstream tool reports the browser          environment, which has no meaning here, so this reports the desktop one          instead — OS, CPU, window geometry and paths"
    )
}

// ------------------------------------------------------------------- registry

pub static TOOLS: &[Tool] = &[
    Tool {
        id: "url-encoder",
        name: "Encode/decode url formatted strings",
        description: "Encode to url-encoded format (also known as percent-encoded) or decode from it.",
        category: Category::Web,
        keywords: &["url", "encode", "decode", "percent", "%20", "format"],
        fields: &[
            Field {
                key: "encode_input",
                label: "Your string",
                kind: crate::FieldKind::TextArea,
                default: "Hello world :)",
                options: &[],
            },
            Field {
                key: "decode_input",
                label: "Your encoded string",
                kind: crate::FieldKind::TextArea,
                default: "Hello%20world%20%3A)",
                options: &[],
            },
        ],
        kind: Kind::Pure,
        run: url_encoder,
    },
    Tool {
        id: "url-parser",
        name: "URL parser",
        description: "Parse a url into its separate components (protocol, origin, params, port, username, password...).",
        category: Category::Web,
        keywords: &[
            "url", "parser", "protocol", "origin", "params", "port", "username", "password", "href",
        ],
        fields: &[Field {
            key: "url",
            label: "Your url to parse",
            kind: crate::FieldKind::Text,
            default: "https://me:pwd@it-tools.tech:3000/url-parser?key1=value&key2=value2#the-hash",
            options: &[],
        }],
        kind: Kind::Pure,
        run: url_parser,
    },
    Tool {
        id: "html-entities",
        name: "Escape html entities",
        description: "Escape or unescape html entities (replace <, >, &, \" and ' to their html version).",
        category: Category::Web,
        keywords: &[
            "html",
            "entities",
            "escape",
            "unescape",
            "special",
            "characters",
            "tags",
        ],
        fields: &[
            Field {
                key: "escape_input",
                label: "Your string",
                kind: crate::FieldKind::TextArea,
                default: "<title>IT Tool</title>",
                options: &[],
            },
            Field {
                key: "unescape_input",
                label: "Your escaped string",
                kind: crate::FieldKind::TextArea,
                default: "&lt;title&gt;IT Tool&lt;/title&gt;",
                options: &[],
            },
        ],
        kind: Kind::Pure,
        run: html_entities,
    },
    Tool {
        id: "safelink-decoder",
        name: "Outlook Safelink decoder",
        description: "Decode Outlook SafeLink links.",
        category: Category::Web,
        keywords: &["outlook", "safelink", "decoder"],
        fields: &[Field::area("url", "Your SafeLink url")],
        kind: Kind::Pure,
        run: safelink_decoder,
    },
    Tool {
        id: "user-agent-parser",
        name: "User-agent parser",
        description: "Detect browser, engine, OS and device from a user-agent string.",
        category: Category::Web,
        keywords: &[
            "user",
            "agent",
            "parser",
            "browser",
            "engine",
            "os",
            "cpu",
            "device",
            "user-agent",
            "client",
        ],
        fields: &[Field::area("user_agent", "User agent string")],
        kind: Kind::Pure,
        run: user_agent_parser,
    },
    Tool {
        id: "mime-types",
        name: "Mime types",
        description: "Convert file extensions to mime types and the other way around.",
        category: Category::Web,
        keywords: &["mime", "types", "extension", "content", "type"],
        fields: &[
            Field::text("mime_type", "Mime type (ex: application/pdf)"),
            Field::text("extension", "File extension (ex: pdf)"),
        ],
        kind: Kind::Pure,
        run: mime_types,
    },
    Tool {
        id: "http-status-codes",
        name: "HTTP status codes",
        description: "The list of all HTTP status codes, their name and their meaning.",
        category: Category::Web,
        keywords: &["http", "status", "codes", "rest", "response"],
        fields: &[Field::text("query", "Search a code, name or description")],
        kind: Kind::Pure,
        run: http_status_codes,
    },
    Tool {
        id: "meta-tag-generator",
        name: "Open graph meta generator",
        description: "Generate open-graph and social meta tags for your website.",
        category: Category::Web,
        keywords: &[
            "og", "open", "graph", "meta", "tags", "twitter", "seo", "social",
        ],
        // it-tools' section order: General information, Image, Twitter.
        fields: &[
            Field::select(
                "type",
                "Page type",
                &[
                    "website",
                    "article",
                    "book",
                    "profile",
                    "music.song",
                    "music.album",
                    "music.playlist",
                    "music.radio_station",
                    "video.movie",
                    "video.episode",
                    "video.tv_show",
                    "video.other",
                ],
                "website",
            ),
            Field::text("title", "Title"),
            Field::text("description", "Description"),
            Field::text("url", "Page URL"),
            Field::text("image", "Image url"),
            Field::text("image_alt", "Image alt"),
            Field::text("image_width", "Width"),
            Field::text("image_height", "Height"),
            Field::select(
                "twitter_card",
                "Twitter card type",
                &["summary", "summary_large_image", "app", "player"],
                "summary_large_image",
            ),
            Field::text("twitter_site", "Site account"),
            Field::text("twitter_creator", "Creator acc."),
        ],
        kind: Kind::Pure,
        run: meta_tag_generator,
    },
    Tool {
        id: "ipv4-address-converter",
        name: "IPv4 address converter",
        description: "Convert an IP address into decimal, binary, hexadecimal or the IPv6 mapped form.",
        category: Category::Network,
        keywords: &[
            "ipv4",
            "address",
            "converter",
            "decimal",
            "hexadecimal",
            "binary",
            "ipv6",
        ],
        fields: &[Field {
            key: "ip",
            label: "The ipv4 address",
            kind: crate::FieldKind::Text,
            default: "192.168.1.1",
            options: &[],
        }],
        kind: Kind::Pure,
        run: ipv4_address_converter,
    },
    Tool {
        id: "ipv4-range-expander",
        name: "IPv4 range expander",
        description: "Given a start and an end IPv4 address, compute the smallest CIDR that contains the range.",
        category: Category::Network,
        keywords: &["ipv4", "range", "expander", "subnet", "creator", "cidr"],
        fields: &[
            Field {
                key: "start_ip",
                label: "Start address",
                kind: crate::FieldKind::Text,
                default: "192.168.1.1",
                options: &[],
            },
            Field {
                key: "end_ip",
                label: "End address",
                kind: crate::FieldKind::Text,
                default: "192.168.6.255",
                options: &[],
            },
        ],
        kind: Kind::Pure,
        run: ipv4_range_expander,
    },
    Tool {
        id: "ipv4-subnet-calculator",
        name: "IPv4 subnet calculator",
        description: "Parse your IPv4 CIDR blocks and get all the information about the subnet.",
        category: Category::Network,
        keywords: &[
            "ipv4",
            "subnet",
            "calculator",
            "mask",
            "network",
            "cidr",
            "netmask",
            "bitmask",
            "broadcast",
        ],
        fields: &[Field {
            key: "ip",
            label: "An IPv4 address with or without mask",
            kind: crate::FieldKind::Text,
            default: "192.168.0.1/24",
            options: &[],
        }],
        kind: Kind::Pure,
        run: ipv4_subnet_calculator,
    },
    Tool {
        id: "ipv6-ula-generator",
        name: "IPv6 ULA generator",
        description: "Generate a random RFC4193 IPv6 unique local address from a MAC address.",
        category: Category::Network,
        keywords: &["ipv6", "ula", "generator", "rfc4193", "network", "private"],
        fields: &[Field {
            key: "mac",
            label: "MAC address",
            kind: crate::FieldKind::Text,
            default: "20:37:06:12:34:56",
            options: &[],
        }],
        kind: Kind::Pure,
        run: ipv6_ula_generator,
    },
    Tool {
        id: "mac-address-generator",
        name: "MAC address generator",
        description: "Generate random MAC addresses, optionally with a fixed vendor prefix.",
        category: Category::Network,
        keywords: &["mac", "address", "generator", "random", "prefix"],
        fields: &[
            Field::number("quantity", "Quantity", "1"),
            Field {
                key: "prefix",
                label: "MAC address prefix",
                kind: crate::FieldKind::Text,
                default: "64:16:7F",
                options: &[],
            },
            // it-tools' fourth separator option is "None", i.e. no separator.
            Field::select("separator", "Separator", &[":", "-", ".", ""], ":"),
            Field::bool("uppercase", "Uppercase", "true"),
        ],
        kind: Kind::Pure,
        run: mac_address_generator,
    },
    Tool {
        id: "mac-address-lookup",
        name: "MAC address lookup",
        description: "Find the vendor of a network device from its MAC address.",
        category: Category::Network,
        keywords: &[
            "mac",
            "address",
            "lookup",
            "vendor",
            "parser",
            "manufacturer",
        ],
        fields: &[Field {
            key: "mac",
            label: "MAC address",
            kind: crate::FieldKind::Text,
            default: "00:0C:29:12:34:56",
            options: &[],
        }],
        kind: Kind::Pure,
        run: mac_address_lookup,
    },
    Tool {
        id: "random-port-generator",
        name: "Random port generator",
        description: "Generate random port numbers outside of the range of the known ports (0-1023).",
        category: Category::Network,
        keywords: &[
            "system",
            "port",
            "lan",
            "generator",
            "random",
            "development",
            "computer",
        ],
        fields: &[Field::number("quantity", "Quantity", "1")],
        kind: Kind::Pure,
        run: random_port_generator,
    },
    Tool {
        id: "iban-validator-and-parser",
        name: "IBAN validator and parser",
        description: "Validate and parse IBAN numbers; check if the IBAN is valid and get its parts.",
        category: Category::Data,
        keywords: &["iban", "validator", "parser", "bic", "bank"],
        fields: &[Field::text("iban", "IBAN to validate and parse")],
        kind: Kind::Pure,
        run: iban_validator,
    },
    Tool {
        id: "phone-parser-and-formatter",
        name: "Phone parser and formatter",
        description: "Parse, validate and format a phone number: country calling code, national number and E.164.",
        category: Category::Data,
        keywords: &[
            "phone",
            "number",
            "parser",
            "formatter",
            "e164",
            "country",
            "international",
        ],
        fields: &[
            Field::text("phone", "Phone number"),
            Field::text("default_calling_code", "Default calling code (e.g. 49)"),
        ],
        kind: Kind::Pure,
        run: phone_parser,
    },
    Tool {
        id: "email-normalizer",
        name: "Email normalizer",
        description: "Normalize email addresses to a standard format for easier comparison and deduplication.",
        category: Category::Data,
        keywords: &["email", "normalizer"],
        fields: &[Field::area(
            "emails",
            "Raw emails to normalize (one per line)",
        )],
        kind: Kind::Pure,
        run: email_normalizer,
    },
    Tool {
        id: "docker-run-to-docker-compose-converter",
        name: "Docker run to docker-compose converter",
        description: "Turn a docker run command into its docker-compose equivalent.",
        category: Category::Development,
        keywords: &[
            "docker", "run", "compose", "yaml", "yml", "convert", "deamon",
        ],
        fields: &[Field {
            key: "command",
            label: "Your docker run command",
            kind: crate::FieldKind::TextArea,
            default: "docker run -p 80:80 -v /var/run/docker.sock:/tmp/docker.sock:ro --restart always --log-opt max-size=1g nginx",
            options: &[],
        }],
        kind: Kind::Pure,
        run: docker_run_to_compose,
    },
    Tool {
        id: "crontab-generator",
        name: "Crontab generator",
        description: "Validate and describe a crontab expression, and see its next occurrences.",
        category: Category::Development,
        keywords: &["crontab", "cron", "job", "schedule", "task", "generator"],
        fields: &[Field {
            key: "expression",
            label: "Cron expression",
            kind: crate::FieldKind::Text,
            default: "40 * * * *",
            options: &[],
        }],
        kind: Kind::Pure,
        run: crontab_generator,
    },
    Tool {
        id: "chmod-calculator",
        name: "Chmod calculator",
        description: "Convert between octal and symbolic Unix file permissions.",
        category: Category::Development,
        keywords: &[
            "chmod",
            "calculator",
            "file",
            "permission",
            "octal",
            "symbolic",
            "unix",
        ],
        // The 3x3 grid, read across then down like it-tools' table.
        fields: &[
            Field::bool("owner_read", "Owner (u) read (4)", "false"),
            Field::bool("group_read", "Group (g) read (4)", "false"),
            Field::bool("public_read", "Public (o) read (4)", "false"),
            Field::bool("owner_write", "Owner (u) write (2)", "false"),
            Field::bool("group_write", "Group (g) write (2)", "false"),
            Field::bool("public_write", "Public (o) write (2)", "false"),
            Field::bool("owner_execute", "Owner (u) execute (1)", "false"),
            Field::bool("group_execute", "Group (g) execute (1)", "false"),
            Field::bool("public_execute", "Public (o) execute (1)", "false"),
        ],
        kind: Kind::Pure,
        run: chmod_calculator,
    },
    Tool {
        id: "slugify-string",
        name: "Slugify string",
        description: "Make a string url, filename and id safe.",
        category: Category::Text,
        keywords: &[
            "slugify",
            "string",
            "escape",
            "emoji",
            "special",
            "character",
            "space",
            "trim",
        ],
        fields: &[Field::area("input", "Your string to slugify")],
        kind: Kind::Pure,
        run: slugify_string,
    },
    Tool {
        id: "device-information",
        name: "Device information",
        description: "Get information about your current device: screen size, pixel ratio, user agent.",
        category: Category::Measurement,
        keywords: &["device", "information", "screen", "viewport", "browser"],
        fields: &[],
        // The browser environment has no meaning in a desktop app; the UI must
        // read the real window/monitor itself.
        kind: Kind::Custom,
        run: device_information,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn args(pairs: &[(&str, &str)]) -> Args {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn run(id: &str, pairs: &[(&str, &str)]) -> Result<String> {
        let t = TOOLS.iter().find(|t| t.id == id).expect("tool exists");
        (t.run)(&args(pairs))
    }

    /// `run` fns look themselves up by id now, so every id they ask for must
    /// exist. This is what used to be `registry_order_is_pinned`.
    #[test]
    fn every_tool_can_find_itself() {
        for t in TOOLS {
            assert_eq!(tool(t.id).id, t.id);
        }
    }

    #[test]
    fn registry_is_sane() {
        let mut ids: Vec<_> = TOOLS.iter().map(|t| t.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate tool ids");
        for t in TOOLS {
            // Must not panic on empty args, and Custom tools must always error.
            let r = (t.run)(&Args::new());
            if t.kind == Kind::Custom {
                assert!(r.is_err(), "{} is Custom but returned Ok", t.id);
            }
            // Fields declare unique keys.
            let mut keys: Vec<_> = t.fields.iter().map(|f| f.key).collect();
            let n = keys.len();
            keys.sort_unstable();
            keys.dedup();
            assert_eq!(keys.len(), n, "{} has duplicate field keys", t.id);
        }
    }

    #[test]
    fn tools_do_not_panic_on_garbage() {
        for t in TOOLS {
            for junk in ["", " ", "\0\u{1f600}", "-1", "!!!!", "\n\n"] {
                let a: Args = t
                    .fields
                    .iter()
                    .map(|f| (f.key.to_string(), junk.to_string()))
                    .collect();
                let _ = (t.run)(&a); // Ok or Err, never a panic.
            }
        }
    }

    // ---- url-encoder / url-parser

    #[test]
    fn url_encoding_shows_both_directions() {
        // it-tools defaults: 'Hello world :)' <-> 'Hello%20world%20%3A)'
        let out = run(
            "url-encoder",
            &[
                ("encode_input", "Hello world :)"),
                ("decode_input", "Hello%20world%20%3A)"),
            ],
        )
        .unwrap();
        assert!(out.contains("Encoded:\nHello%20world%20%3A)"), "{out}");
        assert!(out.contains("Decoded:\nHello world :)"), "{out}");
        // A bad decode input must not take the encode panel down with it.
        let out = run(
            "url-encoder",
            &[("encode_input", "a b"), ("decode_input", "%zz")],
        )
        .unwrap();
        assert!(out.contains("Encoded:\na%20b"), "{out}");
        assert!(out.contains("Impossible to parse this string"), "{out}");
    }

    #[test]
    fn url_parser_splits_all_components() {
        let out = run(
            "url-parser",
            &[(
                "url",
                "https://me:pwd@it-tools.tech:3000/url-parser?key1=value&key2=value2#the-hash",
            )],
        )
        .unwrap();
        assert!(out.contains("Protocol: https:"), "{out}");
        assert!(out.contains("Username: me"));
        assert!(out.contains("Password: pwd"));
        assert!(out.contains("Hostname: it-tools.tech"));
        assert!(out.contains("Port: 3000"));
        assert!(out.contains("Path: /url-parser"));
        assert!(out.contains("Hash: the-hash"));
        assert!(out.contains("key1 = value"));
        assert!(out.contains("key2 = value2"));
        assert!(run("url-parser", &[("url", "not a url")]).is_err());
        assert!(run("url-parser", &[("url", "https://")]).is_err());
    }

    // ---- html-entities

    #[test]
    fn html_entities_escape_and_unescape() {
        let out = run(
            "html-entities",
            &[
                ("escape_input", "<title>IT Tool</title>"),
                ("unescape_input", "&lt;title&gt;IT Tool&lt;/title&gt;"),
            ],
        )
        .unwrap();
        assert!(
            out.contains("Escaped:\n&lt;title&gt;IT Tool&lt;/title&gt;"),
            "{out}"
        );
        assert!(out.contains("Unescaped:\n<title>IT Tool</title>"), "{out}");
    }

    // ---- safelink-decoder (vectors from safelink-decoder.service.test.ts)

    #[test]
    fn safelink_decoder_vectors() {
        let plain = "https://aus01.safelinks.protection.outlook.com/?url=https%3A%2F%2Fwww.google.com%2Fsearch%3Fq%3Dsafelink%26rlz%3D1&data=05%7C02%7C&sdata=x&reserved=0";
        assert_eq!(
            run("safelink-decoder", &[("url", plain)]).unwrap(),
            "https://www.google.com/search?q=safelink&rlz=1"
        );
        let amped = "https://aus01.safelinks.protection.outlook.com/?url=https%3A%2F%2Fwww.google.com%2Fsearch%3Fq%3Dsafelink%26rlz%3D1&amp;data=05&amp;reserved=0";
        assert_eq!(
            run("safelink-decoder", &[("url", amped)]).unwrap(),
            "https://www.google.com/search?q=safelink&rlz=1"
        );
        assert!(run("safelink-decoder", &[("url", "https://google.com")]).is_err());
    }

    // ---- user-agent-parser

    #[test]
    fn user_agent_parser_emits_every_it_tools_section() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
        let out = run("user-agent-parser", &[("user_agent", ua)]).unwrap();
        for section in ["Browser:", "Engine:", "OS:", "Device:", "CPU:"] {
            assert!(out.contains(section), "missing {section} in {out}");
        }
        assert!(out.contains("  Name: Chrome"), "{out}");
        assert!(out.contains("  Version: 120.0.0.0"), "{out}");
        assert!(out.contains("  Name: Blink"), "{out}");
        assert!(out.contains("  Name: Windows 10"), "{out}");
        assert!(out.contains("  Architecture: amd64"), "{out}");
        assert!(out.contains("  Model: No device model available"), "{out}");
        // An unparseable UA still fills the engine/CPU rows it can.
        let out = run("user-agent-parser", &[("user_agent", "!!!!")]).unwrap();
        assert!(out.contains("No engine name available"), "{out}");
        assert!(run("user-agent-parser", &[("user_agent", "   ")]).is_err());
    }

    // ---- mime-types

    #[test]
    fn mime_accessors_answer_both_directions() {
        assert_eq!(mime_for_extension("pdf"), Some("application/pdf"));
        assert_eq!(mime_for_extension("png"), Some("image/png"));
        assert_eq!(mime_for_extension("nope-not-an-ext"), None);
        assert_eq!(mime_for_extension(""), None);
        // Extensions the table has no entry for at all.
        assert_eq!(mime_for_extension("bru"), Some("text/plain"));
        assert_eq!(mime_for_extension("http"), Some("text/plain"));
        assert_eq!(mime_for_extension("lock"), Some("text/plain"));
        assert_eq!(mime_for_extension("jsonc"), Some("application/json"));
        // The table wins wherever it has an answer, including these, which it
        // does know — no override needed.
        assert_eq!(mime_for_extension("toml"), Some("text/x-toml"));
        assert_eq!(mime_for_extension("rs"), Some("text/x-rust"));
        assert_eq!(mime_for_extension("json"), Some("application/json"));
        // `.ts` is the one deliberate override: the table says MPEG-TS video.
        assert_eq!(mime_for_extension("ts"), Some("text/typescript"));
        assert_eq!(mime_for_extension(".TS"), Some("text/typescript"));
        // Leading dots and case are accepted everywhere.
        assert_eq!(mime_for_extension(".PNG"), Some("image/png"));
        assert!(extensions_for_mime("application/pdf").contains(&"pdf"));
        assert!(extensions_for_mime("nope-not-a-type").is_empty());
    }

    #[test]
    fn mime_types_both_directions() {
        let out = run(
            "mime-types",
            &[("mime_type", "application/pdf"), ("extension", "pdf")],
        )
        .unwrap();
        assert!(
            out.contains("Mime type of .pdf files: application/pdf"),
            "{out}"
        );
        assert!(
            out.contains("Extensions of application/pdf files: .pdf"),
            "{out}"
        );
        // One unknown side leaves the other side intact.
        let out = run(
            "mime-types",
            &[("mime_type", "nope-not-a-type"), ("extension", "pdf")],
        )
        .unwrap();
        assert!(out.contains("unknown mime type"), "{out}");
        assert!(out.contains("application/pdf"), "{out}");
    }

    // ---- http-status-codes

    #[test]
    fn http_status_codes_lookup() {
        let out = run("http-status-codes", &[("query", "404")]).unwrap();
        assert!(out.contains("404 Not Found"), "{out}");
        let out = run("http-status-codes", &[("query", "teapot")]).unwrap();
        assert!(out.contains("Search results"), "{out}");
        assert!(out.contains("418"), "{out}");
        // Empty query lists everything, grouped under the five categories.
        let all = run("http-status-codes", &[("query", "")]).unwrap();
        assert_eq!(all.lines().count(), STATUS_CODES.len() * 2 + 5);
        assert!(all.contains("4xx client error"), "{all}");
        assert!(all.contains("    The server is processing the request, but no response is available yet. For WebDav."), "{all}");
        // No match is an empty result group, not an error (it-tools behaviour).
        let none = run("http-status-codes", &[("query", "zzzz")]).unwrap();
        assert!(none.contains("no status code matches"), "{none}");
    }

    // ---- meta-tag-generator

    #[test]
    fn meta_tags_are_generated_and_escaped() {
        let out = run(
            "meta-tag-generator",
            &[
                ("title", "A & B"),
                ("type", "website"),
                ("twitter_card", "summary"),
            ],
        )
        .unwrap();
        assert!(out.contains("<title>A &amp; B</title>"), "{out}");
        assert!(
            out.contains("property=\"og:type\" content=\"website\""),
            "{out}"
        );
        assert!(
            out.contains("name=\"twitter:card\" content=\"summary\""),
            "{out}"
        );
        assert!(
            run(
                "meta-tag-generator",
                &[("title", ""), ("type", ""), ("twitter_card", "")]
            )
            .is_err()
        );
    }

    // ---- ipv4-address-converter (vectors from ipv4-address-converter.service.test.ts)

    #[test]
    fn ipv4_to_int_vectors() {
        for (ip, int) in [
            ("192.168.0.1", 3232235521u32),
            ("10.0.0.1", 167772161),
            ("255.255.255.255", 4294967295),
        ] {
            let out = run("ipv4-address-converter", &[("ip", ip)]).unwrap();
            assert!(out.contains(&format!("Decimal: {int}")), "{out}");
        }
        let out = run("ipv4-address-converter", &[("ip", "192.168.1.1")]).unwrap();
        assert!(out.contains("Hexadecimal: C0A80101"), "{out}");
        assert!(
            out.contains("Ipv6: 0000:0000:0000:0000:0000:ffff:c0a8:0101"),
            "{out}"
        );
        assert!(out.contains("Ipv6 (short): ::ffff:c0a8:0101"), "{out}");
    }

    #[test]
    fn invalid_ipv4_is_rejected() {
        for bad in [
            "256.168.0.1",
            "192.168.0",
            "192.168.0.1.2",
            "192.168.0.1.",
            ".192.168.0.1",
            "192.168.0.a",
            "",
            " ",
            "foo",
            "-1",
            "0",
        ] {
            assert!(
                run("ipv4-address-converter", &[("ip", bad)]).is_err(),
                "{bad} should be invalid"
            );
        }
        for good in ["192.168.0.1", "10.0.0.1"] {
            assert!(
                run("ipv4-address-converter", &[("ip", good)]).is_ok(),
                "{good} should be valid"
            );
        }
    }

    // ---- ipv4-range-expander (vectors from ipv4-range-expander.service.test.ts)

    #[test]
    fn range_expander_vectors() {
        let r = calculate_cidr(
            "192.168.1.1".parse().unwrap(),
            "192.168.7.255".parse().unwrap(),
        )
        .unwrap();
        assert_eq!(r.old_size, 1791);
        assert_eq!(r.new_size, 2048);
        assert_eq!(r.new_start.to_string(), "192.168.0.0");
        assert_eq!(r.new_end.to_string(), "192.168.7.255");
        assert_eq!(r.new_cidr, "192.168.0.0/21");

        let r = calculate_cidr("10.0.0.1".parse().unwrap(), "10.0.0.17".parse().unwrap()).unwrap();
        assert_eq!(r.old_size, 17);
        assert_eq!(r.new_size, 32);
        assert_eq!(r.new_start.to_string(), "10.0.0.0");
        assert_eq!(r.new_end.to_string(), "10.0.0.31");
        assert_eq!(r.new_cidr, "10.0.0.0/27");

        // The rendered table is old value -> new value, per it-tools.
        let out = run(
            "ipv4-range-expander",
            &[("start_ip", "192.168.1.1"), ("end_ip", "192.168.6.255")],
        )
        .unwrap();
        assert!(out.contains("old value  ->  new value"), "{out}");
        assert!(out.contains("192.168.1.1  ->  192.168.0.0"), "{out}");
        assert!(out.contains("Addresses in range"), "{out}");
        assert!(out.contains("192.168.0.0/21"), "{out}");

        // start > end has no result
        assert!(
            run(
                "ipv4-range-expander",
                &[("start_ip", "192.168.7.1"), ("end_ip", "192.168.6.255")]
            )
            .is_err()
        );
        assert!(
            run(
                "ipv4-range-expander",
                &[("start_ip", "nope"), ("end_ip", "10.0.0.1")]
            )
            .is_err()
        );
        // identical addresses collapse to /32
        let r = calculate_cidr("10.0.0.5".parse().unwrap(), "10.0.0.5".parse().unwrap()).unwrap();
        assert_eq!(r.new_cidr, "10.0.0.5/32");
        assert_eq!(r.new_size, 1);
        // full space
        let r = calculate_cidr(
            "0.0.0.0".parse().unwrap(),
            "255.255.255.255".parse().unwrap(),
        )
        .unwrap();
        assert_eq!(r.new_cidr, "0.0.0.0/0");
        assert_eq!(r.new_size, 1 << 32);
    }

    // ---- ipv4-subnet-calculator

    #[test]
    fn subnet_calculator_slash_24() {
        let out = run("ipv4-subnet-calculator", &[("ip", "192.168.0.1/24")]).unwrap();
        assert!(out.contains("Netmask: 192.168.0.0/24"), "{out}");
        assert!(out.contains("Network address: 192.168.0.0"), "{out}");
        assert!(out.contains("Network mask: 255.255.255.0"), "{out}");
        assert!(
            out.contains("Network mask in binary: 11111111.11111111.11111111.00000000"),
            "{out}"
        );
        assert!(out.contains("Wildcard mask: 0.0.0.255"), "{out}");
        assert!(out.contains("Network size: 256"), "{out}");
        assert!(out.contains("First address: 192.168.0.1"), "{out}");
        assert!(out.contains("Last address: 192.168.0.254"), "{out}");
        assert!(out.contains("Broadcast address: 192.168.0.255"), "{out}");
        assert!(out.contains("IP class: C"), "{out}");
    }

    #[test]
    fn subnet_calculator_edge_masks() {
        let s = parse_subnet("192.168.0.1/31").unwrap();
        assert_eq!(s.size, 2);
        assert_eq!(s.first.to_string(), "192.168.0.0");
        assert_eq!(s.last.to_string(), "192.168.0.1");
        assert!(s.broadcast.is_none());

        let s = parse_subnet("192.168.0.1/32").unwrap();
        assert_eq!(s.size, 1);
        assert_eq!(s.first.to_string(), "192.168.0.1");
        assert_eq!(s.last.to_string(), "192.168.0.1");
        assert!(s.broadcast.is_none());

        // No mask means /32, like netmask.js.
        assert_eq!(parse_subnet("10.0.0.1").unwrap().net.prefix_len(), 32);
        // Dotted netmasks are accepted too.
        assert_eq!(
            parse_subnet("10.0.0.1/255.255.255.0")
                .unwrap()
                .net
                .prefix_len(),
            24
        );
        // /30 keeps host reservation.
        let s = parse_subnet("10.0.0.0/30").unwrap();
        assert_eq!(
            (s.first.to_string(), s.last.to_string()),
            ("10.0.0.1".into(), "10.0.0.2".into())
        );
        assert_eq!(s.broadcast.unwrap().to_string(), "10.0.0.3");

        assert!(parse_subnet("10.0.0.1/33").is_err());
        assert!(parse_subnet("10.0.0.1/255.0.255.0").is_err());
        assert!(parse_subnet("garbage").is_err());
    }

    #[test]
    fn ip_classes() {
        assert_eq!(ip_class("10.0.0.0".parse().unwrap()), "A");
        assert_eq!(ip_class("172.16.0.0".parse().unwrap()), "B");
        assert_eq!(ip_class("192.168.0.0".parse().unwrap()), "C");
        assert_eq!(ip_class("224.0.0.1".parse().unwrap()), "D");
        assert_eq!(ip_class("250.0.0.1".parse().unwrap()), "E");
    }

    // ---- ipv6-ula-generator

    #[test]
    fn ula_generator_shape() {
        let out = run("ipv6-ula-generator", &[("mac", "20:37:06:12:34:56")]).unwrap();
        assert!(out.starts_with("IPv6 ULA: fd"), "{out}");
        assert!(out.contains("::/48"), "{out}");
        assert!(out.contains(":0::/64"), "{out}");
        assert!(out.contains(":ffff::/64"), "{out}");
        assert!(run("ipv6-ula-generator", &[("mac", "nope")]).is_err());
    }

    // ---- mac-address-generator (vectors from mac-adress-generator.models.test.ts)

    #[test]
    fn split_prefix_vectors() {
        assert!(split_prefix("").is_empty());
        assert_eq!(split_prefix("01"), ["01"]);
        assert_eq!(split_prefix("01:"), ["01"]);
        assert_eq!(split_prefix("01:23"), ["01", "23"]);
        assert_eq!(split_prefix("01-23"), ["01", "23"]);
        assert_eq!(split_prefix("0123"), ["01", "23"]);
        assert_eq!(split_prefix("012345"), ["01", "23", "45"]);
        assert_eq!(split_prefix("0123456"), ["01", "23", "45", "06"]);
    }

    #[test]
    fn mac_generator_respects_prefix_and_separator() {
        let out = run(
            "mac-address-generator",
            &[
                ("prefix", "ff:ee:aa"),
                ("separator", "-"),
                ("quantity", "3"),
                ("uppercase", "false"),
            ],
        )
        .unwrap();
        assert_eq!(out.lines().count(), 3);
        for line in out.lines() {
            assert!(line.starts_with("ff-ee-aa-"), "{line}");
            assert_eq!(line.split('-').count(), 6);
        }
        let out = run(
            "mac-address-generator",
            &[
                ("prefix", "ff:ee:a"),
                ("separator", ":"),
                ("uppercase", "true"),
                ("quantity", "1"),
            ],
        )
        .unwrap();
        assert!(out.starts_with("FF:EE:0A:"), "{out}");
        assert!(
            run(
                "mac-address-generator",
                &[
                    ("prefix", "0011223344556677"),
                    ("quantity", "1"),
                    ("separator", ":")
                ]
            )
            .is_err()
        );
        assert!(
            run(
                "mac-address-generator",
                &[("quantity", "0"), ("separator", ":"), ("prefix", "")]
            )
            .is_err()
        );
    }

    // ---- mac-address-lookup

    #[test]
    fn mac_lookup_known_and_unknown() {
        let out = run("mac-address-lookup", &[("mac", "00:0C:29:12:34:56")]).unwrap();
        assert!(out.contains("VMware"), "{out}");
        // Separator-insensitive and case-insensitive.
        assert!(
            run("mac-address-lookup", &[("mac", "b8-27-eb-11-22-33")])
                .unwrap()
                .contains("Raspberry Pi")
        );
        assert!(
            run("mac-address-lookup", &[("mac", "00000c112233")])
                .unwrap()
                .contains("Cisco")
        );
        // A miss is a message, not an error.
        assert!(
            run("mac-address-lookup", &[("mac", "FF:FF:FF:FF:FF:FF")])
                .unwrap()
                .contains("Unknown vendor for this address")
        );
        assert!(run("mac-address-lookup", &[("mac", "zz")]).is_err());
        // The table itself has no duplicate prefixes.
        let mut p: Vec<_> = OUI.iter().map(|(p, _)| *p).collect();
        let n = p.len();
        p.sort_unstable();
        p.dedup();
        assert_eq!(p.len(), n, "duplicate OUI prefixes");
    }

    // ---- random-port-generator

    #[test]
    fn random_ports_are_in_range() {
        let out = run("random-port-generator", &[("quantity", "20")]).unwrap();
        assert_eq!(out.lines().count(), 20);
        for line in out.lines() {
            let p: u32 = line.parse().unwrap();
            assert!((1024..=65535).contains(&p), "{p}");
        }
        assert!(run("random-port-generator", &[("quantity", "9999")]).is_err());
    }

    // ---- iban-validator-and-parser

    #[test]
    fn iban_valid_and_invalid() {
        let out = run(
            "iban-validator-and-parser",
            &[("iban", "DE44 5001 0517 5407 3249 31")],
        )
        .unwrap();
        assert!(out.contains("Is IBAN valid ?: true"), "{out}");
        assert!(
            out.contains("IBAN friendly format: DE44 5001 0517 5407 3249 31"),
            "{out}"
        );
        assert!(out.contains("Is IBAN a QR-IBAN ?: false"), "{out}");
        assert!(out.contains("Country code: DE"), "{out}");
        assert!(out.contains("BBAN: 500105175407324931"), "{out}");

        let out = run(
            "iban-validator-and-parser",
            &[("iban", "FR7630006000011234567890189")],
        )
        .unwrap();
        assert!(out.contains("Country code: FR"), "{out}");

        // Broken checksum, unknown country and junk all still fill the rows —
        // it-tools reports validity, it does not blank the panel.
        for bad in [
            "DE44500105175407324932",
            "MR0000020001010000123456754",
            "not-an-iban",
        ] {
            let out = run("iban-validator-and-parser", &[("iban", bad)]).unwrap();
            assert!(out.contains("Is IBAN valid ?: false"), "{bad}: {out}");
            assert!(out.contains("IBAN errors: "), "{bad}: {out}");
            assert!(out.contains("IBAN friendly format: "), "{bad}: {out}");
        }
        assert!(run("iban-validator-and-parser", &[("iban", "")]).is_err());
        // QR-IBAN: a CH IBAN with a QR-IID institution number.
        assert!(
            run(
                "iban-validator-and-parser",
                &[("iban", "CH4431999123000889012")]
            )
            .unwrap()
            .contains("Is IBAN a QR-IBAN ?: true")
        );
    }

    // ---- phone-parser-and-formatter

    #[test]
    fn phone_parser_subset() {
        let out = run(
            "phone-parser-and-formatter",
            &[("phone", "+49 30 1234 5678"), ("default_calling_code", "")],
        )
        .unwrap();
        assert!(out.contains("E.164 format: +493012345678"), "{out}");
        assert!(out.contains("RFC3966 format: tel:+493012345678"), "{out}");
        assert!(out.contains("Country calling code: 49"), "{out}");
        assert!(out.contains("Country: Germany"), "{out}");
        assert!(out.contains("National number: 3012345678"), "{out}");
        assert!(out.contains("Is valid?: Yes"), "{out}");

        // 00 prefix works like +
        assert!(
            run(
                "phone-parser-and-formatter",
                &[("phone", "0033123456789"), ("default_calling_code", "")]
            )
            .unwrap()
            .contains("Country: France")
        );
        assert!(
            run(
                "phone-parser-and-formatter",
                &[("phone", "030 12345678"), ("default_calling_code", "49")]
            )
            .unwrap()
            .contains("E.164 format: +493012345678")
        );
        // National format needs the default code
        assert!(
            run(
                "phone-parser-and-formatter",
                &[("phone", "030 12345678"), ("default_calling_code", "")]
            )
            .is_err()
        );
        // Longest-prefix match beats the shorter one.
        assert!(
            run(
                "phone-parser-and-formatter",
                &[("phone", "+420123456789"), ("default_calling_code", "")]
            )
            .unwrap()
            .contains("Czechia")
        );
        assert!(
            run(
                "phone-parser-and-formatter",
                &[("phone", "+99900011122"), ("default_calling_code", "")]
            )
            .is_err()
        );
        assert!(
            run(
                "phone-parser-and-formatter",
                &[("phone", ""), ("default_calling_code", "")]
            )
            .is_err()
        );
    }

    // ---- email-normalizer

    #[test]
    fn email_normalizer_lines() {
        let input =
            "John.Doe+spam@Gmail.com\nfoo.bar@googlemail.com\nA.B+x@example.com\nnot-an-email";
        let out = run("email-normalizer", &[("emails", input)]).unwrap();
        let lines: Vec<_> = out.lines().collect();
        assert_eq!(lines[0], "johndoe@gmail.com");
        assert_eq!(lines[1], "foobar@gmail.com");
        assert_eq!(lines[2], "a.b+x@example.com");
        assert_eq!(lines[3], "Unable to parse email: not-an-email");
        assert!(run("email-normalizer", &[("emails", "   ")]).is_err());
    }

    // ---- docker-run-to-docker-compose-converter

    #[test]
    fn docker_run_conversion() {
        let out = run("docker-run-to-docker-compose-converter", &[(
            "command",
            "docker run -p 80:80 -v /var/run/docker.sock:/tmp/docker.sock:ro --restart always --log-opt max-size=1g nginx",
        )])
        .unwrap();
        assert!(out.contains("services:"), "{out}");
        assert!(out.contains("nginx:"), "{out}");
        assert!(out.contains("image: nginx"), "{out}");
        assert!(out.contains("80:80"), "{out}");
        assert!(
            out.contains("/var/run/docker.sock:/tmp/docker.sock:ro"),
            "{out}"
        );
        assert!(out.contains("restart: always"), "{out}");
        // serde_norway emits plain scalars where the shim quoted them
        // (`max-size: 1g`, `image: postgres:16`); both are valid YAML.
        assert!(out.contains("max-size: 1g"), "{out}");

        let out = run("docker-run-to-docker-compose-converter", &[(
            "command",
            "docker run -d --name db -e POSTGRES_PASSWORD=secret -p 5432:5432 postgres:16 postgres -c log_statement=all",
        )])
        .unwrap();
        assert!(out.contains("db:"), "{out}");
        assert!(out.contains("container_name: db"), "{out}");
        assert!(out.contains("POSTGRES_PASSWORD=secret"), "{out}");
        assert!(out.contains("image: postgres:16"), "{out}");
        assert!(out.contains("command:"), "{out}");

        assert!(
            run(
                "docker-run-to-docker-compose-converter",
                &[("command", "docker run")]
            )
            .is_err()
        );
        assert!(
            run(
                "docker-run-to-docker-compose-converter",
                &[("command", "kubectl get pods")]
            )
            .is_err()
        );
        assert!(
            run(
                "docker-run-to-docker-compose-converter",
                &[("command", "docker run -e 'unclosed nginx")]
            )
            .is_err()
        );
    }

    #[test]
    fn shell_split_handles_quotes() {
        assert_eq!(shell_split("a  b\tc").unwrap(), ["a", "b", "c"]);
        assert_eq!(shell_split("-e 'A=b c'").unwrap(), ["-e", "A=b c"]);
        assert_eq!(shell_split("-e \"A=b c\"").unwrap(), ["-e", "A=b c"]);
        assert_eq!(shell_split("''").unwrap(), [""]);
        assert!(shell_split("'oops").is_err());
    }

    // ---- crontab-generator

    #[test]
    fn crontab_describes_and_rejects() {
        let out = run("crontab-generator", &[("expression", "40 * * * *")]).unwrap();
        assert!(out.contains("Expression: 40 * * * *"), "{out}");
        assert!(out.contains("Meaning:"), "{out}");
        // it-tools always shows the field diagram and the symbol table.
        assert!(out.contains("day of week (0 - 6, sunday=0)"), "{out}");
        assert!(out.contains("@midnight"), "{out}");
        assert_eq!(
            out.lines().filter(|l| l.starts_with("  2")).count(),
            5,
            "{out}"
        );
        assert!(run("crontab-generator", &[("expression", "* * * *")]).is_err());
        assert!(run("crontab-generator", &[("expression", "99 * * * *")]).is_err());
        assert!(run("crontab-generator", &[("expression", "")]).is_err());
    }

    // ---- chmod-calculator (vectors from chmod-calculator.service.test.ts)

    /// Tick the checkboxes the way the grid would for this octal triple.
    fn chmod_grid(octal: &str) -> Args {
        let mut a = Args::new();
        for (digit, group) in octal.chars().zip(CHMOD_GROUPS) {
            let d = digit.to_digit(8).unwrap() as u8;
            for (scope, bit) in CHMOD_SCOPES {
                a.insert(format!("{group}_{scope}"), (d & bit != 0).to_string());
            }
        }
        a
    }

    #[test]
    fn chmod_octal_symbolic_round_trip() {
        let t = tool("chmod-calculator");
        for (octal, symbolic) in [
            ("777", "rwxrwxrwx"),
            ("000", "---------"),
            ("235", "-w--wxr-x"),
            ("421", "r---w---x"),
            ("124", "--x-w-r--"),
            ("222", "-w--w--w-"),
            ("755", "rwxr-xr-x"),
        ] {
            let out = (t.run)(&chmod_grid(octal)).unwrap();
            assert!(out.contains(&format!("Octal: {octal}")), "{out}");
            assert!(out.contains(&format!("Symbolic: {symbolic}")), "{out}");
            assert!(
                out.contains(&format!("Command: chmod {octal} path")),
                "{out}"
            );
        }
        // An untouched grid is 000, never an error — there is nothing to parse.
        let empty = (t.run)(&Args::new()).unwrap();
        assert!(empty.contains("Octal: 000"), "{empty}");
        assert!(empty.contains("Symbolic: ---------"), "{empty}");
    }

    // ---- slugify-string

    #[test]
    fn slugify_vectors() {
        assert_eq!(
            run("slugify-string", &[("input", "My file path")]).unwrap(),
            "my-file-path"
        );
        assert_eq!(
            run("slugify-string", &[("input", "Héllo Wörld! ")]).unwrap(),
            "hello-world"
        );
        assert!(run("slugify-string", &[("input", "   ")]).is_err());
        // slug maps unknown scalars away; the tool refuses an empty slug.
        assert!(run("slugify-string", &[("input", "---")]).is_err());
    }

    // ---- device-information

    #[test]
    fn device_information_is_custom() {
        let t = TOOLS.iter().find(|t| t.id == "device-information").unwrap();
        assert_eq!(t.kind, Kind::Custom);
        assert!((t.run)(&Args::new()).is_err());
    }
}
