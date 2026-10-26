# Host API: clock / kv / config / audio / HTTP headers / JSON

MBC v28 makes `ui.show` work for every widget — a hidden container hides its
children, so a read-only profile card and its edit form can share one layout
slot — and adds `os.setTitle` for the OS app-bar title. MBC v27 adds the
optional manifest capability allow-list enforced at the host-call boundary
(see [Manifest capabilities](#manifest-capabilities)). MBC
v26 adds the `ui.icon` widget and the App-wide shared OS icon catalog
(`micro_ir::icons`), plus the tappable-surface `label` used for accessible
launcher cards. See [ui-icons.md](ui-icons.md). MBC v25 adds the App-private
`storage.*` file calls backed by the external TF card on ESP32. MBC v24 adds URL
audio and the single background-music stream.
MBC v23 adds `audio.stop` and `audio.setRate`. MBC v22 adds the per-App
`config.*` host calls. MBC v21 added the decorative
`ui.surface` UI kind (cards, pills, rules and page canvases) used by the momo
dashboard. A surface is non-interactive by default;
`ui.surface(tone, { onClick })` turns it into a blank tap target (used for
“tap anywhere to continue”). MBC v20 adds the dictionary lookup used by the momo
study queue, on top of the
MBC v19 header-aware HTTP request and JSON query, and the MBC v18 clock / KV /
audio capabilities. Most calls are
synchronous scalar reads or writes; the `net.*` requests are asynchronous and
deliver their result through a callback. All calls are available in bindings
and handlers, except writes and async requests, which are handlers-only.
`os.setTitle` is the one chrome-only exception: it may run inside a binding
because the launch binding uses it to restore the title on the first frame.

## Manifest capabilities

`app({ id, name, icon, capabilities: [...] })` declares which host API categories
an App may call. The compiler validates the names (`MTS002` for an unknown or
duplicate entry) and stores the list in the MBC metadata section.

| Capability | Calls |
| --- | --- |
| `clock` | `clock.now` |
| `kv` | `kv.getNumber`, `kv.getText`, `kv.setNumber`, `kv.setText`, `kv.remove` |
| `config` | all `config.*` calls (`getNumber`, `getText`, `setNumber`, `setText`, `remove`) |
| `storage` | all `storage.*` calls (`available`, `exists`, `readText`, `writeText`, `appendText`, `list`, `remove`, `mkdir`, `size`) |
| `net` | all `net.*` calls (`wifi*`, `scanWifi`, `http*`, `lookupWords`) |
| `audio` | all `audio.*` calls, including URL and background playback |
| `json` | `json.getText`, `json.getNumber`, `json.length` |
| `device` | all `device.*` calls (`name`, `chip`, memory, reset, backlight, GPIO) |
| `os` | `os.appName`, `os.appIcon`, `os.launchIndex`, `os.goBack`, `os.setTitle`, `os.delay` |

Semantics:

- The gate is default-deny: only the categories in the declared list are
  callable. `Runtime` wraps the host in a capability gate and a call outside the
  list fails with `VmError::Host` before the host implementation runs.
- `microc` rejects a host call outside the manifest at the call site (`MTS002`),
  so under-declaring fails the build rather than the first launch.
- Omitting `capabilities` or writing `capabilities: []` denies every host call.
- The manifest is still unauthenticated: signing and install-time grants are
  missing, so this is a correctness boundary, not yet a security boundary.
- The gate is per category, not per function: declaring `net` permits HTTP and
  Wi-Fi calls. Finer-grained grants are future work.

All four shipped Apps declare exactly what they use: `counter`
(`device`, `net`), `momo` (`audio`, `clock`, `config`, `json`, `kv`, `net`, `os`),
`settings` (`audio`, `config`, `device`, `net`, `os`, `storage`) and `shell`
(`os`).

## `clock.now(): number`

Milliseconds since the Unix epoch.

| Host | Implementation |
| --- | --- |
| Web (`micro-host-web`) | `js_sys::Date::now()` |
| macOS (`micro-host-sdl`) | `SystemTime::now()` since `UNIX_EPOCH` |
| ESP32 (`micro-host-esp32`) | SNTP wall clock (`micro_clock`, pool.ntp.org); falls back to uptime before the first sync |

The value is sampled each call. It is **not** a reactive state source: a
binding that calls `clock.now()` only re-evaluates when one of its `state` or
`bind` dependencies changes.

## `kv`

Per-App persistent key-value storage. The host namespaces every key by the App
id, so two Apps cannot collide. Reads are synchronous; a missing key returns
`0` (`getNumber`) or `""` (`getText`).

```ts
declare const kv: {
  getNumber(key: string): number;
  getText(key: string): string;
  setNumber(key: string, value: number): void;
  setText(key: string, value: string): void;
  remove(key: string): void;
};
```

| Host | Backing store |
| --- | --- |
| Web | `localStorage["micro:kv:<app-id>:<key>"]`, one JSON scalar per key |
| macOS | `~/.micros/kv/<app-id>.json` via `micro-store::NativeStore` |
| ESP32 | NVS namespace `<app-id>` on the `micro_cfg` partition, via the `micro_kv` firmware component |

All three hosts implement the `micro-store` `ScopedKv` contract, so the
namespace/type behavior is shared. The Runtime still exposes the typed
`kv.getNumber` / `getText` calls rather than the design's union
`kv.get` / `kv.set`; the underlying store is no longer host-specific.

The NVS value encoding is type-tagged (`n:`, `s:`, `b:`, `z`) so a number
written by `setNumber` reads back as a number after reboot. NVS keys and
namespaces must be shorter than 16 bytes, matching the ESP32 NVS limit; the
adapter rejects longer ones instead of letting NVS truncate them.

## `config`

Per-App **configuration**, as one flat JSON document provided by the OS. This
is deliberately separate from `kv`: `kv` is App state (review stages, counters,
cache), while `config` is the human-editable / deployable configuration an App
reads at startup. The API is identical to `kv`, so App code never sees paths or
per-platform storage details.

```ts
declare const config: {
  getNumber(key: string): number;
  getText(key: string): string;
  setNumber(key: string, value: number): void;
  setText(key: string, value: string): void;
  remove(key: string): void;
};
```

| Host | Backing store |
| --- | --- |
| Web | `localStorage["micro:config:<app-id>"]`, one JSON object |
| macOS | `~/.micros/<app-id>/config.json` via `micro-store::NativeConfigStore` (pretty-printed, user-editable) |
| ESP32 | one JSON string in NVS namespace `micro_config`, key `<app-id>`, on the `micro_cfg` partition |

The document format is shared by all three hosts (`micro-store::ConfigDoc`):
flat string keys, scalar values, and the same `n:` / `s:` / `b:` / `z` typing
on NVS. Nested JSON values are ignored by the reader rather than failing the
whole document. Writes are handlers-only, like `kv` writes and async `net.*`
calls; reads work in both bindings and handlers.

Because the config file can be seeded before the App launches, an App input can
act as an override: momo's 用户 tab reads the display name and token through
`config.getText` with the active user's `<ns>` prefix (the default user's prefix
is empty, so a pre-provisioned legacy key still matches) and saves both through
`config.setText` with one action, so a token shows up in the field and can be
replaced without the App touching `~/.micros`, `localStorage`, or NVS directly.
The 同步 tab only reads the token and shows whether it is configured.

Apps that kept config-like values in `kv` before this API existed are migrated
without App changes:

* the native host moves a pre-`~/.micros` `~/.micrapp` tree into `~/.micros` on
  first launch (merging into an existing root without overwriting it);
* `config.get*` falls back to the same key in the App's `kv` namespace when the
  config document has no such key;
* the first `config.set*` for that key writes it to the config document and
  removes the legacy `kv` entry, so the old value is promoted exactly once.

This is how momo keeps the name, token and goal that older builds stored in
`~/.micrapp/kv/momo.json` (or the equivalent Web/ESP32 `kv` entry) for the
default user; users 2–5 use `u2:`–`u5:` prefixed config keys and never read
another user's legacy values.

## `audio`

The App calls one API; each host uses the platform's own synthesizer or media
player. URL audio streams over HTTP(S); TTS never uses the network.

```ts
declare const audio: {
  speak(text: string): void;
  stop(): void;
  setRate(rate: number): void;
  playAudio(url: string): number;
  stopAudio(handle: number): void;
  stopAllAudio(): void;
  playBackground(url: string): void;
  stopBackground(): void;
};
```

### Speech

- `speak(text)` trims the text and replaces the utterance already in flight, so
  tapping pronunciation repeatedly only hears the word on screen.
- `stop()` cancels the current utterance and is safe to call when idle. It does
  **not** stop URL audio.
- `setRate(rate)` sets the multiplier used by later calls: `0.5` is slow,
  `1.0` is normal, `2.0` is fast. Hosts clamp it to `0.5..2.0`; the default is
  `0.9`, slightly slower for vocabulary practice. The rate survives across
  `speak` calls until changed again.

### URL audio

- `playAudio(url)` starts a **foreground** stream and returns a positive handle,
  or `0` when the URL is empty/not HTTP(S) or the platform cannot start it.
  Every call creates an independent player, so several sounds mix.
- `stopAudio(handle)` stops one foreground stream; unknown handles are a no-op.
- `stopAllAudio()` stops every foreground stream.
- Foreground streams stop automatically when the App returns to the shell or
  switches to another App.
- `playBackground(url)` starts or replaces the single **system-wide** background
  music stream. It loops and keeps playing after the App leaves the foreground.
- `stopBackground()` stops that stream. It is safe to call when idle.
- Only `http://` and `https://` are accepted. `file://` is rejected so an App
  cannot use the player as a local-file reader.
- The browser autoplay policy still applies to the first sound; a rejected
  `play()` is silent, not fatal. Background music is also subject to the
  browser's tab/media policies.

All audio calls are actions (handlers only, like `kv.set*`); they are rejected
inside bindings so a re-render never starts speech or a player.

| Host | Speech | URL audio |
| --- | --- | --- |
| Web | `speechSynthesis`; cancels the previous utterance, selects an `en-US` voice (Samantha preferred), then speaks with the current rate | one `HTMLAudioElement` per foreground handle; background music is a separate page-lifetime element so it survives an App switch |
| macOS | `AVSpeechSynthesizer` in `native/src/micro_speech.m`; one shared synthesizer, Samantha preferred, new words replace the old one | `AVPlayer` per foreground handle; `AVQueuePlayer` + `AVPlayerLooper` for the global background stream in `native/src/micro_audio.m` |
| ESP32 | no-op until an I2S/codec sink is wired; `stop` / `setRate` stay as explicit arms for that future board | no-op until the same codec exists; `playAudio` returns `0` and the background calls are accepted but silent |

## `net.httpRequestWithHeaders(method, url, headers, body, onResult)`

Like `net.httpRequest`, but the third argument is a newline-delimited header
block. The callback receives `"HTTP <status>\n<body>"`; 4xx/5xx response bodies
are preserved so an App can display the API error.

```ts
net.httpRequestWithHeaders(
  "POST",
  "https://open.maimemo.com/open/api/v1/memo/study/get_today_items",
  "Authorization: Bearer TOKEN\nAccept: application/json\nContent-Type: application/json",
  "{}",
  (response) => { /* parse with json.* */ },
);
```

| Host | Implementation |
| --- | --- |
| Web (`micro-host-web`) | Returns `HTTP 501` with a JSON error: browser `fetch` cannot set `Authorization` across Maimemo's missing CORS headers |
| macOS (`micro-host-sdl`) | In-process `micro-net` transport (ureq + rustls + bundled Mozilla roots) on a worker thread; HTTPS and custom headers are native, with no `curl` process |
| ESP32 (`micro-host-esp32`) | In-process `esp_http_client` (mbedTLS + common certificate bundle) on a worker thread; HTTPS, custom headers, redirects and 4xx/5xx bodies all work |

Both native hosts validate the method, URL, header block (including rejection
of `Host`/`Content-Length`/`Connection` injection) and body size in
`micro-net` before the platform transport runs. The request runs off the UI
thread and posts its result back through the Runtime's normal result queue, so a
slow network call does not block LVGL. `net.httpRequest` and `net.httpGet` use
the same native transport; Web keeps its simulator for those two calls because
the browser only exposes `fetch` to JavaScript.

## `net.lookupWords(words, onResult)`

Look up newline-separated English words and call back with a JSON document:

```ts
net.lookupWords("abandon\nabsorb\n", (response) => {
  // response = "HTTP 200\n{\"entries\":[{...}]}"
});
```

Each entry has `word`, `phonetic`, `definition`, `example`, `cloze`, and
`translation` strings. `cloze` is generated by replacing the word in the
example with `______`; missing fields are empty strings. The callback uses the
same `HTTP <status>\n<body>` envelope as the HTTP calls.

| Host | Implementation |
| --- | --- |
| Web (`micro-host-web`) | Returns `HTTP 501` with a JSON error; the browser/client has no native dictionary bridge |
| macOS (`micro-host-sdl`) | Worker thread calls Youdao's public JSON dictionary (`dict.youdao.com/jsonapi`) through `micro-net` (ureq + rustls), one request per word, and flattens the first `ec` entry |
| ESP32 (`micro-host-esp32`) | Worker thread runs the same `micro-net` dictionary batch through `esp_http_client` over HTTPS; no TLS-specific App code |

The native path is what lets momo study real `today_items` even though the
Maimemo OpenAPI does not expose built-in definitions or examples. Only word
spellings are sent; the Maimemo Bearer token never leaves the Maimemo request.

## `storage`

App-private external storage. On the ESP32-S3 board this is the removable TF
card mounted at `/sdcard`; on macOS it is a sandboxed directory under the store
root; the Web Player simulates it with localStorage. App code never sees a host
path.

```ts
declare const storage: {
  available(): boolean;
  exists(path: string): boolean;
  readText(path: string): string;
  writeText(path: string, text: string): boolean;
  appendText(path: string, text: string): boolean;
  list(path: string): string;
  remove(path: string): boolean;
  mkdir(path: string): boolean;
  size(path: string): number;
};
```

| Host | Backing location |
| --- | --- |
| Web (`micro-host-web`) | `localStorage["micro:fs:file:<app-id>:<path>"]` / `micro:fs:dir:...` |
| macOS (`micro-host-sdl`) | `<MICROS_HOME>/sdcard/micros/apps/<app-id>/<path>` (default `~/.micros`) |
| ESP32 (`micro-host-esp32`) | `/sdcard/micros/apps/<app-id>/<path>` on the mounted TF card via `micro_sd` |

The launcher's installable external-App directory is separate and is **not**
reachable through `storage`: `/sdcard/apps` on ESP32,
`<MICROS_HOME>/sdcard/apps` on macOS and `/sdcard/apps` in the Web simulator.
Only the OS registry scan reads it. See
[docs/external-apps.md](external-apps.md).

### Isolation contract

The path is the only input, and it is always interpreted inside
`micros/apps/<app-id>/`:

* empty path = the App's own storage root (for `list`, `exists`, `mkdir`,
  `size`); file reads/writes require a non-empty path;
* absolute paths, `.`/`..`, backslashes, `:`, control characters, empty
  components, symlinks, Windows device names and non-ASCII names are rejected
  before any filesystem call;
* components are 1–63 bytes, at most 8 levels, 240 bytes total;
* the native backend canonicalizes the parent and checks it is still below the
  App root; the ESP32 C backend re-validates the App id/path and prepends the
  fixed `/sdcard/micros/apps/<app-id>/` prefix independently of Rust, so a
  single-layer bug cannot cross the boundary;
* `remove` is recursive only inside the App's own directory.

`readText` returns `""` for a missing file or when storage is unavailable;
`writeText`/`appendText`/`remove`/`mkdir` return `false`; `size` returns `-1`.
`writeText` and `mkdir` create missing parents. Files are UTF-8 and capped at
64 KiB; `list` is capped at 128 sorted immediate children, with `/` appended to
directories. `available()` is false when the card is absent or the mount failed
— the OS keeps booting and Apps can fall back to `kv`/`config`.

Reads (`available`, `exists`, `readText`, `list`, `size`) are allowed in
bindings. Mutations (`writeText`, `appendText`, `remove`, `mkdir`) are
handlers-only even though they return a boolean, matching `kv`/`config` writes.

### ESP32 wiring

The board (Spotpear ESP32-S3 Touch LCD 7 V1.2 / Waveshare equivalent) wires the
TF slot to SPI: MOSI GPIO11, MISO GPIO13, SCLK GPIO12, and chip-select through
CH422G EXIO4 (active low). The firmware `micro_sd` component holds EXIO4 low
while preserving the backlight/touch-reset bits, mounts SDSPI with
`GPIO_NUM_NC` as the GPIO chip-select, enables FATFS long-file-name support, and
never formats a card. The card must already contain a FAT12/16/32 filesystem
(exFAT is not enabled in this build). Mount failure is logged and non-fatal; the
mount is created at boot and per-App directories are created on first
write/mkdir. Reboot after inserting a card that was absent at boot.

## `json`

Read-only queries over a JSON string, typically an HTTP response. The host
exposes a path reader rather than JSON objects/arrays because the App dialect
has no arrays or record expressions.

```ts
declare const json: {
  getText(text: string, path: string): string;
  getNumber(text: string, path: string): number;
  length(text: string, path: string): number;
};
```

Paths are dot-separated (`data.today_items.0.voc_spelling`, `data.progress.finished`).
A leading non-JSON prefix, including an `HTTP 200\n` status line, is skipped.
Missing paths return `""` / `0`; malformed JSON never panics.

## Shared UI icons (`ui.icon`)

MBC v26 introduces a name-based icon resource shared by every App, so a widget
never ships its own font or image. `ui.icon(name, options)` accepts a catalog
name (`"settings"`, `"trash"`, `"wifi"`, …) or a single legacy display glyph,
and resolves it per host: LVGL paints the matching glyph from the generated
Font Awesome subset font, the Web Player paints the catalog's inline stroke SVG. Both read the same registry
in `crates/micro-ir/src/icons.rs` and the same appearance tokens:

```ts
ui.icon("settings", {
  textStyle: { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 },
  tone: "muted",          // ink | muted | accent | inverse
  variant: "badge",       // plain | badge (circular field)
  onClick: () => { … },
});
```

`textStyle.size` selects the glyph size (12/14/18/24/32; default 24) and is the
only size input. A bound name (`bind(() => os.appIcon(i))`) re-resolves at
runtime through `RenderPatch::SetIcon`, so changing an App's icon does not
rebuild the tree. An unknown multi-character name is a compile error; the
renderer degrades it to `?` with a diagnostic on the hosts. The App manifest
`icon` accepts the same catalog names (the OS launcher then paints a shared
icon instead of a letter). `ui.surface(tone, { onClick, label })` now accepts a
literal `label`, which the Web host exposes as `role="button"` +
`aria-label` so a card made of decorative text/icon siblings stays operable.

See [ui-icons.md](ui-icons.md) for the full catalog and rendering contract.

## `os.setTitle(title): void`

Replaces the centred OS app-bar title for the running App (MBC v28). It only
touches chrome: it does not mutate App state, the platform applies it after the
current tick, and booting another App resets the bar through the normal
`set_app_chrome` path. Use it for a profile-aware title such as
`momo · 小墨`. Unlike other
side-effecting calls it is allowed inside a binding, because the launch binding
restores the title on the first frame.

## Conditional visibility (`ui.show`)

`ui.show(widget, visible)` stores a visibility source on any node (`visible` in
`UiNodeSpec`, MBC v28). A bound value must be Boolean and a constant must be
`true`/`false`; the runtime validates both. A hidden node keeps its layout slot
but paints nothing and receives no clicks, and hiding a container hides its
children — that is how a read-only card hides its edit form. The runtime applies
the initial value right after creating the tree, so a form never flashes before
its first state change, and later binding flips emit `SetVisible` patches like
any other bound value.

## Encoding

| Kind | Tag | Result |
| --- | --- | --- |
| `ClockNow` | 23 | number |
| `KvGetNumber` | 24 | number |
| `KvGetText` | 25 | string |
| `KvSetNumber` | 26 | none |
| `KvSetText` | 27 | none |
| `KvRemove` | 28 | none |
| `AudioSpeak` | 29 | none |
| `NetHttpRequestWithHeaders` | 30 | async callback |
| `JsonGetText` | 31 | string |
| `JsonGetNumber` | 32 | number |
| `JsonLength` | 33 | number |
| `ConfigGetNumber` | 35 | number |
| `ConfigGetText` | 36 | string |
| `ConfigSetNumber` | 37 | none |
| `ConfigSetText` | 38 | none |
| `ConfigRemove` | 39 | none |
| `AudioStop` | 40 | none |
| `AudioSetRate` | 41 | none |
| `AudioPlayUrl` | 42 | number (handle, `0` = failed) |
| `AudioStopUrl` | 43 | none |
| `AudioStopAllUrls` | 44 | none |
| `AudioPlayBackground` | 45 | none |
| `AudioStopBackground` | 46 | none |
| `StorageAvailable` | 47 | bool |
| `StorageExists` | 48 | bool |
| `StorageReadText` | 49 | string |
| `StorageWriteText` | 50 | bool |
| `StorageAppendText` | 51 | bool |
| `StorageList` | 52 | string |
| `StorageRemove` | 53 | bool |
| `StorageMkdir` | 54 | bool |
| `StorageSize` | 55 | number |
| `OsSetTitle` | 56 | none |

`VERSION` is `28` in `crates/micro-ir/src/codec.rs`. A host request may carry up
to four typed arguments (`net.httpRequestWithHeaders` is the first to use that
many). The compiler allow-list and arity table are in
`crates/micro-compiler/src/parse.rs` and `crates/micro-compiler/src/lower.rs`;
the JSON parser lives in `crates/micro-json`.
