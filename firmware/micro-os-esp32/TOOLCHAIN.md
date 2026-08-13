# ESP32-S3 Micro OS toolchain

## Pinned versions

- ESP-IDF 5.5.4
- Rust target `xtensa-esp32s3-espidf`
- LVGL 9.5.0
- `esp_lvgl_port` 2.8.0~1
- `esp_lcd_touch_gt911` 1.2.0~2

Component versions belong in the firmware project's `idf_component.yml`. The
ESP-IDF component manager will materialize them in the ignored
`managed_components/` directory. Do not commit downloaded components, the
generated `sdkconfig`, or build output.

## Install and verify

**Status:** ESP-IDF and Espressif Rust toolchain installation and verification are deferred and have not been performed.
The commands below are the pinned procedure for a later toolchain-validation
session, not a record of success.

Run from the repository root. All cloned or generated toolchain state stays
under the ignored `work/toolchains/` tree. These commands install a project-local
ESP-IDF and an Espressif Rust toolchain; they do not install a global ESP-IDF.

```zsh
mkdir -p work/toolchains
export CARGO_HOME="$PWD/work/toolchains/cargo"
export PATH="$CARGO_HOME/bin:$PATH"
export IDF_TOOLS_PATH="$PWD/work/toolchains/espressif"
git clone --branch v5.5.4 --depth 1 --recursive \
  https://github.com/espressif/esp-idf.git work/toolchains/esp-idf
work/toolchains/esp-idf/install.sh esp32s3
source work/toolchains/esp-idf/export.sh
idf.py --version

cargo install espup --locked
export RUSTUP_HOME="$PWD/work/toolchains/rustup"
espup install --targets esp32s3 \
  --std --export-file work/toolchains/espup-export.sh
source work/toolchains/espup-export.sh
rustc +esp --print target-list | grep -Fx xtensa-esp32s3-espidf
cargo +esp --version
```

The expected ESP-IDF verification output identifies `v5.5.4`. The Rust target
query must print exactly `xtensa-esp32s3-espidf`.

## Fetch, build, and flash the untouched vendor demo

Fetch the pinned board reference (the script verifies its SHA-256 before
extracting only the selected ESP-IDF example):

```zsh
zsh scripts/fetch-spotpear-demo.sh
```

Build the fetched reference example unchanged with the pinned ESP-IDF. The
commands operate on the ignored copy under `work/vendor/`; they do not turn the
LVGL 8 demo into committed Micro OS source.

```zsh
export IDF_TOOLS_PATH="$PWD/work/toolchains/espressif"
source work/toolchains/esp-idf/export.sh
idf.py -C work/vendor/spotpear/ESP32-S3-Touch-LCD-7-Demo/ESP-IDF/08_lvgl_Porting set-target esp32s3
idf.py -C work/vendor/spotpear/ESP32-S3-Touch-LCD-7-Demo/ESP-IDF/08_lvgl_Porting reconfigure
idf.py -C work/vendor/spotpear/ESP32-S3-Touch-LCD-7-Demo/ESP-IDF/08_lvgl_Porting build
```

Connect the board, replace the port value if necessary, then flash and monitor:

```zsh
export ESPPORT=/dev/cu.usbmodem101
idf.py -C work/vendor/spotpear/ESP32-S3-Touch-LCD-7-Demo/ESP-IDF/08_lvgl_Porting -p "$ESPPORT" flash
idf.py -C work/vendor/spotpear/ESP32-S3-Touch-LCD-7-Demo/ESP-IDF/08_lvgl_Porting -p "$ESPPORT" monitor
```

Exit the serial monitor with `Ctrl-]`.

Future Micro OS firmware will live under `firmware/micro-os-esp32/`, use LVGL
9.5.0, and receive its own build commands once an ESP-IDF project exists there.
The vendor demo commands above are only for validating the pinned board source.

## Hardware verification checklist — not yet performed

Leave every item unchecked until it has been observed on the physical
ESP32-S3-Touch-LCD-7 V1.2 N8R8 board.

- [ ] Firmware flashes without an error.
- [ ] The 800×480 LCD initializes and displays the Micro OS UI correctly.
- [ ] GT911 touch coordinates track taps across the full panel.
- [ ] Serial output completes startup without resets, panics, or watchdogs.
- [ ] A power cycle returns to the UI without manual intervention.
