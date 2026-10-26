# PC 原生宿主：方向切换、宿主工具栏与布局重排

本文记录 `micro-host-sdl`（macOS 的 LVGL + SDL3 宿主）如何在横屏与竖屏之间
切换，以及 App/OS 布局如何跟着新的逻辑视口重排。它取代了早期“只旋转整块
800×480 画布、不重排布局”的方案。

## 1. 结论先行

- **设备有两种逻辑方向**：横屏 `800×480`，竖屏 `480×800`。切换时改的是
  LVGL 的逻辑分辨率，不是对成品纹理做位图旋转。
- **宿主工具栏在设备画布之外**：窗口顶部固定 40 px 的 SDL 图标工具栏，提供
  Landscape / Portrait / Rotate 三个图标控件；macOS 上再多一个 **Device**
  齿轮按钮，打开原生 `NSMenu`。工具栏和菜单都不属于 LVGL，不参与 App 布局，
  也不会随方向旋转或缩放。
- **OS chrome 跟着方向走**：40 px app bar 始终在逻辑屏幕顶部；内容容器从
  `800×440` 变为 `480×760`。app bar 的标题、Wi-Fi、时钟、关闭按钮重新定位。
- **App 在同一棵 UI 树上重排**：宿主不重启 Runtime，不清空 App 状态；只把
  LVGL 分辨率、缓冲区和 app bar 内容容器改成新尺寸，再让现有的 `ui.place`
  `anchor` 重新执行。shell/settings/momo 的固定宽度布局已经改为贴边布局，
  counter 本身就是锚点布局。
- **没有“横屏一套 UI、竖屏另一套 UI”**：MBC 里仍然只有一棵静态 UI 树。
  需要适配的地方用 `left+right`、`top+bottom` 这类双边 anchor 表达。

## 2. 窗口分层

```text
┌────────────────────────────────────────────┐
│  host toolbar 40 px (SDL, 不属于 LVGL)      │
│  [▭] [▯] [↻] │ [⚙]  Landscape/Portrait/...  │
├────────────────────────────────────────────┤
│  设备画布 (LVGL)                            │
│  landscape: 800×480   portrait: 480×800     │
│  ┌──────────────────────────────────────┐  │
│  │ OS app bar 40 px                     │  │
│  ├──────────────────────────────────────┤  │
│  │ App content                          │  │
│  │ landscape 800×440 / portrait 480×760 │  │
│  └──────────────────────────────────────┘  │
└────────────────────────────────────────────┘
```

窗口内容高度 = 设备画布高度 + 40 px 工具栏。窗口标题显示当前方向和缩放，
例如 `Micro App - Portrait - 118%`。

### 坐标空间

```text
design (800×480 或 480×800, LVGL / ui.place)
  --uniform fit scale--> render pixels
  --window ratio-------> window points (mouse / SDL events)
```

不再有“旋转后画布”这一层。实现位于 `native/src/micro_native.c`：

| 状态 | 含义 |
| --- | --- |
| `design_width/height` | 当前方向的 LVGL 逻辑尺寸 |
| `landscape_width/height` | 创建时的横屏尺寸；竖屏尺寸由它交换得到 |
| `portrait` | 当前是否为竖屏 |
| `zoom_percent` | 设备比例，25–400，100 = 1 设计像素 / 1 窗口点 |
| `view_scale` | 输出像素 / 设计像素 |
| `view_x/view_y` | 画布在工具栏下方区域中的居中位置 |

`fit` 计算时先扣掉工具栏：

```text
canvas_h = output_h - toolbar_h
scale    = min(output_w / design_w, canvas_h / design_h)
view_x   = (output_w - design_w * scale) / 2
view_y   = toolbar_h + (canvas_h - design_h * scale) / 2
```

鼠标/触摸点先换算到输出像素，再减 `view_x/view_y`、除以 `view_scale`，得到
当前方向的逻辑坐标。点击工具栏区域会被宿主消费，不会传给 LVGL。

### 滚轮 / 触控板滚动

SDL 的鼠标滚轮和触控板双指滑动都以 `SDL_EVENT_MOUSE_WHEEL` 到达宿主，而不是
pointer drag，因此 LVGL 的 pointer indev 看不到它们。`micro_native_poll` 在没有
`Cmd/Ctrl` 时把 wheel 事件换算成逻辑坐标，命中最深层对象后向上找到第一个在
该方向仍有剩余空间的 `LV_OBJ_FLAG_SCROLLABLE` 祖先，调用
`lv_obj_scroll_by_bounded`。滚动量按 48 px/格换算，触控板的小数格会累积到足够
一像素再移动。效果与手指拖动一致：tab page 的内容和滚动条一起移动。
`Cmd/Ctrl+wheel` 仍保留给缩放。

## 3. 方向切换做了什么

`micro_native_set_orientation(native, portrait)`：

1. 选择 `800×480` 或 `480×800`；
2. `realloc` LVGL 局部渲染缓冲区（按新宽度 × 40 行 × 4 字节）；
3. 重建 `SDL_TEXTUREACCESS_STREAMING` 纹理为新的逻辑尺寸；
4. `lv_display_set_resolution()` + `lv_display_set_buffers()`；
5. `micro_appbar_set_viewport()`：调整 app bar、内容容器的位置和大小，
   把 App 根节点设为内容容器的 `100% × 100%`；
6. `lv_obj_update_layout()`：让根容器及其子节点重新执行 `ui.place` 布局；
7. 按当前 `zoom_percent` 重新调整窗口大小并居中；
8. 更新标题，下一次 LVGL flush 时重绘。

Runtime、VM 栈、`state()`、输入框内容和已挂载的 LVGL 对象都保留。切换方向
不是“重启 App”，而是一次分辨率/布局更新。

### OS chrome

`native/src/micro_appbar.c` 的 `micro_appbar_set_viewport(width, height)`：

- 把 `s_bar` 设为 `width × 40`；
- 把 `s_content` 设为 `width × (height - 40)`；
- 重新执行 `reposition_children()`（标题宽度、Wi-Fi/时钟/关闭按钮）；
- 把 `s_content` 的直接子节点（App 根节点）设为 `100% × 100%`；
- `lv_obj_update_layout(s_content)` 触发整棵 App 树的锚点重算。

因此竖屏下 app bar 仍然横跨物理顶部，而不是像旧方案那样被旋转到屏幕侧边。

### App 布局

布局引擎是 Delphi 风格 `ltwh + anchor`，没有 flex/wrap。响应式写法是：

```ts
// 固定宽度 → 贴左右边
ui.place(ui.surface("paper"), {
  top: 108,
  height: 96,
  anchor: { left: 24, right: 24 },
});

// 固定高度 → 贴上下边（页面背景）
ui.place(ui.surface("canvas"), {
  anchor: { left: 0, right: 0, top: 0, bottom: 0 },
});
```

`anchor` 的优先级高于同方向的 `left/width` 或 `top/height`；双边 anchor 会
拉伸子节点。`delphi_layout_update_cb` 在父容器尺寸变化时重新读取
`avail_w/avail_h`，所以同一棵树可以直接在两种方向下重排。

App 根容器、以及 tab 页里直接挂载的内容根容器，都会被宿主拉满可用高度
（原生在 `micro_native_create_column/row` 和 `apply_delphi_layout` 中处理，
Web 在 `dom.rs` 的 fill-container 分支和 `.micro-tab-page > .micro-column`
中处理）。因此全屏 `canvas`、tab 页背景和页面底部点击区域在横竖屏下都覆盖
到边缘，不会因为内容自然高度只有 376 px 而在竖屏底部留下一条“点不到”的缝。

本仓库已适配：

- **shell**：原来的 3 列固定卡片改为纵向全宽卡片列表（宽度 24 px 贴边），
  页面背景四边贴满；footer 贴底，横屏和竖屏都不会被裁掉。
- **settings**：根 tabview 四边贴满；5 个分类页（Network / Display / Sound /
  System / About）的卡片使用左右 20 px 贴边，长说明文字改为 `left+right`
  双边 anchor，不再出现 520/620 px 固定宽度在 480 宽下溢出。
- **momo**：5 个 tab 的 `canvas` 背景改为四边贴满；`width: 500/700` 的说明
  文字改为左右贴边；“认识/不认识”按钮从 365+370 改为 210+200。
- **counter**：原本就以 `anchor` 为主，480 宽下所有固定按钮宽度 ≤ 344，无需改
  结构。

竖屏下内容仍然从顶部开始排；固定高度的卡片下方会出现背景留白。这是当前
“单棵静态树 + 绝对布局”模型的取舍，不是裁切或错位。

## 4. 工具栏与 Device 菜单

SDL 在设备画布上方绘制 40 px 图标工具栏，全部由基础图元（矩形、线段、
三角形、圆弧）绘制，不引入 SDL_ttf：

- 左侧三个方形图标按钮：横屏（横置设备轮廓）、竖屏（竖置设备轮廓）、
  Rotate（顺时针圆弧箭头）；
- macOS 上第四个按钮是齿轮 **Device**，与 Rotate 之间有一条细分隔线；非
  macOS 构建不显示齿轮，`micro_host_menu_available()` 返回 0；
- 当前方向用绿色胶囊高亮；鼠标悬停显示浅色胶囊，并在右侧显示
  `Landscape` / `Portrait` / `Rotate` / `Device menu` 名称（调试字体按输出
  DPI 缩放）；
- 点击命中按输出像素计算，窗口缩放/高 DPI 时仍正确；窗口变窄时按钮宽度
  会自动收窄，图标仍保持居中；
- 点击工具栏只改变方向/打开宿主菜单，不向 LVGL 发送按下/抬起事件；
- 无头测试模式不显示工具栏（窗口和逻辑画布同尺寸）。

Device 菜单是 **锚定在齿轮下方的原生 `NSMenu`**，不是 macOS 菜单栏项：

| 分组 | 条目 | 行为 |
| --- | --- | --- |
| DEVICE | 运行状态 | 只读；`Running` / `Powered off` |
| Power | Power On / Power Off | 关机面只允许 Power On；运行中只允许 Power Off |
| Power | Force Restart… | 二次确认后关闭当前 App，冷启动 Home，保留已安装 App 和存储 |
| DISPLAY | Landscape / Portrait | 切换逻辑分辨率；原生宿主原地重排，不重启 Runtime |
| DISPLAY | Zoom → Fit / 75/100/125/150% | 与快捷键/滚轮同一套缩放状态 |
| Emulator Info | Board / Resolution / Memory / Touch / Runtime | 只读模拟器参数，分辨率随方向更新 |

菜单项通过 `micro_native_request_host_action()` 投递 SDL user event，由
`micro_native_poll()` 收进宿主队列，再由 Rust 主循环 `take_host_action()`
消费。因此电源/重启不会被当成 App 触摸输入。菜单每次打开时重新读取
`micro_native_host_state()`，刷新禁用状态和勾选标记。

快捷键（macOS 用 Cmd，其他平台用 Ctrl）：

| 快捷键 | 行为 |
| --- | --- |
| `Cmd/Ctrl+R` | 横屏/竖屏切换 |
| `Cmd/Ctrl+,` | 打开 Device 菜单（仅 macOS） |
| `Cmd/Ctrl+0` | fit 到当前显示器 |
| `Cmd/Ctrl+1` | 100% |
| `Cmd/Ctrl+=` / `-` | 放大 / 缩小 25% |
| `Cmd/Ctrl+滚轮` | 放大 / 缩小 |

## 5. API 与命令行

C API（`native/include/micro_native.h`）：

```c
int micro_native_set_orientation(micro_native_t *, int portrait);
int micro_native_get_orientation(micro_native_t *);
int micro_native_set_rotation(micro_native_t *, int degrees); /* 兼容别名：90/270 → 竖屏 */
int micro_native_set_zoom(micro_native_t *, int percent);     /* 0 = fit */
int micro_native_set_zoom_fit(micro_native_t *);
int micro_native_object_rect(micro_native_t *, uint32_t node_id,
                             int *x, int *y, int *width, int *height);

/* 宿主菜单动作：0/1/2 = Power Off/Power On/Force Restart，
   3/4 = Landscape/Portrait，5 = Fit，6..9 = 75/100/125/150%。 */
void micro_native_request_host_action(int action);
int  micro_native_take_host_action(micro_native_t *, int *action);
int  micro_native_host_state(micro_native_host_state_t *state);
int  micro_native_open_host_menu(micro_native_t *);
```

Rust `NativeBridge` 对应 `set_orientation` / `portrait`、`set_zoom` /
`set_zoom_fit` / `zoom_percent`、`object_rect`、`take_host_action` /
`request_host_action`、`host_state`、`open_host_menu`。
`set_rotation` / `rotation` 仍保留，90/270 映射到竖屏，0/180 映射到横屏。
`micro_native_open_host_menu` 是测试/自动化入口；正常交互走齿轮或
`Cmd/Ctrl+,`。

命令行参数可以放在 launch target 之前或之后：

```bash
# 竖屏 OS，按显示器适配
npm run demo -- --orientation portrait --zoom fit

# 横屏、150%，直接跑单个 App
cargo run -p micro-host-sdl --features native -- \
  --orientation landscape --zoom 150 apps/counter/dist/counter.mbc

# 兼容旧写法：--rotate 90 等价于 --orientation portrait
cargo run -p micro-host-sdl --features native -- \
  --rotate 90 --zoom fit --smoke apps/counter/dist/counter.mbc

# 无头竖屏 smoke
cargo run -p micro-host-sdl --features native -- \
  --orientation portrait --zoom fit --os-smoke \
  apps/shell/dist/shell.mbc apps/counter/dist/counter.mbc \
  apps/settings/dist/settings.mbc apps/momo/dist/momo.mbc
```

## 6. 分辨率 / DPI / 窗口尺寸变化

窗口尺寸、像素尺寸、显示器变化仍统一走：

- `SDL_EVENT_WINDOW_RESIZED`
- `SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED`
- `SDL_EVENT_WINDOW_DISPLAY_SCALE_CHANGED`
- `SDL_EVENT_WINDOW_DISPLAY_CHANGED`
- `SDL_EVENT_WINDOW_MAXIMIZED` / `RESTORED`

处理方式：重新读取窗口/renderer 尺寸 → 重算 `view_scale/view_x/view_y`（扣掉
工具栏）→ 标记 `view_dirty`。拖动窗口边缘时画面连续跟随；方向切换不依赖窗口
事件，自身会重建纹理并重排。

高 DPI 下窗口点与输出像素分开计算，点击仍能正确反算。当前窗口未启用
`SDL_WINDOW_HIGH_PIXEL_DENSITY`，保持 1 点 ≈ 1 像素。

## 7. 测试

`crates/micro-host-sdl/tests/native_smoke.rs` 覆盖：

- 竖屏切换后逻辑尺寸交换为 `240×320`（测试用 `320×240` 窗口），画布居中、
  左右 pillarbox，坐标往返；
- 横屏/竖屏中心都保持居中；
- letterbox 区域点击被拒绝；
- **同一个已挂载的按钮在切换方向后按新宽度重新贴右边**（`object_rect` 从
  `x=220` 变为 `x=140`），证明是原地重排而不是重建；
- 根内容容器在横屏/竖屏下都填满 app bar 下方内容区（`object_rect` 高度
  `200` → `280`），tab 页内容根填满 48 px tab bar 以下区域；
- 竖屏下程序化点击仍能触发按钮；
- `set_zoom` / `set_zoom_fit` 的百分比报告与下限夹取；
- 宿主菜单动作经 SDL user event 入队、只消费一次，以及 power/orientation/
  zoom 状态快照的同步。

`scripts/test-native.sh` 额外跑一次竖屏 OS-shell smoke（shell → app → shell），
验证 Runtime 在 `480×800` 下能正常启动、切换和返回。

原生测试共享进程级互斥锁，因为 SDL/LVGL 持有全局状态，不能并行创建/销毁两个
原生窗口。

## 8. 已知限制

- `ui.place` 没有 flex/wrap/百分比；复杂布局必须用双边 anchor 表达。若 App
  保留大段固定 `left/width`，竖屏下仍可能溢出。
- 竖屏只改变逻辑分辨率，不改变 UI 树的语义；固定高度的卡片下方会有留白，
  没有按方向隐藏/重排不同内容。
- 原生 Device 菜单目前只有 macOS 实现；Windows/Linux 原生窗口尚未打包验证，
  非 Apple 构建返回“无菜单”，工具栏只保留方向按钮。
- Web Player 用 DOM 对话框提供等价的 Device 菜单（开关机、强制重启、方向、
  模拟器信息），但 Web 端方向切换是冷启动，不是原地重排；ESP32 宿主没有
  这个宿主菜单。
- 不支持双指捏合缩放；缩放走菜单/快捷键/滚轮/命令行。
- 分数缩放会让 LVGL 纹理变软（LVGL 按逻辑分辨率光栅化），整数倍缩放用
  最近邻保持清晰。

## 9. macOS 应用图标与 .app 包

宿主以前从终端裸跑，Dock 里显示的是通用可执行文件（控制台）图标。现在图标由
同一条流水线覆盖三种启动方式：

| 场景 | 图标来源 |
| --- | --- |
| 双击 `Micro OS.app` | `Contents/Info.plist` 的 `CFBundleIconFile` → `Resources/AppIcon.icns` |
| `npm run demo` / `cargo run` | 运行时 `NSApplication.applicationIconImage`（嵌入的 512 px PNG） |
| Finder / Cmd-Tab / Dock | 同上，进程级生效 |

### 图标源

`scripts/generate-app-icon.swift` 用 CoreGraphics/AppKit 直接绘制，无第三方依赖：

- macOS 超椭圆（superellipse n=5）squircle，深墨绿渐变 + 鼠尾草高光；
- 里面的 800×480 设备：深色 bezel（摄像头 + 陶土橙电源灯）、米白 paper 屏、
  40 px OS app bar（Wi-Fi / Home / 9:41）、2×2 启动器卡片（第一张 terracotta，
  另外三张 paper + 圆角 badge）；
- 16/32 px 改用简化构图（只有 squircle + 四张卡片），避免缩小后糊成一团。

```bash
npm run icon:generate        # swift scripts/generate-app-icon.swift
```

输出 `assets/app-icon/`：`AppIcon.icns`（直接写 ICNS 容器，不依赖 iconutil）、
`icon-1024.png`（母版预览）、`icon-512.png`（运行时嵌入）、`icon-128.png`
（文档预览）、`icon-32.png` / `icon-16.png`（小尺寸简化构图预览）。改完图标后重新构建宿主即可；`crates/micro-host-sdl/build.rs`
已把 `icon-512.png` 列入 `rerun-if-changed`。

### 运行时 Dock 图标

`crates/micro-host-sdl/src/app_icon.rs` 用 `include_bytes!` 把 512 px PNG 编进
宿主。`NativeBridge::create` 在**可见窗口**创建成功后调用
`micro_host_icon_set_png`（`native/src/micro_host_icon.m`）：

```objc
NSImage *image = [[NSImage alloc] initWithData:pngData];
[NSApplication sharedApplication].applicationIconImage = image;
```

无头 `--smoke` / `--os-smoke` 用 SDL dummy 驱动，不初始化 AppKit，因此不会走
这条路径。其他平台由 `micro_host_icon_stub.c` 提供返回 0 的同名函数。

### 打包成 .app

```bash
npm run bundle:macos                 # release → dist/Micro OS.app
npm run bundle:macos -- --debug      # debug 包（最快，图标/行为一致）
npm run start:macos                  # 打包后直接打开
```

Bundle 结构：

```text
dist/Micro OS.app/Contents/
├── Info.plist                  CFBundleIdentifier dev.micros.micro-os
├── MacOS/micro-os              宿主二进制（cargo build -p micro-host-sdl --features native）
├── Frameworks/
│   └── libmicro_native.dylib   C/ObjC 桥；install name 改为 @executable_path/../Frameworks
└── Resources/
    ├── AppIcon.icns
    └── apps/
        ├── shell.mbc           OS 首页
        ├── registry.txt        启动器顺序：counter, settings, momo
        ├── counter.mbc
        ├── settings.mbc
        └── momo.mbc
```

`Info.plist` 使用 `CFBundleName=Micro OS`、`CFBundleIconFile=AppIcon.icns`、
`LSMinimumSystemVersion=12.0`、`NSHighResolutionCapable=true`。宿主链接的
`libmicro_native.dylib` 会复制进 `Contents/Frameworks`，并用 `install_name_tool`
把 `@rpath` 改成 `@executable_path/../Frameworks`，因此 `cargo clean` 之后双击
仍然能启动。脚本最后 ad-hoc 签名 dylib 与 bundle（失败只告警，不影响本机运行），
并调用 `lsregister -f` 刷新 LaunchServices 的图标缓存。

### 双击启动（无命令行）

Finder 启动不带参数，因此 `crates/micro-host-sdl/src/main.rs` 的
`bundled_launch()` 会从当前可执行文件推出
`../Resources/apps`：读 `shell.mbc` 和 `registry.txt` 里的 App 顺序，按 OS
模式启动首页。开发时仍可显式传 `--os`，两者走同一套 `Launch::Os`。

如果 Finder 仍显示旧图标，`touch "dist/Micro OS.app"` 或重新登录一次；这是
LaunchServices 的缓存，不是图标生成失败。
