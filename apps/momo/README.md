# momo — 墨墨式背单词 Micro App

`apps/momo/app.ts` 是一个用受限 TypeScript 编写、编译为 MBC（v25）并在 micros 运行时上执行的背单词 App。它没有 HTML/JS App 运行时，界面树、状态、复习调度和持久化全部走 Micro App 的 `state` / `bind` / `ui` / `kv` / `config` / `clock` / `audio` 能力。

## 运行

```bash
# 编译所有 App（含 momo）
npm run build:app

# Web 播放器：构建 MBC + Wasm，再启动 Vite
npm run dev:web

# macOS 原生宿主（LVGL + SDL3）
npm run demo
```

Web 播放器启动后，点击桌面上的 **momo** 图标进入。数据先按 App id 隔离，App 内再按本机用户隔离（最多 5 个），分成两层：

- **`kv`（学习状态）**：每个用户写入 `<ns><key>`；默认用户（用户 1）的 `ns` 为空，沿用旧版键名，所以升级后原有数据自动归到默认用户，其他用户是 `u2:`…`u5:`。Web `localStorage["micro:kv:momo:<ns><key>"]`（每个标量一个 key）；macOS `~/.micros/kv/momo.json`（经 `micro-store::NativeStore`）；ESP32 NVS 命名空间 `momo`（`micro_cfg` 分区，经 `micro_kv` 组件，重启保留）。
- **`config`（配置）**：由 OS 统一 API 读写，App 不直接碰文件系统。Web `localStorage["micro:config:momo"]`（一个 JSON 对象）；macOS `~/.micros/momo/config.json`（经 `micro-store::NativeConfigStore`，可手工编辑）；ESP32 NVS 命名空间 `micro_config` 的 `momo` 键（`micro_cfg` 分区）。用户级 `goal`、`name` 和 `maimemoToken` 带同一用户前缀；全局只有 `userCount` / `activeUser`。名称和 Token 在**用户**页点「编辑」后一起修改并保存（默认只读显示名称和 Token 是否已配置，不显示 Token 内容），同步页只显示当前用户是否已配置 Token。

## 多用户与数据隔离

一个设备上最多 5 个本机用户，在 **用户** 页新建或切换：

- 用户 1 是“默认用户”，键前缀为空；用户 2–5 的前缀分别是 `u2:`…`u5:`。旧版没有前缀的键自动就是默认用户的数据，不需要迁移脚本。
- 学习状态（`w*` / `d*` / `s*` / `l*` / `pool` / `day` / `streak` 等）全部按前缀分开；`goal`、`name` 和 `maimemoToken` 在同一个 config 文档里也带前缀。
- 只有 `userCount` 和 `activeUser` 是无前缀的全局键：新建用户写 `userCount`，切换写 `activeUser`；启动时由第一个绑定恢复用户和前缀，所以下次打开仍是上次的用户。
- 新建用户只递增计数并切过去，不复制任何单词、名称或 Token；新用户第一次点“开始今日学习”时写入自己的 10 个引导词，目标默认 5。
- 当前用户的显示名由 `<ns>name` 保存（缺省时回退到“默认用户”/“用户 N”）；学习页顶部标题显示 `MOMO · <当前用户名>`，系统 AppBar 标题显示 `momo · <当前用户名>`（经 `os.setTitle`），切换、保存或启动时立即更新。
- 切换用户会停止发音、清空当前学习会话和同步统计，避免上一用户的单词、反馈或 Token 短暂残留。同步和在线词典的回调在发起时记录用户快照，切换后到达的响应会被丢弃，不会写进新用户的命名空间。
- 同步链是串行的：上一次同步还没结束（包括已切换用户、等待旧响应返回）时再次点同步只会提示“同步进行中”，不会并发发出第二个请求，避免旧响应覆盖新用户的 `syncUser` 快照。
- “重置学习进度”只删除当前用户的 `s*` / `l*` / `m*` / `r*` / `pool` / 快照等键，不会碰其他用户，也不会删 `goal` / `name` / `maimemoToken`。

## 界面

momo 采用「学习手帐」的视觉方向：暖米色画布、纸白卡片、鼠尾草绿状态胶囊和陶土色书脊线。所有背景、分隔线和卡片都由 `ui.surface` 绘制，文字与按钮仍使用现有 `ui.text` / `ui.button`；没有引入图片资源，因此在 Web、macOS 和 ESP32 上保持同一套布局。

- 学习页：顶部是今日目标与连续天数，中间一张纸白卡承载阶段、单词、例句和释义；发音按钮贴在单词右侧，卡片下方是「认识 / 不认识」操作区。“开始今日学习”只在未开始或本轮完成时出现，学习过程中隐藏。
- 进度页：当前 10 个活动槽位在单张卡片内按行排布，标题显示“本组 N 个 · 今日队列 M 个”。
- 设置页：新词目标和学习数据分成两张卡片。
- 同步页：今日统计一张卡片；上方 Token 卡片只显示“已配置/未配置”，并提示到用户页管理。Token 属于当前用户，切换后状态立即跟着切换。按钮下方固定提示「使用前请先在墨墨 App 打开『今日学习』并开启自动同步」，因为 `get_today_items` 依赖官方 App 初始化当天队列。
- 用户页：当前用户的资料卡默认只读，显示名称和「API Key · 已配置/未配置」；点「编辑」才展开名称与 API Token 输入框，可「保存」或「取消」；下方是「新建用户」、最多 5 个用户的切换列表和新建入口；当前用户按钮用陶土色，与未选中的绿色按钮区分。

`ui.surface` 的 tone 有 `canvas`、`paper`、`sage`、`terracotta`、`line` 五种；实现见 `sdk/index.d.ts`、`crates/micro-ir`、`native/src/micro_native.c` 和 `crates/micro-host-web`。

## 学习流程（墨墨式四段）

每个单词按固定顺序推进，每次只多揭示一层：

| 阶段 | 屏幕 | 用户看到 | 操作 |
| --- | --- | --- | --- |
| 0 | 听音辨义 | 仅读音提示（自动朗读 + “播放发音”） | 认识 / 不认识 |
| 1 | 看词辨义 | 单词拼写 + 国际音标提示 | 认识 / 不认识 |
| 2 | 例句回忆 | 例句挖空，空格处即目标词 | 认识 / 不认识 |
| 3 | 释义 · 自我评估 | 单词、例句、释义、例句中文翻译 | 认识 / 不认识 |

- 阶段 0–2 点“不认识”只揭示下一层，不改变记忆阶段。
- 阶段 3 点“不认识”才会降阶，并把这个词重新放回本轮队列（`last = now - gap`，立即可复习）。
- 任意阶段点“认识”即按当前阶段进阶，同时停留在当前词的例句和释义上，提示“点空白处继续下一个词”；轻点卡片或画布空白处才进入下一个词。
- 学习过程中不显示“开始今日学习”；本轮完成后它重新出现，页面上不再有“回到桌面”。
- 只有从未学过（`last === 0`）的词被评分时，才计入当天的“新词”目标计数。

## 艾宾浩斯调度

阶段 0–8 对应 9 个间隔：

```
5 分钟 → 30 分钟 → 12 小时 → 1 天 → 2 天 → 4 天 → 7 天 → 15 天 → 30 天
```

- 阶段 0 听音阶段直接“认识”：`stage = 3`（1 天）
- 阶段 0 看词阶段直接“认识”：`stage = 2`（12 小时）
- 其他情况“认识”：`stage += 1`（封顶 8）
- “不认识”：`stage = max(0, stage - 2)`，并立即到期
- 到期判断：`last + interval(stage) <= now`

所有词独立推进，没有全局“第几天”的硬编码；`last` 是每个词自己的上次复习时间。

## 每日计划

首次点“开始今日学习”时，如果 `w0` 为空，会先写入 10 个引导词；之后每次开始：

1. 用 `clock.now()` 计算 UTC+8 的日序号，跨天则把 `newToday` 归零。
2. 扫描当前活动槽（`rCount` 最多 10 个词），统计到期词 `due` 和从未学过的词 `unseen`。
3. 本轮总量 `roundTotal = due + unseen`。`goal` 是每日新词目标（默认 5），只用于顶部目标展示，不再把已同步的今日队列硬截断在 5 个。
4. 从 `scanFrom` 开始环形扫描，优先取到期词，再取新词。
5. 当前活动批全部完成后，如果 `pool` 里还有今日词，自动取下一批 10 个填入同样 10 个槽位并继续；只有 `pool` 也耗尽才显示“今日完成”。

`roundDone / roundTotal` 显示本轮进度；重试可能让 `roundDone` 略大于 `roundTotal`，界面会封顶显示。

## 数据键

下表省略用户前缀 `<ns>`（默认用户为空，用户 2–5 为 `u2:`…`u5:`），实际存储为 `<ns><key>`；例如用户 3 的活动槽是 `u3:w0`。

| 键 | 含义 |
| --- | --- |
| `w0`…`w9` | 当前活动批的单词拼写；首次点“开始今日学习”时写入 10 个引导词 |
| `d0`…`d9` | 中文释义 |
| `p0`…`p9` | 音标 |
| `e0`…`e9` | 英文例句 |
| `c0`…`c9` | 例句挖空（目标词替换为 `______`） |
| `t0`…`t9` | 例句中文翻译 |
| `rCount` | 当前活动批长度（1–10） |
| `pool` | 今日待学词的紧凑 JSON 数组（只存拼写），最多 64 个；不含释义和 `voc_id` |
| `poolCount` | `pool` 中的词数 |
| `poolNext` | 下一批从 `pool` 的哪个下标开始取 |
| `rw` / `re` / `rd` / `rt` / `rp` / `rm` | 当前评分词的反馈快照（拼写/例句/释义/翻译/音标/墨墨次数）；跨批加载后仍显示刚评的词 |
| `v0`…`v9` | 墨墨单词 `voc_id`，用于学习记录查询 |
| `m0`…`m9` | 墨墨学习次数 `study_count` |
| `r0`…`r9` | 墨墨上次反馈 `last_response` |
| `s0`…`s9` | 每个词的记忆阶段（0–8） |
| `l0`…`l9` | 每个词的上次复习时间（Unix ms）；`0` 表示从未学过 |
| `day` | 最近一次跨天检查的 UTC+8 日序号 |
| `newToday` | 当天已学新词数 |
| `lastDay` | 最近连续学习的日序号 |
| `streak` | 连续学习天数 |

配置键存放在 `config` 文档（与学习状态分开）。带 `<ns>` 的键按用户隔离，`userCount` / `activeUser` 是全局键：

| 键 | 含义 |
| --- | --- |
| `userCount` | 已创建的本机用户数（1–5，无前缀） |
| `activeUser` | 当前用户序号（1–5，无前缀）；启动时据此恢复前缀 |
| `<ns>goal` | 当前用户的每日新词目标（3 / 5 / 8，仅作展示目标；同步队列会继续学到耗尽） |
| `<ns>name` | 当前用户的显示名；为空时使用默认名 |
| `<ns>maimemoToken` | 当前用户的可选墨墨开放 API Token（明文保存在 OS config 文件/存储中） |

“重置学习进度”只作用于当前用户：删除其 `s*` / `l*` / `m*` / `r*` / `day` / `newToday` / `lastDay` / `streak`，清空 `pool` / `poolCount` / `poolNext` 和评分快照 `rw` / `re` / `rd` / `rt` / `rp` / `rm`；保留单词、释义和 `config` 里的 `goal` / `name` / `maimemoToken`。需要连点两次确认。

### 从旧版本升级

旧版 momo 把 `goal` / `maimemoToken` 放在 `kv` 里，且原生数据在 `~/.micrapp`。新版本会自动迁移，不需要手动改文件：

- 原生宿主首次启动时把 `~/.micrapp` 整体迁到 `~/.micros`（若 `~/.micros` 已存在则只补齐缺失文件，不覆盖新数据）。
- 多用户版本没有额外的数据迁移：没有前缀的旧键就是“默认用户”的键，升级后首次打开会显示默认用户；其他用户从空数据开始。
- `config.get*` 在配置文档里没有该键时，会回退读取同一 App 的 `kv`，所以旧 Token / 配额会直接出现在设置和用户页。
- 第一次通过 `config.set*` 改写该键时，值会写入 `<app>/config.json` 并从 `kv` 中删除，完成一次性提升。

### 为什么是“64 个紧凑池 + 10 个活动槽”

ESP32 的 `micro_cfg` NVS 分区只有 64 KiB，而且 NVS 还要存每个 key 的元数据。一个带释义、音标、例句、挖空和翻译的完整词条约 350–550 B；10 个约 5 KB，64 个就会到 30 KB 以上，和配置、其他 App 共用分区时很容易写满。因此：

- `pool` 只存拼写，64 个通常不到 1 KB，单个 NVS value 不会超过 4 KiB 读取缓冲。`voc_id` 只保留在当前 10 个活动槽里，够首次 `query_study_records` 使用；后续批次不再重复存 68 字节的 ID。
- `w0…w9` 仍保存完整词条，但只服务当前活动批；在线词典一次也只查 10 个（`MAX_WORDS = 10`）。
- 当前批学完后，App 从 `pool` 取下一批 10 个复用这些槽位，并异步补全释义。磁盘占用不随“今日队列”线性增长，学习却能继续。

## 接入真实墨墨数据

参考文档：<https://open.maimemo.com/#/>。同步页不再只是统计：它把账号今天**未完成**的单词写入本机队列，最多缓存 64 个；其中前 10 个立即进入活动槽，并在原生端用在线词典补全释义、例句和音标。这样既避免 ESP32 的 64 KiB NVS 被几十个词的完整释义撑爆，也能在学完 10 个后继续。

- 在用户页点「编辑」，填写名称和 Token 并保存（原生输入框支持键盘和 `Cmd+V`），再到同步页点“同步墨墨”；请求 `POST /open/api/v1/memo/study/get_today_items`，带 `Authorization: Bearer …`、`Accept` 和 `Content-Type` 头。
- `get_today_items` 在墨墨服务端尚未初始化当天队列时会返回空数组（官方文档要求当天先在 App 中打开今日学习，并在 App 中开启自动同步）。momo 遇到空数组不会清空已有 `pool`，同步页会提示先到 App 初始化；已有活动批仍可继续学习。
- 响应通过 `json.length` / `json.getText` 读取 `data.today_items`，统计 `is_finished`；把所有 `is_finished !== "true"` 的词拼写按顺序写入紧凑的 `pool` JSON（最多 64 个），再把前 10 个写入 `w0…w9` 活动槽（活动槽同时保留 `voc_id`）。单词变化时清空该槽的 `s/l/d/e/c/t/p/m/r`，相同则保留本地复习阶段。
- 随后串行调用 `POST /api/v1/memo/study/get_study_progress`，把今日完成数、计划数和学习时长（毫秒换算为分钟）显示在同步页。
- 再调用 `POST /api/v1/memo/study/query_study_records`，按当前活动批的 `voc_ids`（最多 10 个）取每词的学习次数、上次反馈和标签。`study_count` 与 `last_response`（`FAMILIAR/VAGUE/FORGET/WELL_FAMILIAR`）写入 `m*/r*`；如果本地还没有该词的学习记录（`l*=0`），用它们推断初始复习阶段并设为当前到期，避免刚导入的词被未来复习时间挡住。
- 选中词拼成换行分隔的列表，调用 `net.lookupWords`。macOS 原生宿主在 worker 线程逐个查询有道公开 JSON 词典，返回 `entries`（`word / phonetic / definition / example / cloze / translation`），App 再按单词写回对应槽位。
- 当前活动批长度写入 `rCount`；少于 10 个时，多出的旧槽不会进入本轮扫描。`poolNext` 指向下一批起点；当前批学完后自动取下一批，不额外占 NVS key。同步成功后 `idx`、本轮计数、`poolNext` 和当前阶段会重置，避免继续显示旧词。
- 墨墨 OpenAPI 本身只返回 `voc_spelling` 等词表字段，**没有内置释义、例句、音频**，也没有提交复习结果的公开端点；因此释义走在线词典，复习曲线仍由本机艾宾浩斯调度维护，不会向墨墨提交评分。
- 名称和 Token 通过 OS `config` API 按用户保存（macOS `~/.micros/momo/config.json` 的 `<ns>name` / `<ns>maimemoToken`、Web localStorage、ESP32 NVS）；用户页的保存按钮一次写入两者，取消则丢弃本次草稿，同步页只读取当前用户的 Token。在线词典请求只发送单词拼写，不发送 Token。

用真实账号跑一次同步 + 词典链路（token 只放环境变量，测试使用临时 KV/config，不会写你的 `~/.micros`）：

```bash
MAIMEMO_TOKEN='...' cargo test -p micro-host-sdl --features native \
  --test momo_native native_sync_loads_today_items_from_maimemo -- --ignored
```

平台差异的根源是传输能力，而不是 UI：

- macOS 原生宿主通过内置的 `micro-net`（ureq + rustls）在 worker 线程发 HTTPS 请求，支持任意请求头，不再调用 `curl` 等外部命令。
- Web 宿主的 `fetch` 受浏览器 CORS 限制，而 Maimemo 的响应没有 `Access-Control-Allow-Origin`，所以 `net.httpRequestWithHeaders` 返回 `HTTP 501` 和一条 JSON 错误；需要自建代理才能绕过。
- ESP32 固件通过 `esp_http_client` + mbedTLS 证书包实现 HTTPS、自定义请求头和 4xx/5xx 响应体，同步与在线词典走同一套 `micro-net` 逻辑，同样在 worker 线程执行，不阻塞 LVGL。

### 开放 API 覆盖范围

| API | 是否接入 | 原因 |
| --- | --- | --- |
| `study/get_today_items` | 是 | 今日学习队列的唯一来源 |
| `study/get_study_progress` | 是 | 今日完成数、计划数、学习时长 |
| `study/query_study_records` | 是 | 每词学习次数、上次反馈和标签，用于初始化本地阶段 |
| `memo/vocabulary`、`vocabulary/query` | 否 | `today_items` 已经带 `voc_id`，没有额外信息 |
| `memo/interpretations`、`memo/phrases` | 否 | 只返回“自己创建的释义/例句”，不是内置内容；当前用在线词典兜底 |
| `memo/notes` | 否 | 助记没有对应 UI，且同样是用户自建内容 |
| `study/add_words`、`study/advance_study` | 否 | 写操作；前者会改墨墨计划，后者只提前复习且不能提交评分 |
| `memo/notepads`、`markji/*` | 否 | 云词本/记忆卡管理与背单词主流程无关 |

`micro-json` 的路径查询、`net.httpRequestWithHeaders`、`net.lookupWords` 与 MBC v25 标签见 [docs/host-api.md](../../docs/host-api.md)。

平台侧已经补齐本 App 需要的持久化与时间基础组件：三端统一的 `micro-store::ScopedKv`、统一的 `config` API（macOS `~/.micros/momo/config.json`、Web localStorage、ESP32 NVS `micro_config`）、ESP32 的 `micro_kv`（`micro_cfg` NVS 分区）和 `micro_clock`（SNTP）。账号同步和在线词典在 macOS 与 ESP32 上启用（两端都是宿主内置 HTTP + TLS，不依赖外部命令）；Web 仍受浏览器 CORS 限制，可用 10 个引导词体验完整四段流程；Web 的紧凑队列分页逻辑与原生端一致。

## 平台差异

| 能力 | Web | macOS 原生 | ESP32 |
| --- | --- | --- | --- |
| KV 学习状态 | 每 key 一条 `localStorage` | `~/.micros/kv/momo.json` | NVS `micro_cfg` 分区（命名空间 `momo`） |
| `config` 配置 | `localStorage["micro:config:momo"]` | `~/.micros/momo/config.json` | NVS `micro_config` 的 `momo` 键 |
| `clock.now()` | 墙钟（Unix ms） | 墙钟（Unix ms） | SNTP 墙钟；未同步前回退 uptime |
| `audio.speak` | `speechSynthesis`（先 cancel，en-US 音色） | `AVSpeechSynthesizer`（原生桥，Samantha 优先） | no-op（板端无 codec） |
| `audio.playAudio` / `playBackground` | 多个 `HTMLAudioElement`；后台 BGM 为页面级单例 | 多个 `AVPlayer`；后台 BGM 为全局 `AVPlayerLooper` | no-op；`playAudio` 返回 0 |
| 墨墨同步 | 501（CORS） | 内置 HTTPS（ureq + rustls） | 内置 HTTPS（`esp_http_client` + 证书包） |
| 在线词典 | 501（CORS/无原生词典） | 有道 JSON 词典（内置 HTTPS） | 有道 JSON 词典（内置 HTTPS） |

`audio.speak / stop / setRate` 是三端统一的系统 TTS 接口：macOS 由 `native/src/micro_speech.m` 维护一个常驻 `AVSpeechSynthesizer`（不再为每个单词 fork `say`），Web 走浏览器 `speechSynthesis`，ESP32 保留 no-op 直到板端 codec 接通。连续的 `speak` 会替换上一句，本轮完成和“重置学习进度”会调用 `audio.stop()`，避免旧发音继续播。

ESP32 启动后 `micro_clock` 会用 `pool.ntp.org` 做 SNTP 同步；Wi-Fi 首次连上前的短窗口内 `clock.now()` 回退到 uptime，之后恢复真实墙钟。跨天边界因此以真实自然日为准。

## 文件

- `apps/momo/app.ts` — 唯一的 App 源码（状态、绑定、界面、学习算法）
- `apps/momo/dist/momo.mbc` — 编译产物（被 gitignore）
- `apps/shell/app.ts` — 桌面第三个图标注册
- `products/micro-web-player/src/main.js` — Web 播放器加载列表
- `firmware/micro-os-esp32/main/CMakeLists.txt` — ESP32 多 App 拼接顺序
