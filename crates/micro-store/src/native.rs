use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::files::{
    FileStore, MAX_FILE_BYTES, MAX_LIST_ENTRIES, ScopedFiles, validate_app_id, validate_path,
};
use crate::{AppMeta, AppStore, ConfigDoc, KvStore, KvValue, ScopedKv, StoreError};

const APP_DIR: &str = "apps";
const KV_DIR: &str = "kv";
const MANIFEST_FILE: &str = "manifest.json";
const CONFIG_FILE: &str = "config.json";
const ROOT_NAME: &str = ".micros";

/// The native root used before the unified store layout (`~/.micros`).
pub const LEGACY_ROOT_NAME: &str = ".micrapp";

/// The native host root: `$MICROS_HOME`, or `~/.micros` when unset.
pub fn default_root() -> PathBuf {
    if let Some(root) = std::env::var_os("MICROS_HOME") {
        return PathBuf::from(root);
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(ROOT_NAME))
        .unwrap_or_else(|| PathBuf::from(ROOT_NAME))
}

/// Result of [`migrate_legacy_root`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Migration {
    /// Nothing to do: no legacy tree, or `MICROS_HOME` pins an explicit root.
    NotNeeded,
    /// The legacy tree was moved to the current root in one rename.
    Moved,
    /// Missing legacy files were copied into an already-existing root.
    Merged,
}

/// Move a pre-`~/.micros` install from `~/.micrapp` into the current root.
///
/// The migration runs once per launch and is idempotent:
///
/// * no `~/.micrapp` → [`Migration::NotNeeded`];
/// * `~/.micros` absent → rename the legacy tree into place (falling back to a
///   recursive copy when the rename cannot cross devices);
/// * `~/.micros` already exists → copy only the files it does not have, so new
///   data always wins and a partially used new root still gains the old KV,
///   Apps and per-App config documents.
///
/// The legacy tree is left in place after a merge as a backup; it is never
/// modified. `MICROS_HOME` disables the migration because an explicit root is
/// authoritative.
pub fn migrate_legacy_root() -> Result<Migration, StoreError> {
    if std::env::var_os("MICROS_HOME").is_some() {
        return Ok(Migration::NotNeeded);
    }
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Ok(Migration::NotNeeded);
    };
    migrate_legacy_root_from(&home.join(LEGACY_ROOT_NAME), &home.join(ROOT_NAME))
}

/// [`migrate_legacy_root`] with explicit directories, for tests and embedders.
pub fn migrate_legacy_root_from(legacy: &Path, current: &Path) -> Result<Migration, StoreError> {
    if !legacy.is_dir() {
        return Ok(Migration::NotNeeded);
    }
    if !current.exists() {
        if fs::rename(legacy, current).is_ok() {
            return Ok(Migration::Moved);
        }
        copy_tree(legacy, current)?;
        return Ok(Migration::Moved);
    }
    if !current.is_dir() {
        return Err(StoreError::Io(format!(
            "store root `{}` is not a directory",
            current.display()
        )));
    }
    copy_tree(legacy, current)?;
    Ok(Migration::Merged)
}

/// Recursively copy files that `destination` does not already have. Symlinks
/// and special files are skipped so the migration can never escape the root.
fn copy_tree(source: &Path, destination: &Path) -> Result<(), StoreError> {
    fs::create_dir_all(destination).map_err(io_error)?;
    for entry in fs::read_dir(source).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let file_type = entry.file_type().map_err(io_error)?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if file_type.is_file() && !target.exists() {
            fs::copy(entry.path(), &target).map_err(io_error)?;
        }
    }
    Ok(())
}

fn io_error(error: std::io::Error) -> StoreError {
    StoreError::Io(error.to_string())
}

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

    /// Rejects identifiers that could escape the store directory or that NVS
    /// could not host, matching the `[A-Za-z0-9][A-Za-z0-9_.-]*` shape.
    pub(crate) fn validate_ident(name: &str) -> Result<(), StoreError> {
        let mut chars = name.chars();
        let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
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
        Self::validate_ident(id)?;
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
        Self::validate_ident(&meta.id)?;
        let blob_path = self.apps_dir().join(format!("{}.mbc", meta.id));
        atomic_write(&blob_path, bytes)?;
        let mut manifest = self.load_manifest()?;
        manifest.retain(|entry| entry.id != meta.id);
        manifest.push(meta);
        self.save_manifest(&manifest)
    }

    fn uninstall(&mut self, id: &str) -> Result<(), StoreError> {
        Self::validate_ident(id)?;
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
        Self::validate_ident(namespace)?;
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
        self.root
            .join(KV_DIR)
            .join(format!("{}.json", self.namespace))
    }

    fn load(&self) -> Result<BTreeMap<String, KvValue>, StoreError> {
        match fs::read(self.kv_path()) {
            Ok(bytes) => {
                let raw: serde_json::Map<String, serde_json::Value> =
                    serde_json::from_slice(&bytes).map_err(|error| {
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
        let bytes =
            serde_json::to_vec_pretty(&json).map_err(|error| StoreError::Io(error.to_string()))?;
        atomic_write(&self.kv_path(), &bytes)
    }
}

impl ScopedKv for NativeScopedKv {
    fn get(&self, key: &str) -> Result<Option<KvValue>, StoreError> {
        Ok(self.load()?.get(key).cloned())
    }

    fn set(&mut self, key: &str, value: &KvValue) -> Result<(), StoreError> {
        if let KvValue::Number(value) = value
            && !value.is_finite()
        {
            return Err(StoreError::Unsupported(
                "non-finite numbers cannot be stored".into(),
            ));
        }
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

/// Native implementation of the unified `config` API.
///
/// Each App gets its own directory under the store root and a human-editable
/// `config.json`:
///
/// ```text
/// ~/.micros/<app-id>/config.json
/// ```
///
/// The root comes from `$MICROS_HOME` / `~/.micros` in production and from an
/// explicit path in tests, so a test never touches the real home directory.
#[derive(Debug, Clone)]
pub struct NativeConfigStore {
    root: PathBuf,
}

impl NativeConfigStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The App's config directory (`<root>/<app-id>`).
    pub fn app_dir(&self, app_id: &str) -> PathBuf {
        self.root.join(app_id)
    }

    /// Open the App's `config.json` through the shared `ScopedKv` contract.
    pub fn open(&self, app_id: &str) -> Result<Box<dyn ScopedKv>, StoreError> {
        NativeStore::validate_ident(app_id)?;
        Ok(Box::new(NativeConfigKv {
            path: self.app_dir(app_id).join(CONFIG_FILE),
        }))
    }
}

struct NativeConfigKv {
    path: PathBuf,
}

impl NativeConfigKv {
    fn load(&self) -> Result<ConfigDoc, StoreError> {
        match fs::read_to_string(&self.path) {
            Ok(raw) => ConfigDoc::parse(&raw),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ConfigDoc::default()),
            Err(error) => Err(StoreError::Io(error.to_string())),
        }
    }

    fn save(&self, document: &ConfigDoc) -> Result<(), StoreError> {
        atomic_write(&self.path, document.to_json_pretty().as_bytes())
    }
}

impl ScopedKv for NativeConfigKv {
    fn get(&self, key: &str) -> Result<Option<KvValue>, StoreError> {
        Ok(self.load()?.get(key))
    }

    fn set(&mut self, key: &str, value: &KvValue) -> Result<(), StoreError> {
        let mut document = self.load()?;
        document.set(key, value)?;
        self.save(&document)
    }

    fn remove(&mut self, key: &str) -> Result<(), StoreError> {
        let mut document = self.load()?;
        document.remove(key);
        self.save(&document)
    }
}

/// Native implementation of the App-private file store.
///
/// The native host simulates the device's removable volume at
/// `<store root>/sdcard`; each App owns
/// `<store root>/sdcard/micros/apps/<app-id>`. The layout deliberately mirrors
/// the ESP32 mount (`/sdcard/micros/apps/<app-id>`) so a path behaves the same
/// on both hosts.
///
/// Path validation is shared with the other backends. In addition, every
/// existing component is checked with `symlink_metadata` and symlinks are
/// rejected, so an App cannot turn a link in its directory into a read/write
/// escape into `~/.micros`, another App, or the host filesystem.
#[derive(Debug, Clone)]
pub struct NativeFileStore {
    root: PathBuf,
}

impl NativeFileStore {
    /// `root` is the simulated volume root (normally `<store>/sdcard`).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The private directory for one App.
    pub fn app_dir(&self, app_id: &str) -> PathBuf {
        self.root.join("micros").join("apps").join(app_id)
    }
}

impl FileStore for NativeFileStore {
    fn open(&self, app_id: &str) -> Result<Box<dyn ScopedFiles>, StoreError> {
        validate_app_id(app_id)?;
        let dir = self.app_dir(app_id);
        fs::create_dir_all(&dir).map_err(io_error)?;
        let metadata = fs::symlink_metadata(&dir).map_err(io_error)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(StoreError::Unsupported(format!(
                "App storage root `{}` is not a real directory",
                dir.display()
            )));
        }
        let root = fs::canonicalize(&dir).map_err(io_error)?;
        Ok(Box::new(NativeFiles { root }))
    }
}

struct NativeFiles {
    root: PathBuf,
}

impl NativeFiles {
    /// Resolve a validated relative path under the App root.
    ///
    /// `create_parents` creates missing intermediate directories for writes and
    /// `mkdir`; every component is walked without following symlinks and the
    /// resolved parent is canonicalized and checked for containment.
    fn checked_target(&self, path: &str, create_parents: bool) -> Result<PathBuf, StoreError> {
        validate_path(path)?;
        if path.is_empty() {
            return Ok(self.root.clone());
        }

        let components: Vec<&str> = path.split('/').collect();
        let mut current = self.root.clone();
        for (index, component) in components.iter().enumerate() {
            current.push(component);
            let is_last = index + 1 == components.len();
            match fs::symlink_metadata(&current) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() {
                        return Err(StoreError::Unsupported(
                            "symlinks are not allowed in App storage".into(),
                        ));
                    }
                    if !is_last && !metadata.is_dir() {
                        return Err(StoreError::Unsupported(
                            "a path component is not a directory".into(),
                        ));
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if is_last {
                        break;
                    }
                    if !create_parents {
                        return Err(StoreError::NotFound);
                    }
                    fs::create_dir(&current).map_err(io_error)?;
                }
                Err(error) => return Err(io_error(error)),
            }
        }

        let parent = current.parent().unwrap_or(&self.root);
        let canonical = fs::canonicalize(parent).map_err(io_error)?;
        if !canonical.starts_with(&self.root) {
            return Err(StoreError::Unsupported(
                "path escapes the App storage directory".into(),
            ));
        }
        Ok(current)
    }

    fn metadata(&self, path: &str) -> Result<Option<fs::Metadata>, StoreError> {
        match fs::symlink_metadata(self.checked_target(path, false)?) {
            Ok(metadata) if metadata.file_type().is_symlink() => Err(StoreError::Unsupported(
                "symlinks are not allowed in App storage".into(),
            )),
            Ok(metadata) => Ok(Some(metadata)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_error(error)),
        }
    }
}

impl ScopedFiles for NativeFiles {
    fn available(&self) -> bool {
        self.root.is_dir()
    }

    fn read_text(&self, path: &str) -> Result<Option<String>, StoreError> {
        if path.is_empty() {
            return Ok(Some(String::new()));
        }
        let target = match self.checked_target(path, false) {
            Ok(target) => target,
            Err(StoreError::NotFound) => return Ok(None),
            Err(error) => return Err(error),
        };
        let metadata = match fs::metadata(&target) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(error)),
        };
        if metadata.is_dir() {
            return Err(StoreError::Unsupported("path is a directory".into()));
        }
        if metadata.len() > MAX_FILE_BYTES as u64 {
            return Err(StoreError::Full);
        }
        let bytes = fs::read(&target).map_err(io_error)?;
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| StoreError::Corrupt("file is not UTF-8".into()))
    }

    fn write_text(&mut self, path: &str, text: &str) -> Result<(), StoreError> {
        if path.is_empty() {
            return Err(StoreError::Unsupported("cannot write the App root".into()));
        }
        if text.len() > MAX_FILE_BYTES {
            return Err(StoreError::Full);
        }
        let target = self.checked_target(path, true)?;
        if let Ok(metadata) = fs::symlink_metadata(&target) {
            if metadata.file_type().is_symlink() {
                return Err(StoreError::Unsupported(
                    "symlinks are not allowed in App storage".into(),
                ));
            }
            if metadata.is_dir() {
                return Err(StoreError::Unsupported("path is a directory".into()));
            }
        }
        fs::write(target, text.as_bytes()).map_err(io_error)
    }

    fn append_text(&mut self, path: &str, text: &str) -> Result<(), StoreError> {
        if path.is_empty() {
            return Err(StoreError::Unsupported("cannot write the App root".into()));
        }
        let target = self.checked_target(path, true)?;
        let current = match fs::symlink_metadata(&target) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(StoreError::Unsupported(
                        "symlinks are not allowed in App storage".into(),
                    ));
                }
                if metadata.is_dir() {
                    return Err(StoreError::Unsupported("path is a directory".into()));
                }
                metadata.len()
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(io_error(error)),
        };
        if current.saturating_add(text.len() as u64) > MAX_FILE_BYTES as u64 {
            return Err(StoreError::Full);
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(target)
            .map_err(io_error)?;
        use std::io::Write;
        file.write_all(text.as_bytes()).map_err(io_error)
    }

    fn exists(&self, path: &str) -> Result<bool, StoreError> {
        match self.metadata(path) {
            Ok(Some(_)) => Ok(true),
            Ok(None) | Err(StoreError::NotFound) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn remove(&mut self, path: &str) -> Result<bool, StoreError> {
        if path.is_empty() {
            return Ok(false);
        }
        let target = match self.checked_target(path, false) {
            Ok(target) => target,
            Err(StoreError::NotFound) => return Ok(false),
            Err(error) => return Err(error),
        };
        let metadata = match fs::symlink_metadata(&target) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(io_error(error)),
        };
        if metadata.file_type().is_symlink() {
            return Err(StoreError::Unsupported(
                "symlinks are not allowed in App storage".into(),
            ));
        }
        if metadata.is_dir() {
            fs::remove_dir_all(target).map_err(io_error)?;
        } else {
            fs::remove_file(target).map_err(io_error)?;
        }
        Ok(true)
    }

    fn list(&self, path: &str) -> Result<Vec<String>, StoreError> {
        let target = self.checked_target(path, false)?;
        let metadata = self.metadata(path)?.ok_or(StoreError::NotFound)?;
        if !metadata.is_dir() {
            return Err(StoreError::Unsupported("path is not a directory".into()));
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(&target).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let file_type = entry.file_type().map_err(io_error)?;
            if file_type.is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "." || name == ".." {
                continue;
            }
            if file_type.is_dir() {
                entries.push(format!("{name}/"));
            } else {
                entries.push(name);
            }
        }
        entries.sort();
        entries.truncate(MAX_LIST_ENTRIES);
        Ok(entries)
    }

    fn mkdir(&mut self, path: &str) -> Result<(), StoreError> {
        let target = self.checked_target(path, true)?;
        if let Ok(metadata) = fs::symlink_metadata(&target) {
            if metadata.file_type().is_symlink() {
                return Err(StoreError::Unsupported(
                    "symlinks are not allowed in App storage".into(),
                ));
            }
            if !metadata.is_dir() {
                return Err(StoreError::Unsupported("path is a file".into()));
            }
            return Ok(());
        }
        fs::create_dir_all(target).map_err(io_error)
    }

    fn size(&self, path: &str) -> Result<Option<u64>, StoreError> {
        let metadata = match self.metadata(path) {
            Ok(Some(metadata)) => metadata,
            Ok(None) | Err(StoreError::NotFound) => return Ok(None),
            Err(error) => return Err(error),
        };
        if metadata.is_dir() {
            Ok(Some(0))
        } else {
            Ok(Some(metadata.len()))
        }
    }
}
