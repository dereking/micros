//! OS-level HTTP request/response primitives shared by the native hosts.
//!
//! The runtime exposes `net.*` to Apps, but the transport lives here so a host
//! never has to shell out to an external command. This crate owns the parts
//! that must behave identically on every host:
//!
//! * request validation (method / URL / header block / body size),
//! * the `"HTTP <status>\n<body>"` callback envelope,
//! * the Maimemo-facing dictionary lookup, which is a batch of HTTPS calls.
//!
//! The actual I/O is a [`Transport`]: the macOS host uses
//! [`native::NativeTransport`] (ureq + rustls), while the ESP32 host calls the
//! firmware's `esp_http_client` wrapper (TLS certificate bundle included).
//! Keeping the platform transport behind one trait also means the validation
//! and dictionary behavior are covered by host-agnostic tests.

pub mod dictionary;

#[cfg(feature = "native")]
pub mod native;

/// Largest request body a host accepts. Maimemo's JSON payloads are far below
/// this; the cap protects the embedded heap from a runaway App.
pub const MAX_REQUEST_BODY_BYTES: usize = 256 * 1024;

/// Largest URL accepted before handing it to the platform transport.
pub const MAX_URL_BYTES: usize = 4096;

/// Largest newline-delimited header block accepted.
pub const MAX_HEADER_BLOCK_BYTES: usize = 8 * 1024;

/// Largest number of headers in one request.
pub const MAX_HEADERS: usize = 32;

/// Largest response body read by the native transport.
pub const MAX_RESPONSE_BYTES: usize = 256 * 1024;

/// A validated HTTP request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    /// Uppercase HTTP method token (`GET`, `POST`, ...).
    pub method: String,
    /// Absolute `http://` or `https://` URL.
    pub url: String,
    /// Validated header name/value pairs in insertion order.
    pub headers: Vec<(String, String)>,
    /// Request body. Empty means "no body".
    pub body: String,
}

/// A raw HTTP response. `status == 0` means the transport failed and `body`
/// carries the human-readable error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    #[must_use]
    pub fn new(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            body: body.into(),
        }
    }
}

/// Platform I/O boundary. Implementations must be synchronous; hosts call them
/// from a worker thread so the LVGL loop never blocks.
pub trait Transport {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, String>;
}

impl HttpRequest {
    /// Validates and assembles a request from the raw arguments the App passed
    /// to `net.httpRequestWithHeaders` (or the older `net.httpGet` /
    /// `net.httpRequest` calls, which pass an empty header block).
    pub fn parse(method: &str, url: &str, headers: &str, body: &str) -> Result<Self, String> {
        validate_method(method)?;
        validate_url(url)?;
        if body.len() > MAX_REQUEST_BODY_BYTES {
            return Err(format!(
                "request body is too large ({} bytes, limit {MAX_REQUEST_BODY_BYTES})",
                body.len()
            ));
        }
        let headers = parse_header_block(headers)?;
        Ok(Self {
            method: method.to_owned(),
            url: url.to_owned(),
            headers,
            body: body.to_owned(),
        })
    }
}

/// Formats the callback envelope the App expects. A failed transport uses
/// status `0`; HTTP error statuses keep their real code and body.
#[must_use]
pub fn envelope(status: u16, body: &str) -> String {
    let mut out = String::with_capacity(16 + body.len());
    out.push_str("HTTP ");
    out.push_str(&status.to_string());
    out.push('\n');
    out.push_str(body);
    out
}

/// Executes an already-validated request and envelopes the result.
pub fn perform_request(transport: &dyn Transport, request: &HttpRequest) -> String {
    match transport.execute(request) {
        Ok(response) => envelope(response.status, &response.body),
        Err(error) => envelope(0, &error),
    }
}

/// Validates, executes, and envelopes one request. This is the single entry
/// point both native hosts use, so an invalid request fails the same way
/// everywhere.
pub fn perform(
    transport: &dyn Transport,
    method: &str,
    url: &str,
    headers: &str,
    body: &str,
) -> String {
    match HttpRequest::parse(method, url, headers, body) {
        Ok(request) => perform_request(transport, &request),
        Err(error) => envelope(0, &error),
    }
}

fn validate_method(method: &str) -> Result<(), String> {
    if method.is_empty() {
        return Err("HTTP method is empty".into());
    }
    if method.len() > 16 {
        return Err("HTTP method is too long".into());
    }
    if !method.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(format!("invalid HTTP method: {method}"));
    }
    Ok(())
}

fn validate_url(url: &str) -> Result<(), String> {
    if url.is_empty() {
        return Err("URL is empty".into());
    }
    if url.len() > MAX_URL_BYTES {
        return Err(format!("URL is too long (limit {MAX_URL_BYTES} bytes)"));
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("only http:// and https:// URLs are supported".into());
    }
    if url.bytes().any(|byte| byte <= b' ' || byte == 0x7f) {
        return Err("URL contains whitespace or control characters".into());
    }
    Ok(())
}

/// Parses a newline-delimited header block such as
/// `"Authorization: Bearer …\nAccept: application/json"`.
///
/// Empty lines are ignored, CRLF is accepted, and header values are trimmed.
/// Hop-by-hop headers and `Content-Length`/`Host` are rejected because the
/// transport owns framing; allowing them would let an App desynchronize the
/// request or smuggle a second message.
pub fn parse_header_block(block: &str) -> Result<Vec<(String, String)>, String> {
    if block.len() > MAX_HEADER_BLOCK_BYTES {
        return Err(format!(
            "header block is too large ({} bytes, limit {MAX_HEADER_BLOCK_BYTES})",
            block.len()
        ));
    }
    let mut headers = Vec::new();
    for raw_line in block.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            return Err("folded header lines are not supported".into());
        }
        let line = line.trim();
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| format!("header is missing a colon: {line}"))?;
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() {
            return Err("header name is empty".into());
        }
        if name.len() > 64 {
            return Err(format!("header name is too long: {name}"));
        }
        if !name.bytes().all(is_token_byte) {
            return Err(format!("invalid header name: {name}"));
        }
        if value
            .bytes()
            .any(|byte| byte == b'\r' || byte == b'\n' || byte == 0 || byte == 0x7f)
        {
            return Err(format!("invalid header value for {name}"));
        }
        if is_transport_header(name) {
            return Err(format!("header {name} is managed by the HTTP transport"));
        }
        if headers.len() == MAX_HEADERS {
            return Err(format!("too many headers (limit {MAX_HEADERS})"));
        }
        headers.push((name.to_owned(), value.to_owned()));
    }
    Ok(headers)
}

fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn is_transport_header(name: &str) -> bool {
    const MANAGED: [&str; 9] = [
        "host",
        "content-length",
        "connection",
        "transfer-encoding",
        "expect",
        "upgrade",
        "proxy-connection",
        "te",
        "trailer",
    ];
    MANAGED
        .iter()
        .any(|managed| name.eq_ignore_ascii_case(managed))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;

    impl Transport for Echo {
        fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
            assert_eq!(request.method, "POST");
            assert_eq!(request.url, "https://example.test/probe");
            assert_eq!(request.body, "{\"probe\":true}");
            assert!(
                request
                    .headers
                    .iter()
                    .any(|(name, value)| name == "Authorization" && value == "Bearer token")
            );
            Ok(HttpResponse::new(200, "OK"))
        }
    }

    #[test]
    fn performs_a_validated_request_and_formats_the_envelope() {
        let response = perform(
            &Echo,
            "POST",
            "https://example.test/probe",
            "Authorization: Bearer token\nAccept: application/json",
            "{\"probe\":true}",
        );
        assert_eq!(response, "HTTP 200\nOK");
    }

    #[test]
    fn rejects_lowercase_methods_and_non_http_urls() {
        assert_eq!(
            perform(&Echo, "post", "https://example.test/", "", ""),
            "HTTP 0\ninvalid HTTP method: post"
        );
        assert_eq!(
            perform(&Echo, "GET", "file:///etc/passwd", "", ""),
            "HTTP 0\nonly http:// and https:// URLs are supported"
        );
    }

    #[test]
    fn rejects_header_injection_and_transport_headers() {
        let error = parse_header_block("X-Test: ok\rInjected: yes").unwrap_err();
        assert!(error.contains("invalid header value"), "{error}");

        let error = parse_header_block("Authorization: ok\rInjected: yes").unwrap_err();
        assert!(error.contains("invalid header value"), "{error}");

        let error = parse_header_block("Content-Length: 10").unwrap_err();
        assert!(error.contains("managed by the HTTP transport"), "{error}");

        let error = parse_header_block("Bad Name: value").unwrap_err();
        assert!(error.contains("invalid header name"), "{error}");

        let error = parse_header_block("Accept: application/json\n  folded: value").unwrap_err();
        assert!(error.contains("folded header"), "{error}");

        let error = parse_header_block(&format!("X-Test: {}", "a".repeat(9 * 1024))).unwrap_err();
        assert!(error.contains("header block is too large"), "{error}");
    }

    #[test]
    fn accepts_crlf_blank_lines_and_trims_values() {
        let headers =
            parse_header_block("Authorization:  Bearer token \r\n\r\nAccept: application/json\r\n")
                .expect("headers");
        assert_eq!(
            headers,
            vec![
                ("Authorization".into(), "Bearer token".into()),
                ("Accept".into(), "application/json".into()),
            ]
        );
    }

    #[test]
    fn rejects_oversized_bodies() {
        let body = "x".repeat(MAX_REQUEST_BODY_BYTES + 1);
        let error = HttpRequest::parse("POST", "https://example.test/", "", &body).unwrap_err();
        assert!(error.contains("too large"), "{error}");
    }
}
