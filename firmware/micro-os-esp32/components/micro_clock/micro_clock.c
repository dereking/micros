#include "micro_clock.h"

#include <sys/time.h>
#include <time.h>

#include "esp_log.h"
#include "esp_sntp.h"

static const char *TAG = "micro_clock";

/* 2020-01-01T00:00:00Z: anything earlier means SNTP has not set the clock. */
#define MICRO_CLOCK_VALID_FROM_SEC 1577836800LL

void micro_clock_init(void)
{
    esp_sntp_setoperatingmode(ESP_SNTP_OPMODE_POLL);
    esp_sntp_setservername(0, "pool.ntp.org");
    esp_sntp_init();
    ESP_LOGI(TAG, "SNTP started (pool.ntp.org)");
}

uint64_t micro_clock_wall_ms(void)
{
    struct timeval tv;
    if (gettimeofday(&tv, NULL) != 0) {
        return 0;
    }
    if ((long long)tv.tv_sec < MICRO_CLOCK_VALID_FROM_SEC) {
        return 0;
    }
    return (uint64_t)tv.tv_sec * 1000ULL + (uint64_t)tv.tv_usec / 1000ULL;
}
