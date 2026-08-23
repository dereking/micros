# agent.md — ESP32-S3 编译 & 刷机工作流

## 一键编译 + 刷新

```bash
scripts/esp-flash.sh               # 重建 MBC → 编固件 → 刷固件 + micro_app 分区
scripts/esp-flash.sh --monitor     # 刷完再挂串口监视器
scripts/esp-flash.sh --build-only  # 只编译不刷
ESP_PORT=/dev/cu.XXX scripts/esp-flash.sh   # 指定串口(默认自动探测 wchusbserial)
```

脚本内置了项目本地的 ESP-IDF 环境变量(见下),直接跑即可。

## 为什么是"两步刷机"(最容易踩的坑)

- 固件**不内嵌** App 的 MBC,而是从 raw 的 `micro_app` **数据分区**读取。
- `firmware/micro-os-esp32/main/CMakeLists.txt` 把 `apps/counter/dist/app.mbc` **copy 成** `build/esp-idf/main/micro_app.bin`(随固件构建自动刷新,`npm run build:app` 改了源码就够)。
- **`idf.py flash` 不会写 `micro_app` 分区**——必须用 esptool 单独刷到 `partitions_8m.csv` 里 `micro_app` 的 offset(`0x3A0000`)。脚本里用 `awk` 从 csv 动态取 offset,不硬编码。

完整刷新链路:改 `apps/counter/app.ts` → `npm run build:app` → `idf.py build` → `idf.py flash`(bootloader + app + 分区表)→ esptool 刷 `micro_app` 分区(MBC)。

## ESP-IDF 工具链(项目本地,不在 ~/.espressif)

ESP-IDF 5.5.4 装在 `work/toolchains/` 下。直接 `source export.sh` 会失败(它默认找 `~/.espressif`),必须设这些环境变量(脚本已内置):

```bash
export IDF_PATH="/Volumes/Data/code/micros/work/toolchains/esp-idf"
export IDF_TOOLS_PATH="/Volumes/Data/code/micros/work/toolchains/espressif"
export IDF_PYTHON_ENV_PATH="/Volumes/Data/code/micros/work/toolchains/espressif/python_env/idf5.5_py3.14_env"
export ESP_IDF_CONSTRAINTS="/Volumes/Data/code/micros/work/toolchains/espressif/espidf.constraints.v5.5.txt"
source "$IDF_PATH/export.sh"
```

Python venv 缺失时用 `IDF_PYTHON_ENV_PATH` 指向 `work/toolchains/espressif/python_env/idf5.5_py3.14_env`。

## 设备

- 串口:`/dev/cu.wchusbserial59591149741`(WCH USB-serial,ESP32-S3 板;`scripts/esp-flash.sh` 自动探测,`ESP_PORT` 覆盖)
- console:UART0 @ 115200(见 `firmware/micro-os-esp32/sdkconfig` 的 `CONFIG_ESP_CONSOLE_UART_NUM=0`)
- 芯片:esp32s3,8MB flash / 8MB PSRAM;分区见 `partitions_8m.csv`(`factory` app 0x10000 0x380000、`micro_app` data 0x3A0000 0x440000)

## 串口监视器的坑

- `idf.py monitor` 需要真实 TTY(非交互 shell 会报 "requires standard input to be attached to TTY")。
- 无 TTY 时替代方案:
  - 复位+读日志:`python -m esptool --chip esp32s3 --port <PORT> --after hard_reset run`,然后用 pyserial 读 115200;
  - 或 `script -q <file> <idf_monitor.py 参数>` 分配 PTY。
- 验证 MBC 加载成功的日志特征:`micro_os: MBC header: magic OK, version=<N>` + `micro runtime created`。

## 相关 npm / 测试脚本

| 命令 | 作用 |
|---|---|
| `npm run build:app` | 编译 `apps/counter/app.ts` → `apps/counter/dist/app.mbc` |
| `npm run build:web:app` | 同上 → `products/micro-web-player/public/apps/counter.mbc` |
| `npm test` | 全平台无关 Rust 测试 |
| `npm run test:native` | 原生(SDL)一键:自检→编译→构建→冒烟→开窗 |
| `zsh tests/esp32_ui_bridge.sh` | 用 mock 编译 placeholder.c + 跑 bridge 契约测试(依赖 ESP 构建生成的 `pinyin_table.h`) |

## 布局(LTRB / Delphi anchor)关键文件

- SDK:`sdk/index.d.ts` 的 `ui.place`(`align` 糖 + `left/top/right/bottom` 锚点)
- 编译器:`crates/micro-compiler/src/lower.rs`(`align`→LTRB 展开)
- IR/编解码:`crates/micro-ir/src/model.rs`(`LayoutSpec`)、`codec.rs`(MBC **v13**)
- 引擎(两份几乎一致):`firmware/.../placeholder.c` 与 `native/src/micro_native.c` 的 `delphi_layout_update_cb` / `delphi_get_min_size_cb` / `micro_*_set_layout_spec`
- FFI 头:`micro_runtime_ffi.h`、`native/include/micro_native.h`(`mask` bit0=left,bit1=top,bit2=right,bit3=bottom)
