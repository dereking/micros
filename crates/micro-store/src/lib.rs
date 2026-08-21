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
