//! `http-status-codes`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::Result;
use std::fmt::Write as _;

/// `(code, name, description, kind)` — ported verbatim from
/// `http-status-codes.constants.ts`.
pub(crate) static STATUS_CODES: &[(u16, &str, &str, &str)] = &[
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
pub(crate) fn status_category(code: u16) -> &'static str {
    match code / 100 {
        1 => "1xx informational response",
        2 => "2xx success",
        3 => "3xx redirection",
        4 => "4xx client error",
        _ => "5xx server error",
    }
}

pub(crate) fn http_status_codes(args: &Args) -> Result<String> {
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

pub static TOOL: Tool = Tool {
    id: "http-status-codes",
    name: "HTTP status codes",
    description: "The list of all HTTP status codes, their name and their meaning.",
    category: Category::Web,
    keywords: &["http", "status", "codes", "rest", "response"],
    fields: &[Field::text("query", "Search a code, name or description")],
    kind: Kind::Pure,
    run: http_status_codes,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

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
}
