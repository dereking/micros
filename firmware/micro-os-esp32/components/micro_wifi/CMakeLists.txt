# Real ESP32 STA Wi-Fi + minimal HTTP backend for the shell's `net.*` host
# calls. Bridges the async esp_wifi events to the sync host reads; owns the
# radio, the shared state, and the NVS-persisted network. micro_http.c does a
# plain lwip-socket HTTP/1.0 GET (no TLS) so it stays lean.
idf_component_register(
  SRCS "micro_wifi.c" "micro_http.c"
  INCLUDE_DIRS "include"
  REQUIRES esp_wifi esp_netif esp_event nvs_flash lwip
)
