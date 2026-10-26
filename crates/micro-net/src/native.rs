//! Native (macOS) HTTP transport: ureq with rustls and Mozilla's root bundle.
//!
//! This deliberately does not spawn `curl` or any other process. The request
//! runs synchronously on the caller's worker thread; ureq handles HTTPS,
//! redirects, chunked responses and gzip. `http_status_as_error(false)` keeps
//! 4xx/5xx bodies available so the App can show the API's error message.

use std::time::Duration;

use crate::{HttpRequest, HttpResponse, MAX_RESPONSE_BYTES, Transport};

/// Blocking native transport. Cheap to construct per worker; an `Agent` is
/// reused for the whole dictionary batch.
pub struct NativeTransport {
    agent: ureq::Agent,
}

impl Default for NativeTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeTransport {
    #[must_use]
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .user_agent("micros-os/1.0")
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl Transport for NativeTransport {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
        let method = ureq::http::Method::from_bytes(request.method.as_bytes())
            .map_err(|error| format!("invalid HTTP method: {error}"))?;
        let mut builder = ureq::http::Request::builder()
            .method(method)
            .uri(request.url.as_str());
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }

        let mut response = if request.body.is_empty() {
            self.agent
                .run(builder.body(()).map_err(|error| error.to_string())?)
        } else {
            self.agent.run(
                builder
                    .body(request.body.as_bytes())
                    .map_err(|error| error.to_string())?,
            )
        }
        .map_err(|error| error.to_string())?;

        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES as u64)
            .read_to_string()
            .map_err(|error| error.to_string())?;
        Ok(HttpResponse::new(status, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn sends_headers_and_body_without_an_external_command() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0_u8; 4096];
            let mut request = String::new();
            loop {
                let read = stream.read(&mut buffer).expect("read");
                if read == 0 {
                    break;
                }
                request.push_str(&String::from_utf8_lossy(&buffer[..read]));
                if request.contains("{\"probe\":true}") {
                    break;
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK")
                .expect("write");
            request
        });

        let response = crate::perform(
            &NativeTransport::new(),
            "POST",
            &format!("http://{address}/probe"),
            "Authorization: Bearer test-token\nAccept: application/json",
            "{\"probe\":true}",
        );
        let request = server.join().expect("server");
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer test-token"),
            "{request}"
        );
        assert!(request.contains("{\"probe\":true}"), "{request}");
        assert_eq!(response, "HTTP 200\nOK");
    }

    #[test]
    fn error_status_keeps_the_response_body() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer);
            stream
                .write_all(
                    b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 18\r\nConnection: close\r\n\r\n{\"error\":\"denied\"}",
                )
                .expect("write");
        });

        let response = crate::perform(
            &NativeTransport::new(),
            "POST",
            &format!("http://{address}/probe"),
            "",
            "{}",
        );
        server.join().expect("server");
        assert_eq!(response, "HTTP 401\n{\"error\":\"denied\"}");
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    /// Manual smoke test for the real HTTPS path (webpki roots, no `curl`).
    /// Run with: `cargo test -p micro-net --features native -- --ignored`.
    #[test]
    #[ignore = "requires network access"]
    fn https_dictionary_lookup_reaches_youdao() {
        let response = crate::dictionary::lookup_words(&NativeTransport::new(), "abandon");
        assert!(response.starts_with("HTTP 200\n"), "{response}");
        assert!(response.contains("\"word\":\"abandon\""), "{response}");
        assert!(response.contains("\"definition\":\""), "{response}");
    }
}
