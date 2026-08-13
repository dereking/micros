#!/bin/zsh
set -euo pipefail

fetch_script="scripts/fetch-spotpear-demo.sh"
toolchain_doc="firmware/micro-os-esp32/TOOLCHAIN.md"
notice="third_party/NOTICE.md"

for path in "$fetch_script" "$toolchain_doc" "$notice"; do
  test -f "$path" || { print -u2 "missing: $path"; exit 1; }
done

test -x "$fetch_script" || {
  print -u2 "not executable: $fetch_script"
  exit 1
}

for expected in \
  'ESP32-S3-Touch-LCD-7-Demo.zip' \
  '5351d443eaa605cab1eb80d050d867c18e1ce2b33c9cbc78aae1b7bca040b038'; do
  /usr/bin/grep -Fq "$expected" "$fetch_script" || {
    print -u2 "fetch script missing: $expected"
    exit 1
  }
done

for expected in 'ESP-IDF 5.5.4' 'LVGL 9.5.0'; do
  /usr/bin/grep -Fq "$expected" "$toolchain_doc" || {
    print -u2 "toolchain document missing: $expected"
    exit 1
  }
done

for expected in \
  'installation and verification are deferred and have not been performed' \
  'export IDF_TOOLS_PATH="$PWD/work/toolchains/espressif"' \
  'idf.py -C work/vendor/spotpear/ESP32-S3-Touch-LCD-7-Demo/ESP-IDF/08_lvgl_Porting build' \
  'idf.py -C work/vendor/spotpear/ESP32-S3-Touch-LCD-7-Demo/ESP-IDF/08_lvgl_Porting -p "$ESPPORT" flash'; do
  /usr/bin/grep -Fq "$expected" "$toolchain_doc" || {
    print -u2 "toolchain document missing: $expected"
    exit 1
  }
done

cargo_install_line=$(/usr/bin/grep -nF 'cargo install espup --locked' "$toolchain_doc" | /usr/bin/cut -d: -f1)
rustup_home_line=$(/usr/bin/grep -nF 'export RUSTUP_HOME="$PWD/work/toolchains/rustup"' "$toolchain_doc" | /usr/bin/head -n 1 | /usr/bin/cut -d: -f1)
test -n "$cargo_install_line" && test -n "$rustup_home_line" && \
  test "$cargo_install_line" -lt "$rustup_home_line" || {
  print -u2 "cargo install espup must precede the project-local RUSTUP_HOME export"
  exit 1
}

ignored=(
  work/vendor/spotpear/demo.zip
  work/toolchains/esp-idf/
  firmware/micro-os-esp32/build/
  firmware/micro-os-esp32/managed_components/
  firmware/micro-os-esp32/sdkconfig
  firmware/micro-os-esp32/sdkconfig.old
)

for path in $ignored; do
  /usr/bin/git check-ignore -q "$path" || {
    print -u2 "not ignored: $path"
    exit 1
  }
done

if /usr/bin/git check-ignore -q firmware/micro-os-esp32/sdkconfig.defaults; then
  print -u2 "sdkconfig.defaults must remain trackable"
  exit 1
fi

if /usr/bin/git ls-files | /usr/bin/grep -Eq \
  '^(work/vendor|work/toolchains)/'; then
  print -u2 "local vendor source or toolchain is tracked"
  exit 1
fi
