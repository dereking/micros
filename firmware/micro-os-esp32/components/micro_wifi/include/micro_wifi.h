#ifndef MICRO_WIFI_H
#define MICRO_WIFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Initializes the ESP32 STA radio: default NVS, netif, default event loop,
 * esp_wifi init, STA mode, event handlers, and an auto-reconnect to the network
 * persisted by a previous successful connect. Idempotent; call once at boot.
 * The default NVS partition ("nvs") is separate from the BSP's micro_cfg NVS
 * partition, so both initialize independently. */
void micro_wifi_init(void);

/* Starts an asynchronous active scan. The result is delivered through
 * micro_wifi_take_scan_result once WIFI_EVENT_SCAN_DONE fires. Safe to call
 * from any task; the scan runs on the wifi task. */
void micro_wifi_start_scan(void);

/* Returns 1 and fills buf with the fresh scan result (one "SSID, -rssi dBm"
 * line per AP) if a scan completed since the last read, 0 when none is ready,
 * and a negative value on a NULL/empty buf. The ready flag clears on read. */
int micro_wifi_take_scan_result(char *buf, size_t cap);

/* Copies the SSID of the AP at scan index `index` ("" out of range or when no
 * scan has completed). Thread-safe (spinlock-guarded). */
int micro_wifi_ap_name(uint32_t index, char *buf, size_t cap);

/* Connects the STA radio to the given network. Credentials are persisted to
 * the default NVS partition on a successful connect so a reboot auto-reconnects
 * (driven from WIFI_EVENT_STA_START in the event handler). */
void micro_wifi_connect(const char *ssid, const char *pass);

/* Disconnects the radio and forgets the persisted network. */
void micro_wifi_disconnect(void);

/* Copies the current radio state ("off" | "connecting" | "connected" |
 * "error") into buf. Thread-safe (spinlock-guarded). */
int micro_wifi_state(char *buf, size_t cap);

/* Copies the SSID of the network currently connected to / being connected to. */
int micro_wifi_ssid(char *buf, size_t cap);

#ifdef __cplusplus
}
#endif

#endif
