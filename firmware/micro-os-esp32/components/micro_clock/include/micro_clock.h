/* Wall-clock time for the App `clock.now()` host call.
 *
 * SNTP runs once at boot and keeps syncing in the background once Wi-Fi is
 * connected. Before the first successful sync the clock reports 0 so the Rust
 * host can fall back to monotonic uptime (better than a 1970 timestamp for
 * scheduling).
 */
#pragma once

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

void micro_clock_init(void);

/* Unix epoch milliseconds, or 0 when SNTP has not synced yet. */
uint64_t micro_clock_wall_ms(void);

#ifdef __cplusplus
}
#endif
