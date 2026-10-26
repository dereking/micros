#[cfg(all(not(target_os = "espidf"), not(target_arch = "wasm32")))]
pub mod native;

#[cfg(all(not(target_os = "espidf"), not(target_arch = "wasm32")))]
pub use native::{
    LEGACY_ROOT_NAME, Migration, NativeConfigStore, NativeFileStore, NativeStore, default_root,
    migrate_legacy_root, migrate_legacy_root_from,
};

pub mod files;
pub use files::{
    FileStore, MAX_FILE_BYTES, MAX_LIST_ENTRIES, MAX_PATH_BYTES, MAX_PATH_DEPTH, MAX_SEGMENT_BYTES,
    MemoryFileStore, ScopedFiles, validate_app_id, validate_path,
};

use std::collections::BTreeMap;
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

/// A per-App configuration document: one flat JSON object of scalar values.
///
/// This is the on-disk / on-wire format shared by every host backend:
///
/// * macOS: `~/.micros/<app>/config.json` (pretty-printed, user-editable);
/// * Web: `localStorage["micro:config:<app>"]` (a JSON object string);
/// * ESP32: one NVS string key on the `micro_cfg` partition.
///
/// Nested arrays/objects are not exposed by the App SDK; `parse` skips them so
/// a hand-edited file with extra metadata still loads. Keys are flat strings
/// and values are scalars, matching `KvValue`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigDoc {
    values: BTreeMap<String, KvValue>,
}

impl ConfigDoc {
    /// Parse a JSON object. Empty input is an empty document; malformed JSON
    /// is reported as `StoreError::Corrupt`.
    pub fn parse(raw: &str) -> Result<Self, StoreError> {
        if raw.trim().is_empty() {
            return Ok(Self::default());
        }
        let object: serde_json::Map<String, serde_json::Value> = serde_json::from_str(raw)
            .map_err(|error| StoreError::Corrupt(format!("config: {error}")))?;
        let mut values = BTreeMap::new();
        for (key, value) in object {
            if let Some(value) = KvValue::from_json(value) {
                values.insert(key, value);
            }
        }
        Ok(Self { values })
    }

    pub fn get(&self, key: &str) -> Option<KvValue> {
        self.values.get(key).cloned()
    }

    /// Rejects empty/NUL keys and non-finite numbers so all three backends
    /// accept exactly the same values.
    pub fn set(&mut self, key: &str, value: &KvValue) -> Result<(), StoreError> {
        if key.is_empty() || key.contains('\0') {
            return Err(StoreError::Unsupported(format!(
                "invalid config key `{key}`"
            )));
        }
        if let KvValue::Number(number) = value
            && !number.is_finite()
        {
            return Err(StoreError::Unsupported(
                "non-finite numbers cannot be stored".into(),
            ));
        }
        self.values.insert(key.to_owned(), value.clone());
        Ok(())
    }

    pub fn remove(&mut self, key: &str) {
        self.values.remove(key);
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Compact JSON, used by the NVS/localStorage backends where size matters.
    pub fn to_json(&self) -> String {
        self.to_value().to_string()
    }

    /// Indented JSON, used by the native config file so users can edit it.
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(&self.to_value()).unwrap_or_else(|_| "{}".to_owned())
    }

    fn to_value(&self) -> serde_json::Value {
        let object: serde_json::Map<String, serde_json::Value> = self
            .values
            .iter()
            .map(|(key, value)| (key.clone(), value.to_json()))
            .collect();
        serde_json::Value::Object(object)
    }
}

/// The App blob store: installed Apps plus their index.
pub trait AppStore {
    fn list(&self) -> Result<Vec<AppMeta>, StoreError>;

    /// `id` must match `[A-Za-z0-9][A-Za-z0-9_.-]*`; otherwise returns `StoreError::Unsupported`.
    fn read(&self, id: &str) -> Result<Vec<u8>, StoreError>;

    /// `meta.id` must match `[A-Za-z0-9][A-Za-z0-9_.-]*`; otherwise returns `StoreError::Unsupported`.
    fn install(&mut self, meta: AppMeta, bytes: &[u8]) -> Result<(), StoreError>;

    /// `id` must match `[A-Za-z0-9][A-Za-z0-9_.-]*`; otherwise returns `StoreError::Unsupported`.
    fn uninstall(&mut self, id: &str) -> Result<(), StoreError>;
}

/// A key-value handle bound to one App namespace.
pub trait ScopedKv {
    fn get(&self, key: &str) -> Result<Option<KvValue>, StoreError>;

    /// Non-finite numbers (`NaN`, `Infinity`) are rejected with `StoreError::Unsupported`.
    fn set(&mut self, key: &str, value: &KvValue) -> Result<(), StoreError>;

    fn remove(&mut self, key: &str) -> Result<(), StoreError>;
}

/// The KV store: opens per-App namespaces.
pub trait KvStore {
    /// `namespace` must match `[A-Za-z0-9][A-Za-z0-9_.-]*`; otherwise returns `StoreError::Unsupported`.
    fn open(&self, namespace: &str) -> Result<Box<dyn ScopedKv>, StoreError>;
}
