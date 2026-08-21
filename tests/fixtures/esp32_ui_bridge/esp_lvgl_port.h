#ifndef TEST_ESP_LVGL_PORT_H
#define TEST_ESP_LVGL_PORT_H
#include <stdbool.h>
#include <stdint.h>
bool lvgl_port_lock(uint32_t timeout_ms);
void lvgl_port_unlock(void);
#endif
