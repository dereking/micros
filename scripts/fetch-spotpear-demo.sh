#!/bin/zsh
set -euo pipefail

readonly archive_name='ESP32-S3-Touch-LCD-7-Demo.zip'
readonly archive_url='https://files.waveshare.net/wiki/ESP32-S3-Touch-LCD-7/ESP32-S3-Touch-LCD-7-Demo.zip'
readonly archive_sha256='5351d443eaa605cab1eb80d050d867c18e1ce2b33c9cbc78aae1b7bca040b038'
readonly example_path='ESP32-S3-Touch-LCD-7-Demo/ESP-IDF/08_lvgl_Porting'
readonly repo_root=${0:A:h:h}
readonly vendor_dir="$repo_root/work/vendor/spotpear"
readonly archive_path="$vendor_dir/demo.zip"
readonly demo_root="$vendor_dir/ESP32-S3-Touch-LCD-7-Demo"

/bin/mkdir -p -- "$vendor_dir"
/usr/bin/curl -fsSL "$archive_url" -o "$archive_path"

(
  cd "$vendor_dir"
  print -r -- "$archive_sha256  demo.zip" | /usr/bin/shasum -a 256 -c -
)

readonly staging_dir=$(/usr/bin/mktemp -d "$vendor_dir/.extract.XXXXXX")
trap '/bin/rm -rf -- "$staging_dir"' EXIT

/usr/bin/unzip -q "$archive_path" "$example_path/*" -d "$staging_dir"
test -d "$staging_dir/$example_path"

# This is the only persistent extracted subtree this script owns and replaces.
/bin/rm -rf -- "$demo_root"
/bin/mv -- "$staging_dir/ESP32-S3-Touch-LCD-7-Demo" "$vendor_dir/"

print -r -- "Verified $archive_name and extracted $example_path under $vendor_dir"
