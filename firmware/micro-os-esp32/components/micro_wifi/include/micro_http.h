#ifndef MICRO_HTTP_H
#define MICRO_HTTP_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Starts an async HTTP/1.0 request over the connected STA link (raw lwip
 * socket; no TLS, so http:// URLs only). `method` is an uppercase token
 * (GET/POST/PUT/DELETE/HEAD/…); `body` is the request body ("" for GET/HEAD/
 * DELETE). Returns 0 when the request was accepted, -1 when it was rejected
 * (busy or malformed). The result is delivered through micro_http_take_result
 * once the response completes. Safe to call from any task; the request runs on
 * its own FreeRTOS task. */
int micro_http_request(const char *method, size_t method_len,
                       const char *url, size_t url_len,
                       const char *body, size_t body_len);

/* Convenience: a GET with no body. */
int micro_http_get(const char *url, size_t len);

/* Returns 1 and fills buf with the fresh result ("HTTP <status>\n<body>",
 * body truncated to cap) if a request completed since the last read, 0 when
 * none is ready, and a negative value on a NULL/empty buf. Clears on read. */
int micro_http_take_result(char *buf, size_t cap);

#ifdef __cplusplus
}
#endif

#endif
