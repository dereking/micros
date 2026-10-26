type Scalar = number | string | boolean | null;

interface State<T extends Scalar> {
  value: T;
}

interface Binding<T extends Scalar> {
  readonly __binding: T;
}

interface UiNode {
  readonly __node: unique symbol;
}

type UiTextStyle =
  | { font: "uiSans"; size: 12; weight: "regular"; lineHeight: 14 }
  | { font: "uiSans"; size: 14; weight: "regular"; lineHeight: 18 }
  | { font: "uiSans"; size: 18; weight: "regular"; lineHeight: 24 }
  | { font: "uiSans"; size: 24; weight: "regular"; lineHeight: 32 }
  | { font: "uiSans"; size: 32; weight: "regular"; lineHeight: 40 };

declare function state<T extends Scalar>(initial: T): State<T>;
declare function bind<T extends Scalar>(read: () => T): Binding<T>;

/**
 * Host API categories the app may call. The runtime is default-deny: only the
 * listed categories are callable. Omitting the field, or passing `[]`, denies
 * every host call.
 */
type Capability =
  | "clock"
  | "kv"
  | "config"
  | "storage"
  | "net"
  | "audio"
  | "json"
  | "device"
  | "os";

/**
 * Declare the app manifest. `id` is a stable identifier used to launch the
 * app, `name` is shown under its launcher icon, and `icon` is either a shared
 * OS icon name from the `ui.icon` catalog (e.g. "settings", "edit", "plus") or
 * a single legacy display glyph rendered in the platform font.
 */
declare function app(options: {
  id: string;
  name: string;
  icon: string;
  capabilities?: Capability[];
}): void;

/** ESP32 hardware capabilities. Sync reads are sampled when the binding runs. */
declare const device: {
  name(): string;
  chip(): string;
  flashBytes(): number;
  psramBytes(): number;
  resetReason(): string;
  backlight(): number;
  setBacklight(level: number): void;
  /** Configure a GPIO: "in" | "out" | "in-pullup" | "in-pulldown". */
  gpioSetup(pin: number, mode: string): void;
  /** Drive an output GPIO high (1) or low (0). */
  gpioWrite(pin: number, level: number): void;
  /** Read a GPIO input level (0 or 1). */
  gpioRead(pin: number): number;
};

/**
 * Operating-system (shell) capabilities. The launcher reads the installed app
 * list by index; `launchIndex` boots an app (tearing down the shell) and
 * `goBack` returns from an app to the shell. The registry holds built-in Apps
 * first, then external Apps discovered on the SD card, and is capped at the
 * shell's eight slots. A duplicate manifest id never replaces a built-in App.
 * `setTitle` updates the OS app-bar label, and `delay` is a polling heartbeat.
 */
declare const os: {
  /** Display name of the installed app at index `i`, or "" when none. */
  appName(i: number): string;
  /** Icon of the installed app at index `i`: a shared OS icon name or a
   * legacy single glyph; "" when no app is installed there. */
  appIcon(i: number): string;
  /** Boot the installed app at index `i` (the shell is replaced). */
  launchIndex(i: number): void;
  /** Exit the current app and return to the shell. */
  goBack(): void;
  /**
   * Replace the OS app-bar title for the running app (for example
   * `momo · 小墨` to show the active profile). Chrome only — it does not
   * touch app state — so a launch binding may call it to restore the title
   * on the first frame.
   */
  setTitle(title: string): void;
  /** Calls back with "" after `ms` milliseconds. */
  delay(ms: number, onResult: (result: string) => void): void;
};

/**
 * Wall clock. Milliseconds since the Unix epoch. The value is sampled when the
 * binding/handler runs; it is not a reactive source by itself. The ESP32 host
 * uses SNTP once Wi-Fi is connected and falls back to uptime before the first
 * successful sync.
 */
declare const clock: {
  now(): number;
};

/**
 * Per-App persistent key-value storage for App state. The host namespaces every
 * key by the app id, so two Apps can never collide. Reads are synchronous; a
 * missing key returns 0 / "" respectively. Values survive relaunches on Web
 * (per-key localStorage) and native (`~/.micros/kv`); the ESP32 host stores
 * them in the App's NVS namespace, so they survive a reboot too.
 */
declare const kv: {
  getNumber(key: string): number;
  getText(key: string): string;
  setNumber(key: string, value: number): void;
  setText(key: string, value: string): void;
  remove(key: string): void;
};

/**
 * Per-App configuration, provided by the OS as one flat JSON document. Unlike
 * `kv` (App state), the config document is meant to be human-editable and can
 * be seeded before the App launches. The same API maps to a native file, a Web
 * storage object, or an ESP32 partition block, so App code never touches paths:
 *
 * | Host  | Backing location |
 * | ----- | ---------------- |
 * | macOS | `~/.micros/<app-id>/config.json` |
 * | Web   | `localStorage["micro:config:<app-id>"]` |
 * | ESP32 | NVS `micro_config` on the `micro_cfg` partition |
 *
 * Reads are synchronous; a missing key returns 0 / "". Writes are handlers-only
 * (the compiler rejects them inside bindings). A value written here is the
 * effective value, so an App input can override a seeded config file entry.
 * Keys that a pre-v22 App kept in `kv` are still readable as a fallback; the
 * first `config` write promotes the value into the document and removes the
 * old `kv` entry. On macOS a pre-`~/.micros` tree is migrated from `~/.micrapp`
 * on first launch.
 */
declare const config: {
  getNumber(key: string): number;
  getText(key: string): string;
  setNumber(key: string, value: number): void;
  setText(key: string, value: string): void;
  remove(key: string): void;
};

/**
 * App-private external storage: the removable TF/SD card on the ESP32-S3
 * board, a sandboxed directory on macOS, and a simulated localStorage volume in
 * the Web Player.
 *
 * Every path is relative to the App's own directory
 * (`/sdcard/micros/apps/<app-id>/` on the device); there is no way to name the
 * card root, the OS, or another App's data. The host rejects absolute paths,
 * `.`/`..`, backslashes, drive/stream separators, control characters, symlinks
 * and over-long components before touching a filesystem. Names are ASCII
 * (`A-Z a-z 0-9 _ - . space`), at most 63 bytes per component, 8 levels and 240
 * bytes total.
 *
 * Files are UTF-8 text and may not exceed 64 KiB. `list()` returns at most 128
 * immediate children, one per line and sorted by name; directories end with
 * `/`. `size()` returns bytes, 0 for a directory and -1 when missing. Reads are
 * safe in bindings; `writeText`, `appendText`, `remove` and `mkdir` are
 * handlers-only.
 *
 * `available()` is false when no card is inserted or the mount failed; the
 * other calls then return their empty result ("" / false / -1) rather than
 * throwing. Writes create missing parent directories. `remove` deletes a file
 * or a directory tree.
 */
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

/**
 * Read-only JSON query over text such as an HTTP response. Paths are
 * dot-separated (`today_items.0.voc_spelling`); a leading `HTTP <code>\n`
 * status line is ignored. Missing paths return "" / 0.
 */
declare const json: {
  getText(text: string, path: string): string;
  getNumber(text: string, path: string): number;
  length(text: string, path: string): number;
};

/**
 * Audio output.
 *
 * `speak` pronounces an English word/phrase with the host's built-in speech
 * synthesizer (Web Speech API on the Web player, `AVSpeechSynthesizer` on
 * macOS). A new `speak` replaces the utterance in flight, so tapping
 * pronunciation repeatedly does not queue stale audio. `stop` only stops
 * speech; `setRate` adjusts the multiplier used by later `speak` calls: 0.5 is
 * slow, 1.0 is normal and 2.0 is fast. Hosts clamp it to 0.5–2.0 and default
 * to 0.9.
 *
 * `playAudio(url)` starts a foreground URL stream and returns a positive
 * handle. Calls mix, so several sounds can play at once. `stopAudio(handle)`
 * stops one stream and `stopAllAudio()` stops every foreground stream.
 * Foreground streams stop automatically when the App leaves the foreground.
 *
 * `playBackground(url)` starts or replaces the single system-wide background
 * music stream. It loops and keeps playing after the App returns to the shell.
 * `stopBackground()` stops it.
 *
 * URL audio accepts only http(s). An invalid or empty URL makes `playAudio`
 * return 0 and `playBackground` a no-op. Browser autoplay policy still applies
 * to the first sound. The ESP32 host has no codec yet, so `playAudio` returns 0
 * and the background calls are no-ops.
 */
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

/** Network (Wi-Fi) capabilities. Dynamic values arrive via async callbacks. */
declare const net: {
  /** "off" | "connecting" | "connected" | "error" */
  wifiState(): string;
  /** Connected SSID, or "" when not connected. */
  wifiSsid(): string;
  wifiConnect(ssid: string, password: string): void;
  wifiDisconnect(): void;
  /** Calls back with the SSID list, one per line. */
  scanWifi(onResult: (list: string) => void): void;
  /** SSID of the AP at scan index `i` from the last scan ("" when none). */
  wifiApName(i: number): string;
  /** Calls back with the HTTP response body (or an error line). */
  httpGet(url: string, onResult: (response: string) => void): void;
  /**
   * HTTP request with an explicit method (GET/POST/PUT/DELETE/HEAD…) and body.
   * macOS and ESP32 run it on the native HTTP transport; the Web host replies
   * with a canned response. Calls back with "HTTP <status>\n<body>" (or an
   * error).
   */
  httpRequest(
    method: string,
    url: string,
    body: string,
    onResult: (response: string) => void,
  ): void;
  /**
   * HTTP request with newline-delimited `Name: value` headers, e.g.
   * `"Authorization: Bearer ...\nAccept: application/json"`.
   * macOS and ESP32 perform the request with real HTTPS and headers; Web
   * returns a descriptive 501 because browser `fetch` cannot bypass CORS.
   */
  httpRequestWithHeaders(
    method: string,
    url: string,
    headers: string,
    body: string,
    onResult: (response: string) => void,
  ): void;
  /**
   * Look up newline-separated English words in the host dictionary. The
   * callback receives `HTTP <status>\n` plus a JSON document with
   * `entries: { word, phonetic, definition, example, cloze, translation }[]`.
   * macOS and ESP32 query Youdao's public JSON dictionary over HTTPS; Web
   * returns 501 because it has no native dictionary bridge.
   */
  lookupWords(words: string, onResult: (response: string) => void): void;
};

declare const ui: {
  column(children: UiNode[]): UiNode;
  row(children: UiNode[]): UiNode;
  /**
   * Decorative filled rectangle used for cards, pills, rules and page
   * canvases. It takes no children; place it before the content it sits
   * behind. Tones: "canvas", "paper", "sage", "terracotta", "line" (default
   * "paper"). With `onClick` the blank surface becomes a tap target; `label`
   * is its accessible name (a literal string), so a tile can be activated by
   * assistive tech even though the visible title is a separate `text` node.
   */
  surface(
    tone?: "canvas" | "paper" | "sage" | "terracotta" | "line",
    options?: { onClick?: () => void; label?: string },
  ): UiNode;
  /**
   * A shared OS icon from the `micro_ir::icons` catalog (Settings, files,
   * media, status…), painted as a symbol glyph on LVGL and inline SVG on Web.
   * `textStyle.size` picks the glyph size (12/14/18/24/32); `tone` tints it;
   * `variant: "badge"` paints a circular field behind it (App launcher).
   * A single display glyph is also accepted for legacy App letter marks.
   */
  icon(
    name: string | Binding<string>,
    options?: {
      onClick?: () => void;
      textStyle?: UiTextStyle;
      tone?: "ink" | "muted" | "accent" | "inverse";
      variant?: "plain" | "badge";
    },
  ): UiNode;
  /**
   * Conditionally show a widget. Hidden nodes keep their layout slot but do
   * not paint or receive clicks. Works for every widget kind; hiding a
   * container hides its children, so a read-only view and an edit form can
   * share one layout position.
   */
  show(widget: UiNode, visible: boolean | Binding<boolean>): UiNode;
  text(value: string | Binding<string>, style?: UiTextStyle): UiNode;
  /**
   * A labelled button. `variant: "selected"` paints the active/selected row
   * with the accent colour; it is static, so use two buttons plus `show` when
   * the selection changes at runtime.
   */
  button(
    label: string,
    options: {
      onClick: () => void;
      textStyle?: UiTextStyle;
      variant?: "default" | "selected";
    },
  ): UiNode;
  progress(value: number | Binding<number>): UiNode;
  switch(value: Binding<boolean>, options?: { onToggle?: () => void }): UiNode;
  input(
    value: string | Binding<string>,
    options?: {
      placeholder?: string;
      textStyle?: UiTextStyle;
      onChange?: (text: string) => void;
    },
  ): UiNode;
  slider(
    value: number | Binding<number>,
    options?: { min?: number; max?: number; onChange?: (value: number) => void },
  ): UiNode;
  checkbox(
    label: string,
    checked: boolean | Binding<boolean>,
    options?: { onChange?: (value: boolean) => void },
  ): UiNode;
  dropdown(
    options: string[],
    index: number | Binding<number>,
    opts?: { onChange?: (index: number) => void },
  ): UiNode;
  roller(
    options: string[],
    index: number | Binding<number>,
    opts?: { onChange?: (index: number) => void },
  ): UiNode;
  led(on: boolean | Binding<boolean>): UiNode;
  spinner(active: boolean | Binding<boolean>): UiNode;
  scale(value: number | Binding<number>, options?: { min?: number; max?: number }): UiNode;
  list(items: { text: string; onClick: () => void }[]): UiNode;
  tabview(tabs: { title: string; content: UiNode }[]): UiNode;
  place(
    widget: UiNode,
    layout: {
      /** Base left position (used when the left edge is not anchored). */
      left?: number;
      /** Base top position (used when the top edge is not anchored). */
      top?: number;
      /** Base width (used when the horizontal axis is not stretched by anchors). */
      width?: number;
      /** Base height (used when the vertical axis is not stretched by anchors). */
      height?: number;
      /** Edge anchors: an offset pins that edge to the parent's edge, taking
       * priority over the base position; both opposite edges set stretches. */
      anchor?: {
        left?: number;
        top?: number;
        right?: number;
        bottom?: number;
      };
      /**
       * Dock the widget to an edge (Delphi Align) — seeds the default anchor
       * combo; explicit `anchor` edges override per edge.
       * - "top": `{ left: 0, right: 0, top: 0 }`
       * - "bottom": `{ left: 0, right: 0, bottom: 0 }`
       * - "left": `{ left: 0, top: 0, bottom: 0 }`
       * - "right": `{ right: 0, top: 0, bottom: 0 }`
       * - "client": `{ left: 0, top: 0, right: 0, bottom: 0 }`
       * - "none": no anchors (base ltwh positioning)
       */
      align?: "top" | "bottom" | "left" | "right" | "client" | "none";
    },
  ): UiNode;
  mount(root: UiNode): void;
};
