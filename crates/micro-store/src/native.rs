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
