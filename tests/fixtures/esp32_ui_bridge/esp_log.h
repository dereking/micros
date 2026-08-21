#ifndef TEST_ESP_LOG_H
#define TEST_ESP_LOG_H
void test_log_warning(const char *tag, const char *format, ...);
#define ESP_LOGW(tag, format, ...) test_log_warning(tag, format, ##__VA_ARGS__)
#endif
