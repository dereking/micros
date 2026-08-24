# Real ESP32 STA Wi-Fi backend for the shell's `net.*` host calls. Bridges the
# async esp_wifi events to the sync host reads; owns the radio, the shared
# state, and the NVS-persisted network.
idf_component_register(
  SRCS "micro_wifi.c"
  INCLUDE_DIRS "include"
  REQUIRES esp_wifi esp_netif esp_event nvs_flash
)
