# Micro Store design

## Goal

Give Micro Apps one host-managed storage model shaped by the ESP32's real flash layout, simulated by the PC and Web hosts, that (a) holds installed Apps as MBC blobs, (b) exposes per-App key-value data through the SDK, and (c) lets the device install and uninstall Apps over HTTP from a Store screen. The model has no traditional file paths: the App and KV stores are the two abstractions, and AppId is the only identifier that crosses the core boundary.

## Two-store contract

A new `micro-store` crate defines the contract in the shape of the ESP32 flash. It has no UI, VM, or renderer dependencies.

```rust
struct AppMeta { id: String, name: String, version: u16 }

enum StoreError { NotFound, Exists, Io(String), Corrupt, Full, Unsupported }

trait AppStore {
    fn list(&self) -> Vec<AppMeta>;
    fn read(&self, id: &str) -> Result<Vec<u8>, StoreError>;      // launch path: returns MBC bytes
    fn install(&mut self, meta: AppMeta, bytes: &[u8]) -> Result<(), StoreError>;
    fn uninstall(&mut self, id: &str) -> Result<(), StoreError>;
}

trait KvStore {
    fn open(&self, namespace: &str) -> Result<ScopedKv, StoreError>; // per-App namespace
}

trait ScopedKv {
    fn get(&self, key: &str) -> Result<Option<Value>, StoreError>;
    fn set(&mut self, key: &str, value: &Value) -> Result<(), StoreError>;
    fn remove(&mut self, key: &str) -> Result<(), StoreError>;
}
```

`Value` is the existing `micro-vm` scalar union (`Number | String | Bool | Null`) re-used as the KV value type. Apps and the VM never see paths; the host maps namespaces and blob files to physical storage.

The ESP32-S3 partition table already reserves both stores: `micro_cfg` (NVS, 64 KiB) for KV and `micro_apps` (LittleFS, ~4.25 MiB) for Apps. Firmware currently declares them but does not mount or use them.

### Backends

| | AppStore | KvStore |
|---|---|---|
| ESP32 (real) | LittleFS on `micro_apps`: `<id>.mbc` files + `manifest.json` | NVS on `micro_cfg`: `nvs_open(namespace = app_id)` |
| PC (simulator) | `~/.micrapp/apps/` directory + `manifest.json` | `~/.micrapp/kv/<namespace>.json` |
| Web (simulator) | in-memory preloaded set | `localStorage` keys `micro:kv:<ns>:<key>` |

PC and Web are literal simulators of the ESP contract: behavior is fixed by the trait, physical layout is host-local. `manifest.json` stores `[{ id, name, version, file }]` and is the AppStore index on both ESP32 and PC.

## Runtime is a shared engine

The runtime engine (`micro-core::Runtime` + `micro-vm` + `micro-ir`) is compiled once into each host and runs any AppImage. It is not per-App. A per-launch `Runtime` instance owns one decoded MBC image plus fresh state slots, UI tree, and event queue. Switching Apps drops the current instance (unloading its MBC) and creates a new instance from the next App's bytes on the same renderer and window:

```
Launcher ─ read(id) → decode → Runtime::new(image, renderer, budget) ─▶ running
running ─ Back → drop instance → re-render Launcher
```

The host opens the KV namespace for the launched App (`kv.open(app_id)`) and injects the resulting `ScopedKv` into that instance alongside the renderer, so each App reads and writes only its own keys.

## MBC v3 and KV instructions

KV calls require three new stack-based instructions with no immediates: `KVGet`, `KVSet`, `KVRemove`. `KVSet` pops value then key; `KVGet` pops key and pushes the scalar result (or `Null`); `KVRemove` pops key and pushes a `Bool`. Because the codec is otherwise incompatible, MBC advances from v2 to v3 and the codec rejects v1, v2, and unknown versions explicitly.

Each KV instruction consumes a fixed budget so a flash-backed write cannot be looped as a free instruction: `KVGet` = 10, `KVSet` = 50, `KVRemove` = 20 instructions.

## SDK KV API

Apps see a synchronous, typed-scalar API consistent with the supported TypeScript subset:

```ts
declare const kv: {
  get(key: string): string | number | boolean | null;
  set(key: string, value: string | number | boolean | null): boolean;
  remove(key: string): boolean;
};
```

`kv.set("count", 42)` stores the number 42 and `kv.get("count")` returns the number 42, not a string. The host serializes each value with a one-byte type tag plus bytes so NVS, PC JSON, and `localStorage` behave identically. Keys are strings only; the compiler rejects non-string keys.

`kv.get` is allowed in handlers and bindings (a one-shot read; binding dependency tracking covers state reads only, so KV changes do not re-evaluate bindings). `kv.set` and `kv.remove` are allowed in handlers only; bindings are pure UI mappings and must not perform writes.

Storage failures never crash the runtime: `get` returns `null`, `set`/`remove` return `false`, and a host diagnostic is emitted through the existing `report_diagnostic` channel. A missing key and a stored `null` both read back as `null`, matching TypeScript semantics.

## Compiler and VM plumbing

The compiler allow-list in `parse.rs` gains `kv.get` / `kv.set` / `kv.remove` with their argument counts and string-key validation. `lower.rs` lowers them to the new instructions. The VM receives the injected `ScopedKv` on each `Vm::new` in `Runtime::tick` and `evaluate_binding`, and services KV instructions synchronously.

## Launcher

The Launcher is a host-rendered system screen, not a Micro UI App. Launching requires the privileged `AppStore::read` + runtime swap, which the app SDK cannot express, and the Web player already renders its launcher as host UI. A small `HostScreen` abstraction renders the App list, launches on click, and returns to the launcher on Back:

- ESP32 / native: LVGL objects drawn directly, outside the Micro UI tree.
- Web: reuse the existing DOM system shell, replacing the hard-coded app list with `AppStore::list()`.

System screens and the App screen render through separate channels and never share nodes.

## Store screen and HTTP install

The Store is a second host-rendered system screen showing installed Apps (each with an uninstall action) and an "add App by URL" flow. URL entry uses LVGL textarea + keyboard on ESP32/native and a DOM `<input>` on Web.

Install pipeline:

```
GET <url> → stream to temp → decode + validate → atomic rename → manifest update
     (native: ureq sync)      (CRC-32, magic,       (on success)
     (ESP32: esp_http_client)  structural checks)
```

- The URL points directly at an `.mbc` file; there is no catalog server in this phase.
- App identity derives from the URL basename (`counter.mbc` → id `counter`, name `Counter`); a duplicate id asks to confirm and overwrite.
- `micro-ir` validation is the install gate: a corrupt download never enters the store.
- Any failure deletes the temp file, leaves `manifest.json` untouched, and shows the error on the Store screen.
- Uninstall deletes `<id>.mbc` and its manifest entry.

Platform notes:

- ESP32 HTTP download requires a connected Wi-Fi network first; the firmware gains an `esp_wifi` connect path that feeds the existing `micro-os-core` Wi-Fi state machine. The event-driven download is host-side, so the app SDK remains synchronous.
- Web installs via `fetch` into the in-memory store and are session-only; the browser cannot persist a real store. This is a documented Web limitation, not a silent feature.

### Security note (accepted for this phase)

Plaintext HTTP means a man-in-the-middle can inject an MBC; CRC detects corruption but not tampering. Apps are already sandboxed by the VM instruction budget and the KV-only permission surface, and app content is treated as untrusted. Production should require HTTPS plus signed MBC verification; that is explicitly future work, not a blocker here.

## Verification

- Codec: MBC v3 round trips with all three KV instructions; v1/v2 and unknown versions rejected.
- Compiler: `kv.get/set/remove` accepted, wrong arity and non-string keys rejected with `MTS...` diagnostics.
- VM: KV instructions route through a fake `ScopedKv`; get pushes typed scalars, set/remove push success booleans; budget charging is exact.
- Runtime: a full App persists a value across two launches (native, hidden), and a KV failure surfaces as `null`/`false` plus a host diagnostic without crashing.
- Store backends: install/list/read/uninstall round trip on the PC backend; corrupt bytes rejected; uninstall removes blob and manifest entry.
- Native E2E: local HTTP server serves an `.mbc`; Store screen installs it, Launcher lists it, click launches, Back returns.
- Web: store screen lists the preloaded set; install is documented session-only.

## Implementation phases

Each phase lands independently testable work, native-first, hardware last:

1. `micro-store` crate with the two-store contract, PC backend, and unit tests.
2. SDK KV API: compiler allow-list, MBC v3 instructions, VM routing, runtime injection, native E2E persistence test.
3. Launcher as host-rendered screens on native and Web, listing from `AppStore::list()`.
4. Store screen and HTTP install on native first, then ESP32 LittleFS/NVS mounting, Wi-Fi connect, and `esp_http_client`.
