#!/bin/zsh

set -euo pipefail

repo_root=${0:A:h:h}
firmware_dir="$repo_root/firmware/micro-os-esp32"
partition_file="$firmware_dir/partitions_8m.csv"
sdkconfig_file="$firmware_dir/sdkconfig.defaults"
component_file="$firmware_dir/main/idf_component.yml"

fail() {
  print -u2 -- "esp32_layout: $1"
  exit 1
}

for required_file in "$partition_file" "$sdkconfig_file" "$component_file"; do
  [[ -f "$required_file" ]] || fail "missing ${required_file#$repo_root/}"
done

typeset -a expected_rows=(
  'nvs|data|nvs|0x9000|0x6000'
  'phy_init|data|phy|0xF000|0x1000'
  'factory|app|factory|0x10000|0x380000'
  'micro_cfg|data|nvs|0x390000|0x10000'
  'micro_apps|data|littlefs|0x3A0000|0x440000'
  'coredump|data|coredump|0x7E0000|0x20000'
)

typeset -a actual_rows
while IFS= read -r row; do
  actual_rows+=("$row")
done < <(
  awk -F, '
    /^[[:space:]]*#/ || /^[[:space:]]*$/ { next }
    {
      for (i = 1; i <= 6; i++) {
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", $i)
      }
      if (NF < 5) {
        printf "invalid partition row %d\n", NR > "/dev/stderr"
        exit 2
      }
      printf "%s|%s|%s|%s|%s\n", $1, $2, $3, $4, $5
    }
  ' "$partition_file"
)

(( ${#actual_rows[@]} == ${#expected_rows[@]} )) || \
  fail "expected ${#expected_rows[@]} partitions, found ${#actual_rows[@]}"

typeset previous_end=0x9000
for index in {1..${#expected_rows[@]}}; do
  [[ "${actual_rows[$index]}" == "${expected_rows[$index]}" ]] || \
    fail "partition $index differs: expected '${expected_rows[$index]}', found '${actual_rows[$index]}'"

  IFS='|' read -r name type subtype offset size <<< "${actual_rows[$index]}"
  typeset offset_value=$(( offset ))
  typeset size_value=$(( size ))
  (( size_value > 0 )) || fail "$name has a non-positive size"
  (( offset_value == previous_end )) || \
    fail "$name starts at $offset but previous partition ends at $(([#16] previous_end))"
  previous_end=$(( offset_value + size_value ))
done

(( previous_end == 0x800000 )) || \
  fail "partition layout ends at $(([#16] previous_end)), expected 0x800000"

typeset -a sdkconfig_contract=(
  'CONFIG_IDF_TARGET="esp32s3"'
  'CONFIG_IDF_TARGET_ESP32S3=y'
  'CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ_240=y'
  'CONFIG_ESPTOOLPY_FLASHMODE_QIO=y'
  'CONFIG_ESPTOOLPY_FLASHFREQ_80M=y'
  'CONFIG_ESPTOOLPY_FLASHSIZE_8MB=y'
  'CONFIG_SPIRAM=y'
  'CONFIG_SPIRAM_MODE_OCT=y'
  'CONFIG_SPIRAM_SPEED_80M=y'
  'CONFIG_FREERTOS_HZ=1000'
  'CONFIG_PARTITION_TABLE_CUSTOM=y'
  'CONFIG_PARTITION_TABLE_CUSTOM_FILENAME="partitions_8m.csv"'
  'CONFIG_ESP_COREDUMP_ENABLE_TO_FLASH=y'
  'CONFIG_COMPILER_OPTIMIZATION_PERF=y'
)

for setting in "${sdkconfig_contract[@]}"; do
  rg -Fqx -- "$setting" "$sdkconfig_file" || fail "missing sdkconfig setting: $setting"
done

typeset -a component_contract=(
  'idf: { version: "=5.5.4" }'
  'lvgl/lvgl: { version: "=9.5.0", public: true }'
  'espressif/esp_lvgl_port: { version: "=2.8.0~1", public: true }'
  'espressif/esp_lcd_touch_gt911: { version: "=1.2.0~2", public: true }'
)

for dependency in "${component_contract[@]}"; do
  rg -Fqx -- "  $dependency" "$component_file" || fail "missing pinned dependency: $dependency"
done

print -- "ESP32 8 MB firmware layout contract passed"
