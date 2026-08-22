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

**Status:** The project-local ESP-IDF 5.5.4 toolchain was installed and used
to build the Micro OS scaffold plus shared Rust Runtime on 2026-08-14. The
Espressif Rust toolchain remains project-local under `work/toolchains/` and
targets `xtensa-esp32s3-espidf`; the generated firmware map contains
`libmicro_host_esp32.a`. This records only a local toolchain/build check; no firmware was flashed and no hardware verification was performed.

## Panic strategy constraint

The pinned Espressif Rust target uses `panic=abort`. A direct project-local
experiment with `-Zbuild-std=std,panic_unwind` still selected the target's
abort strategy and failed looking for `panic_abort`; forcing `-C panic=unwind`
then failed with `unwinding panics are not supported without std`. Therefore
the ESP C ABI does not use `catch_unwind`, which cannot contain an abort. It
validates null pointers, declared lengths and capacities, canonical event
fields, enum discriminants, addressable byte-size arithmetic, and decode results before mutation and returns
stable error codes. As with ordinary C opaque handles, an arbitrary non-null
stale, misaligned, cross-type, aliased, or already-destroyed pointer cannot be
validated at runtime and violates the documented ABI contract. Host-side tests still use
unwind containment to prove corrupt MBC does not panic. Out-of-memory and an
unexpected internal Rust panic remain process-level faults on this target.

C enum width is not guaranteed by ISO C. The public header uses explicit enum
values and compile-time `sizeof == 4` assertions, verified with both the host C
compiler and the pinned Xtensa GCC toolchain.

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

Micro OS firmware lives under `firmware/micro-os-esp32/` and builds with:

```zsh
export IDF_TOOLS_PATH="$PWD/work/toolchains/espressif"
source work/toolchains/esp-idf/export.sh
source work/toolchains/espup-export.sh
idf.py -C firmware/micro-os-esp32 set-target esp32s3
idf.py -C firmware/micro-os-esp32 build
```

The first build compiles LVGL 9.5.0, esp_lvgl_port, GT911, and the Rust
stdlib for `xtensa-esp32s3-espidf`; later builds reuse everything under
`firmware/micro-os-esp32/build/` and `work/toolchains/`.

## Flash and monitor the Micro OS firmware

A successful build produces the standard ESP-IDF output plus the staged MBC
image under `firmware/micro-os-esp32/build/`. Connect the Spotpear
ESP32-S3-Touch-LCD-7 V1.2 N8R8 board, then export the port and flash the four
images that make up the device: bootloader, partition table, application, and
the Counter MBC.

`idf.py flash` only knows about the three ESP-IDF images. The MBC image lives
at a fourth partition (`micro_app`, raw subtype `0x06`) that ESP-IDF does not
treat as a standard flashable asset, so the `esptool` invocation below writes
all four segments in one go:

```zsh
export IDF_TOOLS_PATH="$PWD/work/toolchains/espressif"
source work/toolchains/esp-idf/export.sh
source work/toolchains/espup-export.sh
export ESPPORT=/dev/cu.wchusbserial59591149741   # WCH USB-serial on this machine
python -m esptool --chip esp32s3 -b 460800 \
  --before default_reset --after hard_reset write_flash \
  --flash_mode dio --flash_size 8MB --flash_freq 80m \
  0x0      firmware/micro-os-esp32/build/bootloader/bootloader.bin \
  0x8000   firmware/micro-os-esp32/build/partition_table/partition-table.bin \
  0x10000  firmware/micro-os-esp32/build/micro_os_esp32.bin \
  0x3A0000 firmware/micro-os-esp32/build/esp-idf/main/micro_app.bin
```

Then attach the serial monitor:

```zsh
python -m esp_idf_monitor --port "$ESPPORT"
```

`esp_idf_monitor` requires a real TTY; when running it from a non-interactive
shell, use a small Python snippet that opens the port with `pyserial`,
pulse-toggles RTS to reset the board, and reads the boot stream into a buffer.

A successful boot prints, in order:

```
micro_os: detected Flash: 8388608 bytes
micro_os: detected PSRAM: 8388608 bytes
micro_os: 8 MB Flash / 8 MB PSRAM hardware class verified
GT911: TouchPad_ID:0x39,0x31,0x31
LVGL: Starting LVGL task
micro_os: clearing smoke screen for MBC runtime bring-up
micro_os: micro_app partition: offset=0x3a0000 size=4456448
micro_os: MBC header: magic OK, version=3, payload=1274, total=1288
micro_os: loaded MBC: 1288 bytes from micro_app partition
micro_os: micro runtime created; ticking every 30 ms
main_task: Returned from app_main()
```

After this the LCD shows the Counter App's static UI tree (title, count line,
status line, Add / Reset / Double / Switch buttons); each touch on a button
goes through the GT911 → LVGL input device → `micro_esp_ui_take_activation`
queue, and `micro_runtime_tick` (driven by an `lv_timer`) drains the queue
and runs the matching handler with a 10,000-instruction budget.

On an iteration, reflash only the parts that changed:

```zsh
# Just the application binary (no MBC change):
python -m esptool --chip esp32s3 -b 460800 \
  --before default_reset --after hard_reset write_flash \
  --flash_mode dio --flash_size 8MB --flash_freq 80m \
  0x10000 firmware/micro-os-esp32/build/micro_os_esp32.bin

# Just the MBC image (after `npm run build:app` re-emits apps/counter/dist/app.mbc):
python -m esptool --chip esp32s3 -b 460800 \
  --before default_reset --after hard_reset write_flash \
  --flash_mode dio --flash_size 8MB --flash_freq 80m \
  0x3A0000 firmware/micro-os-esp32/build/esp-idf/main/micro_app.bin
```

## Incremental rebuilds

`work/toolchains/` holds the project-local ESP-IDF 5.5.4, Xtensa GCC 14.2.0
ESP32-S3 toolchain, and the Xtensa Rust toolchain (none of it is in version
control). After editing the firmware C code, the `micro-host-esp32` Rust
crate, the LVGL C port, or the UI fonts, rerun:

```zsh
idf.py -C firmware/micro-os-esp32 build
```

ESP-IDF only recompiles changed components. The CMake `ExternalProject_Add`
under `components/micro_runtime_ffi/` only reruns `cargo build` when sources in
`crates/micro-host-esp32/` or its SDK bindings change. The Rust ABI stamp
under
`firmware/micro-os-esp32/build/esp-idf/micro_runtime_ffi/rust-abi-complete.sha256`
is rewritten on every successful Rust archive build; this is the commit point
proving the Rust ABI is part of the final link.

## Build artifacts

A successful build produces:

- `firmware/micro-os-esp32/build/micro_os_esp32.bin` — application image, flashed at `0x10000`
- `firmware/micro-os-esp32/build/micro_os_esp32.elf` — debug ELF with symbols
- `firmware/micro-os-esp32/build/micro_os_esp32.map` — full linker map
- `firmware/micro-os-esp32/build/esp-idf/micro_runtime_ffi/target/xtensa-esp32s3-espidf/release/libmicro_host_esp32.a` — Rust static archive linked into the C ABI component
- `firmware/micro-os-esp32/build/esp-idf/micro_runtime_ffi/rust-abi-complete.sha256` — SHA-256 of the Rust archive inputs, the archive itself, and the link map

## Hardware verification checklist — not yet performed

Leave every item unchecked until it has been observed on the physical
ESP32-S3-Touch-LCD-7 V1.2 N8R8 board.

- [ ] Firmware flashes without an error.
- [ ] The 800×480 LCD initializes and displays the Micro OS UI correctly.
- [ ] GT911 touch coordinates track taps across the full panel.
- [ ] Serial output completes startup without resets, panics, or watchdogs.
- [ ] A power cycle returns to the UI without manual intervention.
