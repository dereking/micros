#ifndef MICRO_HTTP_H
#define MICRO_HTTP_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Blocking HTTP/HTTPS client for the App `net.*` host calls.
 *
 * Runs on IDF's `esp_http_client` (TLS through mbedTLS + the configured
 * certificate bundle) instead of a raw lwip socket or an external command, so
 * `https://` URLs, custom request headers, redirects, chunked responses and
 * 4xx/5xx response bodies all work. Call it from a worker task; it blocks until
 * the request finishes or times out.
 *
 * `method` is an uppercase token (GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS).
 * `headers` is a newline-delimited "Name: value" block (already validated by
 * the Rust host); pass NULL/0 for no headers. `body` is a byte buffer and does
 * not need a trailing NUL.
 *
 * On return:
 *   - 0 means `*response` is a heap buffer of `*response_len` bytes (always
 *     NUL-terminated for logging) and `*status` is the HTTP status. A status of
 *     0 means the transport failed and the buffer contains a short error text.
 *   - -1 means the output could not be produced (bad arguments / OOM).
 * Free a non-NULL response with micro_http_free().
 */
int micro_http_request(const char *method, size_t method_len,
                       const char *url, size_t url_len,
                       const char *headers, size_t headers_len,
                       const char *body, size_t body_len,
                       int *status, char **response, size_t *response_len);

/* Frees a response returned by micro_http_request(). NULL is a no-op. */
void micro_http_free(char *response);

#ifdef __cplusplus
}
#endif

#endif
