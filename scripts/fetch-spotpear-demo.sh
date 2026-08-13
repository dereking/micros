#!/bin/zsh
set -euo pipefail

readonly archive_name='ESP32-S3-Touch-LCD-7-Demo.zip'
readonly official_url='https://files.waveshare.net/wiki/ESP32-S3-Touch-LCD-7/ESP32-S3-Touch-LCD-7-Demo.zip'
readonly official_sha256='5351d443eaa605cab1eb80d050d867c18e1ce2b33c9cbc78aae1b7bca040b038'
readonly example_path='ESP32-S3-Touch-LCD-7-Demo/ESP-IDF/08_lvgl_Porting'
readonly repo_root=${0:A:h:h}
readonly default_vendor_dir="$repo_root/work/vendor/spotpear"

if (( ${+MICRO_SPOTPEAR_OUT} )) && [[ -z $MICRO_SPOTPEAR_OUT ]]; then
  print -u2 -- 'MICRO_SPOTPEAR_OUT must not be empty'
  exit 2
fi

readonly archive_url=${MICRO_SPOTPEAR_URL:-$official_url}
readonly archive_sha256=${MICRO_SPOTPEAR_SHA256:-$official_sha256}
readonly vendor_dir=${MICRO_SPOTPEAR_OUT:-$default_vendor_dir}
readonly canonical_vendor_dir=${vendor_dir:A}
readonly canonical_default_dir=${default_vendor_dir:A}
readonly canonical_repo_root=${repo_root:A}
readonly canonical_user_home=${HOME:A}

[[ -n $archive_url ]] || {
  print -u2 -- 'vendor URL must not be empty'
  exit 2
}
/usr/bin/grep -Eq '^[[:xdigit:]]{64}$' <<< "$archive_sha256" || {
  print -u2 -- 'vendor SHA-256 must contain exactly 64 hexadecimal digits'
  exit 2
}

case "$canonical_vendor_dir" in
  /|"$canonical_user_home"|"$canonical_repo_root")
    print -u2 -- "refusing unsafe vendor output: $canonical_vendor_dir"
    exit 2
    ;;
esac

if [[ ${MICRO_SPOTPEAR_TEST_ALLOW_OUTSIDE_REPO:-0} != 1 && \
      $canonical_vendor_dir != "$canonical_default_dir" && \
      $canonical_vendor_dir != "$canonical_default_dir"/* ]]; then
  print -u2 -- "vendor output must remain under $canonical_default_dir"
  exit 2
fi

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
