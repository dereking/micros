# Third-party source notice

## Waveshare / Spotpear ESP32-S3-Touch-LCD-7 demo

- Canonical archive: <https://files.waveshare.net/wiki/ESP32-S3-Touch-LCD-7/ESP32-S3-Touch-LCD-7-Demo.zip>
- Archive name: `ESP32-S3-Touch-LCD-7-Demo.zip`
- SHA-256: `5351d443eaa605cab1eb80d050d867c18e1ce2b33c9cbc78aae1b7bca040b038`
- Retrieved: 2026-08-13
- Reference example: `ESP32-S3-Touch-LCD-7-Demo/ESP-IDF/08_lvgl_Porting`
- Hardware identity: ESP32-S3-Touch-LCD-7 schematic V1.2 with an
  ESP32-S3-WROOM-1-N8R8 module.

The example source headers identify these licenses:

- `main/main.c`: CC0-1.0
- `main/waveshare_rgb_lcd_port.c`: CC0-1.0
- `main/lvgl_port.c`: Apache-2.0

The archive and extracted example are local reference material only. They are
downloaded under the ignored `work/vendor/` tree and are not retained in this
repository. Micro OS targets LVGL 9; the upstream LVGL 8 port is reference-only
and must not become a vendored implementation. If upstream code is later copied
or adapted, preserve its applicable license and attribution in the committed
source and update this notice.

`firmware/micro-os-esp32/components/micro_bsp_lcd7/` adapts the board-specific
RGB timing and CH422G/GT911 reset sequence from the CC0-1.0 reference while
using ESP-IDF 5.5 and LVGL 9 APIs. Its CC0 notice is retained in that component.

Official resources:

- Waveshare documentation: <https://www.waveshare.com/wiki/ESP32-S3-Touch-LCD-7>
- Waveshare resource index: <https://docs.waveshare.com/ESP32-S3-Touch-LCD-7/Resources-And-Documents>
- Spotpear documentation: <https://spotpear.com/wiki/ESP32-S3N8R8-7inch-LCD-Display-TouchScreen-800x480-LVGL-CAN-Sensor-RS485.html>
- Schematic V1.2: <https://files.waveshare.net/wiki/ESP32-S3-Touch-LCD-7/ESP32-S3-Touch-LCD-7-Sch.pdf>
