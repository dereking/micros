#ifndef MICRO_LV_CONF_H
#define MICRO_LV_CONF_H

#define LV_COLOR_DEPTH 32
#define LV_MEM_SIZE (256U * 1024U)
#define LV_USE_LOG 1
#define LV_LOG_LEVEL LV_LOG_LEVEL_WARN
/* micro_ui_sans_* are LVGL-9 RLE-compressed fonts (bitmap_format=1); without
 * this LVGL returns NULL glyphs and text renders invisible. */
#define LV_USE_FONT_COMPRESSED 1
#define LV_USE_ASSERT_NULL 1
#define LV_USE_ASSERT_MALLOC 1
#define LV_USE_OS LV_OS_NONE

#endif
