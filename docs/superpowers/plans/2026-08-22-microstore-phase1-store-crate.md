# Micro Store Phase 1: Store Contract Crate + PC Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land a new `micro-store` crate defining the two-store contract (AppStore + KvStore) from the Micro Store design spec, plus a PC filesystem backend with atomic writes and a full test suite.

**Architecture:** `micro-store` is a dependency-light leaf crate: the contract types (`AppMeta`, `StoreError`, `KvValue`) and two traits (`AppStore`, `KvStore`/`ScopedKv`) in `lib.rs`, one concrete PC backend `NativeStore` in `native.rs`. The backend is rooted at a caller-supplied directory (`apps/` for blobs + `manifest.json`, `kv/<namespace>.json` for data), writes atomically via temp-file-then-rename, and rejects path-unsafe identifiers. Later phases build the SDK KV API and hosts on top of this contract.

**Tech Stack:** Rust (edition 2024, workspace), `serde`/`serde_json`, `tempfile` (dev).

---

## File structure

- Create: `crates/micro-store/Cargo.toml` — crate manifest, depends on serde/serde_json, dev-dep on tempfile
- Create: `crates/micro-store/src/lib.rs` — `AppMeta`, `StoreError`, `KvValue`, `AppStore`, `KvStore`, `ScopedKv`
- Create: `crates/micro-store/src/native.rs` — `NativeStore` + `NativeScopedKv` PC backend
- Create: `crates/micro-store/tests/contract.rs` — contract type tests
- Create: `crates/micro-store/tests/native.rs` — backend test suite
- Modify: `Cargo.toml` — add `crates/micro-store` to workspace members and `micro-store` to workspace dependencies

`KvValue` is the four-variant scalar union `Number | String | Bool | Null`. It lives in `micro-store` (not `micro-vm`) so the store crate has no core dependency; Phase 2 converts between `KvValue` and the VM `Value` at the VM boundary.

---

### Task 1: Scaffold the crate in the workspace

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/micro-store/Cargo.toml`
- Create: `crates/micro-store/src/lib.rs`
- Create: `crates/micro-store/src/native.rs`

- [ ] **Step 1: Register the crate in the workspace**

Edit the workspace `Cargo.toml` members list:

```toml
members = [
  "crates/micro-os-core",
  "crates/micro-board-profile",
  "crates/micro-ir",
  "crates/micro-vm",
  "crates/micro-core",
  "crates/micro-compiler",
  "crates/micro-lvgl",
  "crates/micro-host-sdl",
  "crates/micro-renderer-web",
  "crates/micro-host-web",
  "crates/micro-host-esp32",
  "crates/micro-store",
]
```

And add to `[workspace.dependencies]`:

```toml
micro-store = { path = "crates/micro-store" }
```

- [ ] **Step 2: Create the crate manifest**

`crates/micro-store/Cargo.toml`:

```toml
[package]
name = "micro-store"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Create empty module stubs**

`crates/micro-store/src/lib.rs`:

```rust
pub mod native;
```

`crates/micro-store/src/native.rs`:

```rust
```

- [ ] **Step 4: Verify the workspace builds**

Run: `cargo build -p micro-store`
Expected: builds successfully with no warnings.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/micro-store
git commit -m "chore: scaffold micro-store crate"
```

---

### Task 2: Define the contract types and traits

**Files:**
- Modify: `crates/micro-store/src/lib.rs`
- Create: `crates/micro-store/tests/contract.rs`

- [ ] **Step 1: Write the failing contract test**

`crates/micro-store/tests/contract.rs`:

```rust
use micro_store::{AppMeta, KvValue, StoreError};

#[test]
fn app_meta_round_trips_through_json() {
    let meta = AppMeta {
        id: "counter".to_owned(),
        name: "Counter".to_owned(),
        version: 3,
    };
    let encoded = serde_json::to_vec(&meta).expect("serialize");
    let decoded: AppMeta = serde_json::from_slice(&encoded).expect("deserialize");
    assert_eq!(decoded, meta);
}

#[test]
fn store_error_is_a_std_error() {
    let error: Box<dyn std::error::Error> = Box::new(StoreError::NotFound);
    assert!(!error.to_string().is_empty());
}

#[test]
fn kv_value_round_trips_all_scalar_types() {
    let cases = [
        (KvValue::Number(42.0), serde_json::json!(42.0)),
        (KvValue::String("hi".to_owned()), serde_json::json!("hi")),
        (KvValue::Bool(true), serde_json::json!(true)),
        (KvValue::Null, serde_json::Value::Null),
    ];
    for (value, json) in cases {
        assert_eq!(value.to_json(), json, "to_json for {value:?}");
        assert_eq!(KvValue::from_json(json).expect("from_json"), value);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p micro-store --test contract`
Expected: FAIL — `cannot find type 'AppMeta' in crate 'micro_store'`.

- [ ] **Step 3: Write the contract in lib.rs**

Replace `crates/micro-store/src/lib.rs` entirely:

```rust
pub mod native;

use std::fmt;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AppMeta {
    pub id: String,
    pub name: String,
    pub version: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StoreError {
    NotFound,
    Io(String),
    Corrupt(String),
    Full,
    Unsupported(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "not found"),
            Self::Io(message) => write!(f, "io error: {message}"),
            Self::Corrupt(message) => write!(f, "corrupt store: {message}"),
            Self::Full => write!(f, "store full"),
            Self::Unsupported(message) => write!(f, "unsupported: {message}"),
        }
    }
}

impl std::error::Error for StoreError {}

#[derive(Debug, Clone, PartialEq)]
pub enum KvValue {
    Number(f64),
    String(String),
    Bool(bool),
    Null,
}

impl KvValue {
    /// Serialize to a plain JSON value (number, string, bool, or null).
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Number(value) => serde_json::json!(value),
            Self::String(value) => serde_json::json!(value),
            Self::Bool(value) => serde_json::json!(value),
            Self::Null => serde_json::Value::Null,
        }
    }

    /// Deserialize from a plain JSON value; returns `None` for arrays/objects.
    pub fn from_json(value: serde_json::Value) -> Option<Self> {
        match value {
            serde_json::Value::Number(value) => value.as_f64().map(Self::Number),
            serde_json::Value::String(value) => Some(Self::String(value)),
            serde_json::Value::Bool(value) => Some(Self::Bool(value)),
            serde_json::Value::Null => Some(Self::Null),
            _ => None,
        }
    }
}

/// The App blob store: installed Apps plus their index.
pub trait AppStore {
    fn list(&self) -> Result<Vec<AppMeta>, StoreError>;
    fn read(&self, id: &str) -> Result<Vec<u8>, StoreError>;
    fn install(&mut self, meta: AppMeta, bytes: &[u8]) -> Result<(), StoreError>;
    fn uninstall(&mut self, id: &str) -> Result<(), StoreError>;
}

/// A key-value handle bound to one App namespace.
pub trait ScopedKv {
    fn get(&self, key: &str) -> Result<Option<KvValue>, StoreError>;
    fn set(&mut self, key: &str, value: &KvValue) -> Result<(), StoreError>;
    fn remove(&mut self, key: &str) -> Result<(), StoreError>;
}

/// The KV store: opens per-App namespaces.
pub trait KvStore {
    fn open(&self, namespace: &str) -> Result<Box<dyn ScopedKv>, StoreError>;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p micro-store --test contract`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/micro-store
git commit -m "feat: define micro-store contract types and traits"
```

---

### Task 3: PC AppStore backend (install / list / read / uninstall)

**Files:**
- Modify: `crates/micro-store/src/native.rs`
- Create: `crates/micro-store/tests/native.rs`

Note: identifier validation is deliberately NOT added in this task; it lands test-first in Task 5. This task's tests use valid identifiers only.

- [ ] **Step 1: Write the failing AppStore tests**

`crates/micro-store/tests/native.rs`:

```rust
use micro_store::{AppMeta, AppStore, KvStore, KvValue, NativeStore, StoreError};
use std::fs;
use tempfile::TempDir;

fn fresh_store() -> (TempDir, NativeStore) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = NativeStore::new(dir.path());
    (dir, store)
}

fn counter_meta() -> AppMeta {
    AppMeta {
        id: "counter".to_owned(),
        name: "Counter".to_owned(),
        version: 1,
    }
}

#[test]
fn install_list_read_round_trip() {
    let (_dir, mut store) = fresh_store();
    store
        .install(counter_meta(), b"MBC1-app-bytes")
        .expect("install");

    let listed = store.list().expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "counter");
    assert_eq!(listed[0].name, "Counter");
    assert_eq!(listed[0].version, 1);

    assert_eq!(store.read("counter").expect("read"), b"MBC1-app-bytes");
}

#[test]
fn install_overwrites_existing_id() {
    let (_dir, mut store) = fresh_store();
    store.install(counter_meta(), b"one").expect("first");
    store.install(counter_meta(), b"two").expect("second");

    assert_eq!(store.list().expect("list").len(), 1);
    assert_eq!(store.read("counter").expect("read"), b"two");
}

#[test]
fn read_missing_app_is_not_found() {
    let (_dir, store) = fresh_store();
    assert_eq!(store.read("counter"), Err(StoreError::NotFound));
}

#[test]
fn uninstall_removes_blob_and_manifest_entry() {
    let (_dir, mut store) = fresh_store();
    store.install(counter_meta(), b"MBC1-app-bytes").expect("install");
    store.uninstall("counter").expect("uninstall");

    assert!(store.list().expect("list").is_empty());
    assert_eq!(store.read("counter"), Err(StoreError::NotFound));
}

#[test]
fn uninstall_missing_app_is_not_found() {
    let (_dir, mut store) = fresh_store();
    assert_eq!(store.uninstall("ghost"), Err(StoreError::NotFound));
}

#[test]
fn app_store_persists_across_instances() {
    let dir = tempfile::tempdir().expect("tempdir");
    {
        let mut store = NativeStore::new(dir.path());
        store.install(counter_meta(), b"MBC1-app-bytes").expect("install");
    }
    let store = NativeStore::new(dir.path());
    assert_eq!(store.list().expect("list").len(), 1);
    assert_eq!(store.read("counter").expect("read"), b"MBC1-app-bytes");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p micro-store --test native`
Expected: FAIL — compile error: `NativeStore` does not exist.

- [ ] **Step 3: Implement the AppStore backend**

Replace `crates/micro-store/src/lib.rs` with (re-exports `NativeStore` at the crate root so tests and hosts use `micro_store::NativeStore`):

```rust
pub mod native;

pub use native::NativeStore;
```

Replace `crates/micro-store/src/native.rs` entirely:

```rust
use std::fs;
use std::path::{Path, PathBuf};

use crate::{AppMeta, AppStore, StoreError};

const APP_DIR: &str = "apps";
const KV_DIR: &str = "kv";
const MANIFEST_FILE: &str = "manifest.json";

#[derive(Debug, Clone)]
pub struct NativeStore {
    root: PathBuf,
}

impl NativeStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn apps_dir(&self) -> PathBuf {
        self.root.join(APP_DIR)
    }

    fn kv_dir(&self) -> PathBuf {
        self.root.join(KV_DIR)
    }

    fn manifest_path(&self) -> PathBuf {
        self.apps_dir().join(MANIFEST_FILE)
    }

    fn load_manifest(&self) -> Result<Vec<AppMeta>, StoreError> {
        match fs::read(self.manifest_path()) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| StoreError::Corrupt(format!("manifest.json: {error}"))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(StoreError::Io(error.to_string())),
        }
    }

    fn save_manifest(&self, manifest: &[AppMeta]) -> Result<(), StoreError> {
        let bytes = serde_json::to_vec_pretty(manifest)
            .map_err(|error| StoreError::Io(error.to_string()))?;
        atomic_write(&self.manifest_path(), &bytes)
    }
}

/// Write `bytes` to `path` atomically: create the parent, write a sibling
/// `.tmp`, then rename over the destination.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| StoreError::Io("path has no parent".into()))?;
    fs::create_dir_all(parent).map_err(|error| StoreError::Io(error.to_string()))?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|error| StoreError::Io(error.to_string()))?;
    fs::rename(&tmp, path).map_err(|error| StoreError::Io(error.to_string()))
}

impl AppStore for NativeStore {
    fn list(&self) -> Result<Vec<AppMeta>, StoreError> {
        self.load_manifest()
    }

    fn read(&self, id: &str) -> Result<Vec<u8>, StoreError> {
        let path = self.apps_dir().join(format!("{id}.mbc"));
        fs::read(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                StoreError::NotFound
            } else {
                StoreError::Io(error.to_string())
            }
        })
    }

    fn install(&mut self, meta: AppMeta, bytes: &[u8]) -> Result<(), StoreError> {
        let blob_path = self.apps_dir().join(format!("{}.mbc", meta.id));
        atomic_write(&blob_path, bytes)?;
        let mut manifest = self.load_manifest()?;
        manifest.retain(|entry| entry.id != meta.id);
        manifest.push(meta);
        self.save_manifest(&manifest)
    }

    fn uninstall(&mut self, id: &str) -> Result<(), StoreError> {
        let blob_path = self.apps_dir().join(format!("{id}.mbc"));
        let blob_removed = match fs::remove_file(&blob_path) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(StoreError::Io(error.to_string())),
        };
        let mut manifest = self.load_manifest()?;
        let had_entry = manifest.iter().any(|entry| entry.id == id);
        manifest.retain(|entry| entry.id != id);
        if !blob_removed && !had_entry {
            return Err(StoreError::NotFound);
        }
        self.save_manifest(&manifest)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p micro-store --test native`
Expected: PASS (6 tests). `kv_dir` and `KV_DIR` are unused until Task 4 and may produce a dead-code warning; acceptable.

- [ ] **Step 5: Commit**

```bash
git add crates/micro-store
git commit -m "feat: implement PC AppStore backend with atomic writes"
```

---

### Task 4: PC KvStore backend (open / get / set / remove)

**Files:**
- Modify: `crates/micro-store/src/native.rs`
- Modify: `crates/micro-store/tests/native.rs`

Note: the non-finite number guard is deliberately NOT added in this task; it lands test-first in Task 5.

- [ ] **Step 1: Write the failing KvStore tests**

Append to `crates/micro-store/tests/native.rs`:

```rust
#[test]
fn kv_round_trips_all_scalar_types() {
    let (_dir, store) = fresh_store();
    let mut kv = store.open("counter").expect("open");
    kv.set("count", &KvValue::Number(42.0)).expect("set count");
    kv.set("name", &KvValue::String("hi".to_owned()))
        .expect("set name");
    kv.set("ok", &KvValue::Bool(true)).expect("set ok");
    kv.set("nil", &KvValue::Null).expect("set nil");

    assert_eq!(kv.get("count"), Ok(Some(KvValue::Number(42.0))));
    assert_eq!(kv.get("name"), Ok(Some(KvValue::String("hi".to_owned()))));
    assert_eq!(kv.get("ok"), Ok(Some(KvValue::Bool(true))));
    assert_eq!(kv.get("nil"), Ok(Some(KvValue::Null)));
}

#[test]
fn kv_missing_key_is_none() {
    let (_dir, store) = fresh_store();
    let kv = store.open("counter").expect("open");
    assert_eq!(kv.get("nope"), Ok(None));
}

#[test]
fn kv_remove_deletes_the_key() {
    let (_dir, store) = fresh_store();
    let mut kv = store.open("counter").expect("open");
    kv.set("count", &KvValue::Number(1.0)).expect("set");
    kv.remove("count").expect("remove");
    assert_eq!(kv.get("count"), Ok(None));
}

#[test]
fn kv_namespaces_are_isolated() {
    let (_dir, store) = fresh_store();
    let mut alpha = store.open("alpha").expect("open alpha");
    let beta = store.open("beta").expect("open beta");
    alpha
        .set("shared", &KvValue::Number(1.0))
        .expect("set alpha");
    assert_eq!(beta.get("shared"), Ok(None));
}

#[test]
fn kv_persists_across_open() {
    let dir = tempfile::tempdir().expect("tempdir");
    {
        let store = NativeStore::new(dir.path());
        let mut kv = store.open("counter").expect("open");
        kv.set("count", &KvValue::Number(42.0)).expect("set");
    }
    let store = NativeStore::new(dir.path());
    let kv = store.open("counter").expect("open");
    assert_eq!(kv.get("count"), Ok(Some(KvValue::Number(42.0))));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p micro-store --test native`
Expected: FAIL — compile error: `KvStore` not implemented for `NativeStore`.

- [ ] **Step 3: Implement the KvStore backend**

Replace `crates/micro-store/src/native.rs` entirely (adds `NativeScopedKv` and the `KvStore` impl, removes the dead-code warning):

```rust
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{AppMeta, AppStore, KvStore, KvValue, ScopedKv, StoreError};

const APP_DIR: &str = "apps";
const KV_DIR: &str = "kv";
const MANIFEST_FILE: &str = "manifest.json";

#[derive(Debug, Clone)]
pub struct NativeStore {
    root: PathBuf,
}

impl NativeStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn apps_dir(&self) -> PathBuf {
        self.root.join(APP_DIR)
    }

    fn kv_dir(&self) -> PathBuf {
        self.root.join(KV_DIR)
    }

    fn manifest_path(&self) -> PathBuf {
        self.apps_dir().join(MANIFEST_FILE)
    }

    fn load_manifest(&self) -> Result<Vec<AppMeta>, StoreError> {
        match fs::read(self.manifest_path()) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| StoreError::Corrupt(format!("manifest.json: {error}"))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(StoreError::Io(error.to_string())),
        }
    }

    fn save_manifest(&self, manifest: &[AppMeta]) -> Result<(), StoreError> {
        let bytes = serde_json::to_vec_pretty(manifest)
            .map_err(|error| StoreError::Io(error.to_string()))?;
        atomic_write(&self.manifest_path(), &bytes)
    }
}

/// Write `bytes` to `path` atomically: create the parent, write a sibling
/// `.tmp`, then rename over the destination.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| StoreError::Io("path has no parent".into()))?;
    fs::create_dir_all(parent).map_err(|error| StoreError::Io(error.to_string()))?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|error| StoreError::Io(error.to_string()))?;
    fs::rename(&tmp, path).map_err(|error| StoreError::Io(error.to_string()))
}

impl AppStore for NativeStore {
    fn list(&self) -> Result<Vec<AppMeta>, StoreError> {
        self.load_manifest()
    }

    fn read(&self, id: &str) -> Result<Vec<u8>, StoreError> {
        let path = self.apps_dir().join(format!("{id}.mbc"));
        fs::read(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                StoreError::NotFound
            } else {
                StoreError::Io(error.to_string())
            }
        })
    }

    fn install(&mut self, meta: AppMeta, bytes: &[u8]) -> Result<(), StoreError> {
        let blob_path = self.apps_dir().join(format!("{}.mbc", meta.id));
        atomic_write(&blob_path, bytes)?;
        let mut manifest = self.load_manifest()?;
        manifest.retain(|entry| entry.id != meta.id);
        manifest.push(meta);
        self.save_manifest(&manifest)
    }

    fn uninstall(&mut self, id: &str) -> Result<(), StoreError> {
        let blob_path = self.apps_dir().join(format!("{id}.mbc"));
        let blob_removed = match fs::remove_file(&blob_path) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(StoreError::Io(error.to_string())),
        };
        let mut manifest = self.load_manifest()?;
        let had_entry = manifest.iter().any(|entry| entry.id == id);
        manifest.retain(|entry| entry.id != id);
        if !blob_removed && !had_entry {
            return Err(StoreError::NotFound);
        }
        self.save_manifest(&manifest)
    }
}

impl KvStore for NativeStore {
    fn open(&self, namespace: &str) -> Result<Box<dyn ScopedKv>, StoreError> {
        Ok(Box::new(NativeScopedKv {
            root: self.root.clone(),
            namespace: namespace.to_owned(),
        }))
    }
}

struct NativeScopedKv {
    root: PathBuf,
    namespace: String,
}

impl NativeScopedKv {
    fn kv_path(&self) -> PathBuf {
        self.root.join(KV_DIR).join(format!("{}.json", self.namespace))
    }

    fn load(&self) -> Result<BTreeMap<String, KvValue>, StoreError> {
        match fs::read(self.kv_path()) {
            Ok(bytes) => {
                let raw: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(&bytes)
                    .map_err(|error| {
                        StoreError::Corrupt(format!("{}.json: {error}", self.namespace))
                    })?;
                let mut out = BTreeMap::new();
                for (key, value) in raw {
                    out.insert(
                        key,
                        KvValue::from_json(value).ok_or_else(|| {
                            StoreError::Corrupt(format!(
                                "{}.json: unsupported value",
                                self.namespace
                            ))
                        })?,
                    );
                }
                Ok(out)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(error) => Err(StoreError::Io(error.to_string())),
        }
    }

    fn save(&self, map: &BTreeMap<String, KvValue>) -> Result<(), StoreError> {
        let json: serde_json::Map<String, serde_json::Value> = map
            .iter()
            .map(|(key, value)| (key.clone(), value.to_json()))
            .collect();
        let bytes = serde_json::to_vec_pretty(&json)
            .map_err(|error| StoreError::Io(error.to_string()))?;
        atomic_write(&self.kv_path(), &bytes)
    }
}

impl ScopedKv for NativeScopedKv {
    fn get(&self, key: &str) -> Result<Option<KvValue>, StoreError> {
        Ok(self.load()?.get(key).cloned())
    }

    fn set(&mut self, key: &str, value: &KvValue) -> Result<(), StoreError> {
        let mut map = self.load()?;
        map.insert(key.to_owned(), value.clone());
        self.save(&map)
    }

    fn remove(&mut self, key: &str) -> Result<(), StoreError> {
        let mut map = self.load()?;
        map.remove(key);
        self.save(&map)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p micro-store --test native`
Expected: PASS (11 tests total).

- [ ] **Step 5: Commit**

```bash
git add crates/micro-store
git commit -m "feat: implement PC KvStore backend with per-namespace files"
```

---

### Task 5: Security and corruption guards (test-first)

**Files:**
- Modify: `crates/micro-store/src/native.rs`
- Modify: `crates/micro-store/tests/native.rs`

This task adds the identifier-validation and non-finite-number guards that Tasks 3 and 4 deliberately omitted. The corrupt-file tests below are characterization tests (the guards already exist in `load_manifest` and `NativeScopedKv::load`) and will pass immediately; the identifier and non-finite tests are the red-green ones.

- [ ] **Step 1: Write the failing edge-case tests**

Append to `crates/micro-store/tests/native.rs`:

```rust
#[test]
fn invalid_identifiers_are_rejected() {
    let (_dir, mut store) = fresh_store();

    let evil = AppMeta {
        id: "../evil".to_owned(),
        name: "Evil".to_owned(),
        version: 1,
    };
    assert!(matches!(
        store.install(evil, b"bytes"),
        Err(StoreError::Unsupported(_))
    ));
    assert!(matches!(
        store.read("../evil"),
        Err(StoreError::Unsupported(_))
    ));
    assert!(matches!(
        store.open("a/b"),
        Err(StoreError::Unsupported(_))
    ));
}

#[test]
fn empty_identifier_is_rejected() {
    let (_dir, store) = fresh_store();
    assert!(matches!(
        store.open(""),
        Err(StoreError::Unsupported(_))
    ));
}

#[test]
fn non_finite_kv_number_is_rejected() {
    let (_dir, store) = fresh_store();
    let mut kv = store.open("ns").expect("open");
    assert!(matches!(
        kv.set("nan", &KvValue::Number(f64::NAN)),
        Err(StoreError::Unsupported(_))
    ));
}

#[test]
fn corrupt_manifest_is_reported_corrupt() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("apps")).expect("mkdir");
    fs::write(dir.path().join("apps/manifest.json"), b"not json").expect("write");
    let store = NativeStore::new(dir.path());
    assert!(matches!(store.list(), Err(StoreError::Corrupt(_))));
}

#[test]
fn corrupt_kv_file_is_reported_corrupt() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("kv")).expect("mkdir");
    fs::write(dir.path().join("kv/counter.json"), b"not json").expect("write");
    let store = NativeStore::new(dir.path());
    let kv = store.open("counter").expect("open");
    assert!(matches!(kv.get("k"), Err(StoreError::Corrupt(_))));
}
```

- [ ] **Step 2: Run test to verify the red-green split**

Run: `cargo test -p micro-store --test native`
Expected: `invalid_identifiers_are_rejected`, `empty_identifier_is_rejected`, and `non_finite_kv_number_is_rejected` FAIL (no guards yet — `open("a/b")` returns `Ok`, `set(NaN)` returns `Ok`). `corrupt_manifest_is_reported_corrupt` and `corrupt_kv_file_is_reported_corrupt` PASS (guards already present). 13 tests pass, 3 fail.

- [ ] **Step 3: Implement the guards**

Apply these edits to `crates/micro-store/src/native.rs`:

Add `validate_ident` as a method on `NativeStore` (inside `impl NativeStore`, after `manifest_path`):

```rust
    /// Rejects identifiers that could escape the store directory or that NVS
    /// could not host, matching the `[A-Za-z0-9][A-Za-z0-9_.-]*` shape.
    fn validate_ident(name: &str) -> Result<(), StoreError> {
        let mut chars = name.chars();
        let first_ok = chars.next().map_or(false, |c| c.is_ascii_alphanumeric());
        if !first_ok
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        {
            return Err(StoreError::Unsupported(format!(
                "invalid identifier `{name}`"
            )));
        }
        Ok(())
    }
```

Call it at the start of `AppStore::read`, `AppStore::install`, `AppStore::uninstall`, and `KvStore::open`:

```rust
    fn read(&self, id: &str) -> Result<Vec<u8>, StoreError> {
        Self::validate_ident(id)?;
        let path = self.apps_dir().join(format!("{id}.mbc"));
        // ... unchanged ...
    }

    fn install(&mut self, meta: AppMeta, bytes: &[u8]) -> Result<(), StoreError> {
        Self::validate_ident(&meta.id)?;
        let blob_path = self.apps_dir().join(format!("{}.mbc", meta.id));
        // ... unchanged ...
    }

    fn uninstall(&mut self, id: &str) -> Result<(), StoreError> {
        Self::validate_ident(id)?;
        let blob_path = self.apps_dir().join(format!("{id}.mbc"));
        // ... unchanged ...
    }
```

```rust
impl KvStore for NativeStore {
    fn open(&self, namespace: &str) -> Result<Box<dyn ScopedKv>, StoreError> {
        Self::validate_ident(namespace)?;
        Ok(Box::new(NativeScopedKv {
            root: self.root.clone(),
            namespace: namespace.to_owned(),
        }))
    }
}
```

Add the non-finite guard at the top of `ScopedKv::set`:

```rust
    fn set(&mut self, key: &str, value: &KvValue) -> Result<(), StoreError> {
        if let KvValue::Number(value) = value {
            if !value.is_finite() {
                return Err(StoreError::Unsupported(
                    "non-finite numbers cannot be stored".into(),
                ));
            }
        }
        let mut map = self.load()?;
        map.insert(key.to_owned(), value.clone());
        self.save(&map)
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p micro-store --test native`
Expected: PASS (16 tests total).

- [ ] **Step 5: Commit**

```bash
git add crates/micro-store
git commit -m "fix: reject path-unsafe identifiers and non-finite kv numbers"
```

---

### Task 6: Full workspace verification

**Files:** none

- [ ] **Step 1: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: PASS — all existing workspace tests plus the 19 `micro-store` tests (3 contract + 16 native).

- [ ] **Step 2: Run clippy and fmt**

Run: `cargo clippy -p micro-store --all-targets -- -D warnings`
Expected: no warnings.

Run: `cargo fmt -p micro-store --check`
Expected: formatting is clean. If not, run `cargo fmt -p micro-store` and re-run the check.

- [ ] **Step 3: Confirm the diff is minimal and committed**

Run: `git status --short`
Expected: clean working tree.

Run: `git log --oneline -7`
Expected: the six Phase 1 commits on top of the prior work.

---

## Self-review notes

- **Spec coverage:** Phase 1 maps to spec sections "Two-store contract", "Backends" (PC row), and "Verification" (store backends, corrupt bytes, uninstall). KV instructions / SDK API / launcher / HTTP install are explicitly Phase 2–4 and out of scope here.
- **`KvValue` placement:** the spec re-uses the VM scalar union; this plan defines an identical `KvValue` inside `micro-store` so the crate stays dependency-free. Phase 2 adds `From<KvValue> for micro_vm::Value` at the VM boundary.
- **Identifier validation is test-first (Task 5), not smuggled into Task 3/4**, so each guard is genuinely red-green. Until Task 5, the backend accepts any identifier; only the Phase 1 test suite (which uses valid ids) exercises it.
- **`&mut self` on install/uninstall** matches the spec trait signatures; the file-based backend is actually interior-mutation-free but keeps the contract's intended ownership.
