# Real ESP32 STA Wi-Fi + HTTP(S) backend for the shell's `net.*` host calls.
# Bridges the async esp_wifi events to the sync host reads; owns the radio, the
# shared state, and the NVS-persisted network. micro_http.c uses
# esp_http_client + the mbedTLS certificate bundle so App sync/dictionary calls
# get real HTTPS and custom headers without an external command.
idf_component_register(
  SRCS "micro_wifi.c" "micro_http.c"
  INCLUDE_DIRS "include"
  REQUIRES esp_wifi esp_netif esp_event nvs_flash lwip esp_http_client mbedtls
)
