#!/usr/bin/env bash
# One-click ESP32-S3 compile + flash.
#
#   scripts/esp-flash.sh              build App MBC + firmware, flash both
#   scripts/esp-flash.sh --monitor    ...then attach the serial monitor
#   scripts/esp-flash.sh --build-only build only, skip flashing
#   ESP_PORT=/dev/cu.XXX scripts/esp-flash.sh   override the auto-detected port
#
# The firmware reads the Counter MBC from the raw `micro_app` partition, so the
# full refresh is: rebuild MBC -> build firmware -> flash firmware -> flash the
# `micro_app` partition (idf.py flash does NOT write it). See agent.md.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
firmware_dir="$repo_root/firmware/micro-os-esp32"

# --- Project-local ESP-IDF 5.5.4 toolchain (NOT ~/.espressif) ---
export IDF_PATH="${IDF_PATH:-$repo_root/work/toolchains/esp-idf}"
export IDF_TOOLS_PATH="${IDF_TOOLS_PATH:-$repo_root/work/toolchains/espressif}"
export IDF_PYTHON_ENV_PATH="${IDF_PYTHON_ENV_PATH:-$repo_root/work/toolchains/espressif/python_env/idf5.5_py3.14_env}"
export ESP_IDF_CONSTRAINTS="${ESP_IDF_CONSTRAINTS:-$repo_root/work/toolchains/espressif/espidf.constraints.v5.5.txt}"
# shellcheck disable=SC1091
source "$IDF_PATH/export.sh" >/dev/null

monitor=false
build_only=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --monitor) monitor=true; shift ;;
    --build-only) build_only=true; shift ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

# Auto-detect the WCH USB-serial port; override with ESP_PORT.
port="${ESP_PORT:-$(ls /dev/cu.wchusbserial* 2>/dev/null | head -1 || true)}"
if [[ -z "$port" ]]; then
  echo "no ESP32 serial port found; set ESP_PORT=/dev/cu.XXX" >&2
  exit 1
fi

echo "==> rebuild App MBC (apps/counter/app.ts)"
(cd "$repo_root" && npm run build:app)

echo "==> build firmware"
idf.py -C "$firmware_dir" build

if [[ "$build_only" == true ]]; then
  echo "build only: skipping flash"
  exit 0
fi

echo "==> flash firmware to $port"
idf.py -C "$firmware_dir" -p "$port" flash

# `micro_app` is a raw data partition: flash the staged MBC separately.
micro_app_offset="$(awk -F'[ ,]+' '$1 == "micro_app" {print $4}' "$firmware_dir/partitions_8m.csv")"
micro_app_bin="$firmware_dir/build/esp-idf/main/micro_app.bin"
echo "==> flash micro_app partition ($micro_app_bin @ $micro_app_offset)"
"$IDF_PYTHON_ENV_PATH/bin/python" -m esptool --chip esp32s3 -b 460800 \
  --before default_reset --after hard_reset --port "$port" \
  write_flash --flash_mode dio --flash_size 8MB --flash_freq 80m \
  "$micro_app_offset" "$micro_app_bin"

if [[ "$monitor" == true ]]; then
  # Requires a real TTY; in a non-interactive shell use the esptool-run +
  # pyserial trick documented in agent.md instead.
  echo "==> serial monitor (Ctrl+] to exit)"
  "$IDF_PYTHON_ENV_PATH/bin/python" "$IDF_PATH/tools/idf_monitor.py" -p "$port" -b 115200 \
    --target esp32s3 "$firmware_dir/build/micro_os_esp32.elf" \
    "$firmware_dir/build/bootloader/bootloader.elf"
fi

echo "done"
