#!/bin/zsh
set -euo pipefail

readonly repo_root=${0:A:h:h}
readonly fetch_script="$repo_root/scripts/fetch-spotpear-demo.sh"
readonly fixture_root=$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/micro-spotpear-test.XXXXXX")
trap '/bin/rm -rf -- "$fixture_root"' EXIT

readonly fixture_tree="$fixture_root/tree"
readonly fixture_zip="$fixture_root/demo-fixture.zip"
readonly output_dir="$fixture_root/output"
readonly example_path='ESP32-S3-Touch-LCD-7-Demo/ESP-IDF/08_lvgl_Porting'
readonly demo_root="$output_dir/ESP32-S3-Touch-LCD-7-Demo"

/bin/mkdir -p -- "$fixture_tree/$example_path/main"
/bin/mkdir -p -- "$fixture_tree/ESP32-S3-Touch-LCD-7-Demo/ESP-IDF/99_unrelated"
print -r -- 'fixture main' > "$fixture_tree/$example_path/main/main.c"
print -r -- 'must not extract' > "$fixture_tree/ESP32-S3-Touch-LCD-7-Demo/ESP-IDF/99_unrelated/unrelated.txt"
(
  cd "$fixture_tree"
  /usr/bin/zip -qr "$fixture_zip" ESP32-S3-Touch-LCD-7-Demo
)

readonly fixture_sha=$(/usr/bin/shasum -a 256 "$fixture_zip" | /usr/bin/awk '{print $1}')
readonly fixture_url="file://$fixture_zip"

expect_refused_output() {
  local label=$1
  local candidate=$2
  local allow_outside=${3:-0}
  if MICRO_SPOTPEAR_URL="$fixture_url" \
    MICRO_SPOTPEAR_SHA256="$fixture_sha" \
    MICRO_SPOTPEAR_OUT="$candidate" \
    MICRO_SPOTPEAR_TEST_ALLOW_OUTSIDE_REPO="$allow_outside" \
    zsh "$fetch_script" >/dev/null 2>&1; then
    print -u2 -- "unsafe output unexpectedly accepted: $label"
    exit 1
  fi
}

expect_refused_output empty ''
expect_refused_output root / 1
expect_refused_output home "$HOME" 1
expect_refused_output repository "$repo_root" 1
expect_refused_output outside-default "$output_dir"

/bin/mkdir -p -- "$demo_root"
print -r -- 'keep on checksum failure' > "$demo_root/sentinel.txt"

if MICRO_SPOTPEAR_URL="$fixture_url" \
  MICRO_SPOTPEAR_SHA256='0000000000000000000000000000000000000000000000000000000000000000' \
  MICRO_SPOTPEAR_OUT="$output_dir" \
  MICRO_SPOTPEAR_TEST_ALLOW_OUTSIDE_REPO=1 \
  zsh "$fetch_script" >/dev/null 2>&1; then
  print -u2 'bad SHA unexpectedly succeeded'
  exit 1
fi
test -f "$demo_root/sentinel.txt" || {
  print -u2 'bad SHA replaced the existing extraction'
  exit 1
}

run_fetch() {
  MICRO_SPOTPEAR_URL="$fixture_url" \
    MICRO_SPOTPEAR_SHA256="$fixture_sha" \
    MICRO_SPOTPEAR_OUT="$output_dir" \
    MICRO_SPOTPEAR_TEST_ALLOW_OUTSIDE_REPO=1 \
    zsh "$fetch_script"
}

run_fetch >/dev/null
test -f "$demo_root/ESP-IDF/08_lvgl_Porting/main/main.c" || {
  print -u2 'wanted vendor example was not extracted'
  exit 1
}
test ! -e "$demo_root/ESP-IDF/99_unrelated" || {
  print -u2 'unrelated vendor content was extracted'
  exit 1
}

print -r -- 'stale' > "$demo_root/stale.txt"
run_fetch >/dev/null
test ! -e "$demo_root/stale.txt" || {
  print -u2 'repeat fetch did not replace stale extraction'
  exit 1
}
test -f "$demo_root/ESP-IDF/08_lvgl_Porting/main/main.c" || {
  print -u2 'repeat fetch lost wanted vendor example'
  exit 1
}
