# 开发指南 · Development Guide

本文档描述 Micro App Platform 的日常开发工作流：初始化开发环境、编译各宿主、
运行与调试（原生 / Web / Wasm）。命令均已在本机验证。

This guide covers the day-to-day development workflow for the Micro App Platform:
environment setup, building each host, and running & debugging (native, Web, Wasm).
All commands were verified on this machine.

平台采用 Rust workspace，TypeScript App 编译为 MBC（Micro Bytecode），由 Rust VM
（`micro-vm`）在预算内执行，UI 通过 `micro-core` 的 UI Tree 与 patch 渲染到
LVGL+SDL3（macOS）或 DOM（浏览器 Wasm）。更详细的架构与命令见 [README](../README.md)。

## 1. 概览 / Overview

| 目标 | 产物 | 关键 crate / 包 |
|---|---|---|
| App（TS → MBC） | `apps/*/dist/*.mbc`（counter、settings、shell、momo） | `micro-compiler` / `micro-ir` |
| 原生宿主（macOS） | `target/debug/micro-host-sdl` | `micro-host-sdl` + `micro-lvgl` |
| Web 宿主（Wasm） | `products/micro-web-player/src/generated/` | `micro-host-web` + `micro-renderer-web` |
| Web 播放器页面 | `products/micro-web-player/dist/` | `micro-web-player`（Vite） |

## 2. 命令速查 / Command Cheat Sheet

| 命令 | 作用 |
|---|---|
| `npm run setup:dev` | 一键初始化开发环境（幂等，可反复运行） |
| `npm run check:env` | 环境自检，输出 PASS/WARN/FAIL |
| `npm run build:app` | 编译 `apps/*/app.ts` → `apps/*/dist/*.mbc` |
| `npm run build:web` | 编译 MBC + Wasm + Vite 打包 → `dist/` |
| `npm run dev:web` | 编译 MBC + Wasm，并启动 Vite 开发服务器（热更新） |
| `npm run preview:web` | 预览已打包产物（端口 4173） |
| `npm run demo` | 编译全部 App 并打开原生 800×480 桌面（Counter / Settings / momo） |
| `npm run test:native` | 一键原生测试：自检 → 编译 App → 构建宿主 → Counter 冒烟 → momo 四段流程/输入测试 → OS shell 冒烟 → 打开窗口；`-- --smoke` 只跑无头部分 |
| `npm test` | 运行全部平台无关 Rust 测试（`cargo test --workspace`） |
| `npm run test:web` | Web 端：Node 单测 + 构建 + Playwright 浏览器验收 |
| ESP 固件 build / flash | 一次性安装见 `firmware/micro-os-esp32/TOOLCHAIN.md`；之后 `idf.py -C firmware/micro-os-esp32 build` / `flash` |

## 3. 初始化开发环境 / Environment Setup

### 3.1 一键初始化（推荐）

```bash
npm run setup:dev
```

脚本 `scripts/setup-dev.sh` 是幂等的：已安装的组件会跳过，只补齐缺失项。它会：

1. 校验平台（macOS + Apple Silicon）。
2. 检查 Xcode Command Line Tools（缺失时提示你手动 `xcode-select --install`）。
3. 缺失 Rust 时经 rustup 非交互安装，并补 `rustfmt`、`clippy`。
4. 安装 `wasm32-unknown-unknown` target。
5. 缺失时用 `cargo install wasm-pack --locked` 安装 wasm-pack（首次需从源码编译，数分钟）。
6. `npm install` 安装 JS 依赖。
7. `npx playwright install chromium` 安装浏览器测试内核。
8. 末尾自动运行环境自检。

### 3.2 环境自检

```bash
npm run check:env
```

`scripts/check-env.sh` 逐项输出 `[PASS]` / `[WARN]` / `[FAIL]`，任何必需项缺失时
以非零码退出。建议在新机器、升级工具链或 CI 失败时先跑它。

### 3.3 手动分步安装（可选）

| 工具 | 要求 | 安装/验证 |
|---|---|---|
| Xcode CLT | 必须 | `xcode-select --install`；验证 `xcode-select -p` |
| Rust | ≥ stable | [rustup.rs](https://rustup.rs)；验证 `cargo --version` |
| rustfmt / clippy | 必须 | `rustup component add rustfmt clippy` |
| wasm32 target | Web 必须 | `rustup target add wasm32-unknown-unknown` |
| wasm-pack | Web 必须 | `cargo install wasm-pack --locked` |
| Node.js + npm | Web 必须 | Node LTS；验证 `node --version` |
| Playwright Chromium | Web 测试必须 | `npx playwright install chromium` |
| CMake | 原生必须 | ≥ 3.24；验证 `cmake --version` |

> **EN — Setup.** Run `npm run setup:dev` once to initialize the environment; it is
> idempotent and installs/verifies Rust + wasm32 target + wasm-pack + npm deps +
> Playwright Chromium. Re-run `npm run check:env` any time to confirm readiness.

## 4. 编译 / Building

### 4.1 App：TypeScript → MBC

```bash
npm run build:app
# 等价于：对 apps/*/app.ts 逐个运行
# cargo run -p micro-compiler --bin microc -- apps/counter/app.ts apps/counter/dist/counter.mbc
```

Web 播放器使用另一份 MBC 副本（由 `npm run dev:web` / `build:web` 自动生成）：

```bash
cargo run -p micro-compiler --bin microc -- \
  apps/momo/app.ts products/micro-web-player/public/apps/momo.mbc
```

MBC 是生成产物（已被 gitignore），请编辑 `app.ts` 后重新编译，不要手改 `.mbc`。

### 4.2 Web 宿主：MBC + Rust→Wasm + 页面

```bash
npm run build:web
```

等价于三步：

```bash
# 1) TS → MBC（到 public/，Vite 直接伺服）
npm run build:web:app
# 2) Rust → Wasm（产物到 src/generated/，作为 ES module 引入）
npm run build:web:wasm   # wasm-pack build crates/micro-host-web --target web
# 3) Vite 打包页面 → products/micro-web-player/dist/
npx vite build products/micro-web-player
```

### 4.3 原生宿主：macOS（LVGL + SDL3）

```bash
cargo build -p micro-host-sdl --features native
```

首次构建会经 CMake FetchContent 下载并编译 SDL 3.4.10 与 LVGL 9.5.0（缓存于
`target/native-deps`），后续构建复用缓存。无需 Homebrew 依赖。

如果 GitHub 不可达，可以先把两个依赖克隆到
`target/native-deps/sdl3-src` 与 `target/native-deps/lvgl-src`；`scripts/test-native.sh`
会自动把这两个目录作为本地 source，并设置 `FETCHCONTENT_FULLY_DISCONNECTED=ON`。
手动构建时对应的环境变量是 `MICRO_NATIVE_SDL3_SOURCE` /
`MICRO_NATIVE_LVGL_SOURCE` / `MICRO_NATIVE_OFFLINE=1`。

### 4.4 清理

- 原生依赖缓存异常：只删 `rm -rf target/native-deps` 后重新构建（**不要删仓库**）。
- 生成产物（`.mbc`、`src/generated/`、`dist/`）随时可重新生成，删掉会自动重建。

> **EN — Building.** `npm run build:app` compiles the Counter TS to MBC;
> `npm run build:web` produces MBC + Wasm + a static page bundle; the native host
> builds with `cargo build -p micro-host-sdl --features native` (first run fetches
> SDL3/LVGL into `target/native-deps`). All `.mbc`, Wasm, and `dist/` outputs are
> gitignored and regenerated on demand.

## 5. 运行 / Running

### 5.1 Web 播放器（开发 / 预览）

```bash
npm run dev:web
# 打开 http://127.0.0.1:5173/  （端口被占用时 Vite 会自动递增）
```

页面即 ESP32-S3 Touch-LCD-7 模拟器（800×480，RGB565/16 MHz，GT911，CH422G，
8 MiB Flash / 8 MiB PSRAM）。Launcher、Settings、Counter 与 momo 都由同一份编译好的
MBC 走共享 Runtime 执行。Settings 用根级 `ui.tabview` 分 Network / Display / Sound /
Security / System / About 六类：亮度、Wi-Fi 扫描与连接、语速、存储卡状态接的是真实宿主
API，其余条目以 `Coming soon` 占位。Vite 对 `main.js` 等源码热更新，改
`products/micro-web-player/src/*.js` 保存即生效；改 Rust 侧需重新 `build:web:wasm`
（重启 `dev:web` 或另跑 `npm run build:web:wasm`）。

预览打包产物：

```bash
npm run preview:web   # http://127.0.0.1:4173/
```

### 5.2 原生宿主（macOS 窗口）

一键原生测试（自检 → 编译 App → 构建宿主 → 无头冒烟 → 打开窗口）：

```bash
npm run test:native
# 只跑无头冒烟、不开窗口：
npm run test:native -- --smoke
```

打开窗口：

```bash
npm run demo   # = build:app + cargo run ... --os shell + counter + settings + momo
```

或直接运行（需先 `npm run build:app`）：

```bash
cargo run -p micro-host-sdl --features native -- --os \
  apps/shell/dist/shell.mbc apps/counter/dist/counter.mbc \
  apps/settings/dist/settings.mbc apps/momo/dist/momo.mbc
```

无头验收模式（隐藏窗口，两次排队点击后校验状态为 2，成功则退出）：

```bash
cargo run -p micro-host-sdl --features native -- \
  --smoke apps/counter/dist/counter.mbc
```

原生窗口顶部有宿主图标工具栏（Landscape / Portrait / Rotate，悬停显示名称）；
macOS 上还有一个齿轮 **Device** 菜单（Power On/Off、Force Restart、方向、缩放、
模拟器信息）。命令行也支持 `--orientation landscape|portrait`、
`--zoom 25..400|fit`（参数可放在 launch 目标前后）：

```bash
cargo run -p micro-host-sdl --features native -- \
  --orientation portrait --zoom fit --os \
  apps/shell/dist/shell.mbc apps/counter/dist/counter.mbc \
  apps/settings/dist/settings.mbc apps/momo/dist/momo.mbc
```

窗口内快捷键：`Cmd/Ctrl+R` 切换横竖屏、`Cmd/Ctrl+,` 打开 macOS Device 菜单、
`Cmd/Ctrl+0` 适配显示器、`Cmd/Ctrl+1` 回到 100%、`Cmd/Ctrl+=` / `-` 或
`Cmd/Ctrl+滚轮` 缩放。切换方向会
改变 LVGL 逻辑分辨率（800×480 ↔ 480×800），同步调整 app bar/内容容器并让现有
`ui.place` 锚点原地重排；Runtime 与 App 状态保留。`--rotate 90/270` 仍作为
竖屏的兼容别名。坐标空间、工具栏和布局重排细节见
[docs/native-display.md](native-display.md)。

原生输入框支持键盘输入和 `Cmd+V` 粘贴：SDL text input 事件会写入当前
textarea，宿主循环再把 bridge 里的 `onChange` 变更转成 `Event::InputChanged`。
momo 的真实同步会把今日未完成词写入队列，读取今日进度和每词学习记录，并通过
`net.lookupWords` 调用有道 JSON 词典补全释义/例句；可用环境变量跑一次 ignored
冒烟（token 不写入仓库，测试 KV 在临时目录）：

```bash
MAIMEMO_TOKEN='...' cargo test -p micro-host-sdl --features native \
  --test momo_native native_sync_loads_today_items_from_maimemo -- --ignored
```

> **EN — Running.** `npm run dev:web` compiles MBC + Wasm then serves the simulator at
> http://127.0.0.1:5173/; `npm run preview:web` serves the built bundle at :4173.
> `npm run demo` opens the native desktop window with a top Landscape/Portrait/Rotate
> toolbar plus a macOS **Device** menu (power on/off, force restart, orientation,
> zoom, emulator info; `Cmd/Ctrl+,`). `--smoke` runs the host headlessly and exits
> after verifying Counter reaches state 2.

## 6. 调试 / Debugging

### 6.1 Rust 单元测试调试

```bash
# 全 workspace；-p 限定 crate；<filter> 按名字过滤
cargo test -p micro-vm
cargo test -p micro-vm -- budget
cargo test -p micro-core -- --nocapture     # 打印测试内 println!/dbg!
RUST_BACKTRACE=1 cargo test -p micro-ir     # 断言失败时打印栈
```

### 6.2 原生宿主调试（LLDB / rust-lldb）

`rust-lldb` 随 rustup 自带（缺则 `rustup component add rust-lldb`）。先构建调试二进制：

```bash
cargo build -p micro-host-sdl --features native
rust-lldb target/debug/micro-host-sdl
```

在 lldb 内：

```text
(lldb) breakpoint set -n host_iteration        # 在循环推进函数处下断点
(lldb) breakpoint set -f main.rs -l 71         # 或按文件/行号
(lldb) run -- --smoke apps/counter/dist/counter.mbc   # 无头模式，跑完即退出
(lldb) run -- apps/counter/dist/counter.mbc           # 交互窗口模式，断点后手动点击
(lldb) next / step / continue
```

小技巧：

- 交互模式下手动点击窗口触发事件；`--smoke` 模式自动排队两次点击，适合快速验证逻辑。
- `RUST_BACKTRACE=1` 可在 panic 时打印 Rust 栈。
- 只测某段逻辑而无需 UI 时，优先用对应 crate 的单测（6.1）而非宿主进程。

### 6.3 Web / Wasm 调试（Chrome DevTools）

```bash
npm run dev:web
```

1. 打开 http://127.0.0.1:5173/，按 `Cmd+Option+J` 打开 DevTools。
2. **Sources** 面板：Vite 以 ESM 伺服 `main.js`、`runtime-loop.js`、`device-shell.js`，
   可直接下断点、单步、查看变量（热更新不会打断断点）。
3. **Console**：Rust 侧在 Wasm 中 panic 时，dev profile 保留了调试信息，panic 消息会
   出现在 Console；JS 侧异常会被 `main.js` 的 `reportError` 捕获并显示在模拟器屏幕上的
   运行时错误区域。
4. 改 JS 源码保存即热更新；改 Rust 后需 `npm run build:web:wasm` 重新编译 Wasm。

> 说明：默认 Wasm 不带 DWARF/sourcemap，因此不能在 DevTools 里直接对 Rust 源码单步。
> 需要时可在 `micro-host-web` 接入 `console_error_panic_hook`（打印完整 panic 消息与
> 回溯）并让 wasm-pack 生成 debug symbols；这是可选增强，不影响现有流程。

### 6.4 编译器 microc 调试

```bash
# 直接调用 CLI 看诊断（path:line:column + MTS 错误码，编译失败退出码 2）
cargo run -p micro-compiler --bin microc -- apps/counter/app.ts /tmp/out.mbc

# 调试编译器自身
cargo build -p micro-compiler --bin microc
rust-lldb target/debug/microc
(lldb) breakpoint set -n run
(lldb) run -- apps/counter/app.ts /tmp/out.mbc
```

### 6.5 Playwright 端到端调试

```bash
npm run build:web                 # 先构建（test:web 内含构建）
npx playwright test --headed      # 有头模式，肉眼观察 Chromium 行为
PWDEBUG=1 npx playwright test     # 打开 Playwright Inspector，可逐步回放
npx playwright test --ui          # UI 模式浏览测试
```

`npm run test:web` = Node 单测 + 构建 + Playwright 全量，日常迭代用上面的拆分命令更快。

> **EN — Debugging.** Rust unit tests: `cargo test -p <crate> -- <filter>`.
> Native host: build then `rust-lldb target/debug/micro-host-sdl`, break at
> `host_iteration`, `run -- [--smoke] apps/counter/dist/app.mbc`. Web/Wasm: `npm run
> dev:web`, then Chrome DevTools Sources/Console — Rust panics surface on the
> simulator screen and console (dev profile keeps debug info). Playwright: `npx
> playwright test --headed` or `PWDEBUG=1`.

## 7. 测试 / Testing

| 命令 | 内容 |
|---|---|
| `npm test` | `cargo test --workspace`：MBC 编解码/校验、VM 预算与分支、Core 事件/状态/绑定、编译器诊断与 CLI、LVGL/DOM 桥接、原生 `--smoke`、共享 Wasm 激活队列 |
| `npm run test:web` | Node 单测（`tests/web/*.test.js`）+ 构建 + Playwright 在 Chromium 中点击两次 **Add** 验证 `Count: 2` 无 Runtime 错误 |
| `npm run test:native -- --smoke` | 原生 LVGL/SDL 无头冒烟；含 momo 四段流程、键盘输入与真实数据测试（不跑 ignored 联网用例） |

## 8. 排障 / Troubleshooting

- 某工具缺失或版本不符：先 `npm run check:env` 定位，再 `npm run setup:dev` 补齐。
- 原生首建失败（网络/SDL/LVGL）：恢复网络后重跑同一命令，已下载源码缓存在
  `target/native-deps`。
- MBC 过期或被拒：`npm run build:app` 重新生成。
- 浏览器测试缺内核：`npx playwright install chromium`。
- 详细故障排查清单见 [README](../README.md) 的 **Troubleshooting** 一节。

## 9. 产物与 gitignore / Build Artifacts

所有产物都不进版本库，由构建命令重新生成：

| 产物 | 来源 | 生成命令 |
|---|---|---|
| `apps/*/dist/*.mbc` | App 编译输出 | `npm run build:app` |
| `products/micro-web-player/public/apps/*.mbc` | Web 版 MBC | `npm run build:web:app` / `dev:web` |
| `products/micro-web-player/src/generated/` | Wasm 包 | `npm run build:web:wasm` |
| `products/micro-web-player/dist/` | Vite 产物 | `npm run build:web` |
| `target/`、`target/native-deps/` | Rust/原生编译 | cargo 各命令 |
| `test-results/`、`playwright-report/` | 测试报告 | `npm run test:web` |
