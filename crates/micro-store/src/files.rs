//! App-private file storage with one path policy shared by every host.
//!
//! The SDK's `storage.*` calls never see a host path. They pass a path relative
//! to the calling App's private directory, and the host resolves it under that
//! directory:
//!
//! ```text
//! ESP32  /sdcard/micros/apps/<app-id>/<path>
//! macOS  <MICROS_HOME>/sdcard/micros/apps/<app-id>/<path>   (default ~/.micros)
//! Web    localStorage keys owned by <app-id>          (simulator)
//! ```
//!
//! This module owns the portable parts:
//!
//! * [`validate_app_id`] / [`validate_path`] enforce a conservative ASCII
//!   grammar. Absolute paths, `.`/`..`, backslashes, drive/stream separators,
//!   empty components, over-long names and Windows device names are rejected
//!   before a host touches a filesystem. Two Apps can therefore never share a
//!   resolved path, even on a case-insensitive or Unicode-normalizing host.
//! * [`ScopedFiles`] is the per-App file handle. Hosts implement it (native
//!   filesystem, browser storage, SD card) using the same policy.
//! * [`MemoryFileStore`] is a deterministic reference backend used by tests and
//!   by native builds of the Web host.
//!
//! File contents are UTF-8 text and capped at [`MAX_FILE_BYTES`]. `list`
//! returns at most [`MAX_LIST_ENTRIES`] immediate children, sorted by name, with
//! directories suffixed by `/`.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use crate::StoreError;

/// Maximum length of a relative path in bytes.
pub const MAX_PATH_BYTES: usize = 240;
/// Maximum length of one path component in bytes.
pub const MAX_SEGMENT_BYTES: usize = 63;
/// Maximum number of `/`-separated components.
pub const MAX_PATH_DEPTH: usize = 8;
/// Maximum size of one file's UTF-8 contents.
pub const MAX_FILE_BYTES: usize = 64 * 1024;
/// Maximum number of entries returned by `list`.
pub const MAX_LIST_ENTRIES: usize = 128;

const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A per-App file handle. Paths are always relative to the App's private
/// directory and must pass [`validate_path`].
///
/// Implementations treat an empty path as the App's storage root for `list`,
/// `exists`, `mkdir` and `size`, and as invalid for file operations.
pub trait ScopedFiles {
    /// Whether the backing volume is mounted and usable. When false, every
    /// other call fails with `StoreError::Unsupported`/`StoreError::Io`.
    fn available(&self) -> bool;

    /// Read one UTF-8 file. `Ok(None)` means the file does not exist.
    fn read_text(&self, path: &str) -> Result<Option<String>, StoreError>;

    /// Create or replace a file, creating missing parent directories.
    fn write_text(&mut self, path: &str, text: &str) -> Result<(), StoreError>;

    /// Append to a file, creating it when missing. The resulting file must not
    /// exceed [`MAX_FILE_BYTES`].
    fn append_text(&mut self, path: &str, text: &str) -> Result<(), StoreError>;

    fn exists(&self, path: &str) -> Result<bool, StoreError>;

    /// Remove a file, or a directory and everything below it. `Ok(false)` means
    /// the path did not exist.
    fn remove(&mut self, path: &str) -> Result<bool, StoreError>;

    /// List immediate children, sorted by name; directories end with `/`.
    fn list(&self, path: &str) -> Result<Vec<String>, StoreError>;

    /// Create a directory and missing parents. Existing directories succeed.
    fn mkdir(&mut self, path: &str) -> Result<(), StoreError>;

    /// File size in bytes, `Some(0)` for a directory, `None` when missing.
    fn size(&self, path: &str) -> Result<Option<u64>, StoreError>;
}

/// A factory that opens one App's private file directory.
pub trait FileStore {
    fn open(&self, app_id: &str) -> Result<Box<dyn ScopedFiles>, StoreError>;
}

/// Validate an App id before it is used as a directory name.
///
/// The shape matches the KV namespace rule (`[A-Za-z0-9][A-Za-z0-9_.-]*`) and
/// is additionally capped at 32 bytes so a FAT path stays well within limits.
pub fn validate_app_id(app_id: &str) -> Result<(), StoreError> {
    let bytes = app_id.as_bytes();
    let Some(first) = bytes.first() else {
        return Err(StoreError::Unsupported("empty app id".into()));
    };
    if !first.is_ascii_alphanumeric()
        || app_id.len() > 32
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(StoreError::Unsupported(format!(
            "invalid app id `{app_id}`"
        )));
    }
    Ok(())
}

/// Validate an App-relative storage path.
///
/// Empty input is the App root and is accepted; callers that need a file reject
/// it themselves. The accepted grammar is deliberately narrow:
///
/// * relative, `/`-separated, at most [`MAX_PATH_DEPTH`] components;
/// * each component is 1..=[`MAX_SEGMENT_BYTES`] bytes of ASCII alphanumerics,
///   `_`, `-`, `.` or space;
/// * no component is `.` or `..`, starts/ends with a space, or ends with `.`;
/// * no control characters, backslashes, colons, quotes or other host-specific
///   separators;
/// * no Windows reserved device names (`CON`, `COM1`, …).
pub fn validate_path(path: &str) -> Result<(), StoreError> {
    if path.is_empty() {
        return Ok(());
    }
    if path.len() > MAX_PATH_BYTES {
        return Err(StoreError::Unsupported(format!(
            "path exceeds {MAX_PATH_BYTES} bytes"
        )));
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(StoreError::Unsupported(
            "absolute paths are not allowed".into(),
        ));
    }
    if path
        .bytes()
        .any(|byte| byte < 0x20 || byte == 0x7f || byte == b'\\' || byte == b':')
    {
        return Err(StoreError::Unsupported(
            "path contains a forbidden character".into(),
        ));
    }

    let mut depth = 0;
    for segment in path.split('/') {
        depth += 1;
        if depth > MAX_PATH_DEPTH {
            return Err(StoreError::Unsupported(format!(
                "path exceeds {MAX_PATH_DEPTH} components"
            )));
        }
        if segment.is_empty() {
            return Err(StoreError::Unsupported(
                "path contains an empty component".into(),
            ));
        }
        if segment == "." || segment == ".." {
            return Err(StoreError::Unsupported(
                "path traversal is not allowed".into(),
            ));
        }
        if segment.len() > MAX_SEGMENT_BYTES {
            return Err(StoreError::Unsupported(format!(
                "path component exceeds {MAX_SEGMENT_BYTES} bytes"
            )));
        }
        if segment.starts_with(' ') || segment.ends_with(' ') || segment.ends_with('.') {
            return Err(StoreError::Unsupported(
                "path component may not start/end with a space or end with a dot".into(),
            ));
        }
        if !segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b' '))
        {
            return Err(StoreError::Unsupported(
                "path component contains an unsupported character".into(),
            ));
        }
        if is_reserved_name(segment) {
            return Err(StoreError::Unsupported(
                "path component is a reserved device name".into(),
            ));
        }
    }
    Ok(())
}

fn is_reserved_name(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment);
    let upper = stem.to_ascii_uppercase();
    RESERVED_NAMES.contains(&upper.as_str())
}

/// Shared in-memory backend used by tests and by native builds of the Web host.
///
/// The same store can open several App handles; a handle can only see its own
/// `app_id` prefix, which mirrors the real per-App directory boundary.
#[derive(Clone, Default)]
pub struct MemoryFileStore {
    state: Rc<RefCell<MemoryState>>,
}

#[derive(Default)]
struct MemoryState {
    files: BTreeMap<String, Vec<u8>>,
    dirs: BTreeSet<String>,
}

impl MemoryFileStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FileStore for MemoryFileStore {
    fn open(&self, app_id: &str) -> Result<Box<dyn ScopedFiles>, StoreError> {
        validate_app_id(app_id)?;
        Ok(Box::new(MemoryFiles {
            state: Rc::clone(&self.state),
            app_id: app_id.to_owned(),
        }))
    }
}

struct MemoryFiles {
    state: Rc<RefCell<MemoryState>>,
    app_id: String,
}

impl MemoryFiles {
    fn key(&self, path: &str) -> String {
        if path.is_empty() {
            self.app_id.clone()
        } else {
            format!("{}/{path}", self.app_id)
        }
    }

    fn ensure_parents(&self, state: &mut MemoryState, path: &str) {
        let mut current = self.app_id.clone();
        let components: Vec<&str> = path.split('/').collect();
        for component in &components[..components.len().saturating_sub(1)] {
            current.push('/');
            current.push_str(component);
            state.dirs.insert(current.clone());
        }
    }

    fn is_dir(&self, state: &MemoryState, path: &str) -> bool {
        state.dirs.contains(&self.key(path))
    }

    fn is_file(&self, state: &MemoryState, path: &str) -> bool {
        state.files.contains_key(&self.key(path))
    }

    fn has_children(&self, state: &MemoryState, path: &str) -> bool {
        let key = self.key(path);
        let prefix = format!("{key}/");
        state
            .files
            .keys()
            .any(|candidate| candidate.starts_with(&prefix))
            || state
                .dirs
                .iter()
                .any(|candidate| candidate.starts_with(&prefix))
    }
}

impl ScopedFiles for MemoryFiles {
    fn available(&self) -> bool {
        true
    }

    fn read_text(&self, path: &str) -> Result<Option<String>, StoreError> {
        validate_path(path)?;
        if path.is_empty() {
            return Ok(Some(String::new()));
        }
        let state = self.state.borrow();
        if self.is_dir(&state, path) {
            return Err(StoreError::Unsupported("path is a directory".into()));
        }
        let Some(bytes) = state.files.get(&self.key(path)) else {
            return Ok(None);
        };
        if bytes.len() > MAX_FILE_BYTES {
            return Err(StoreError::Full);
        }
        String::from_utf8(bytes.clone())
            .map(Some)
            .map_err(|_| StoreError::Corrupt("file is not UTF-8".into()))
    }

    fn write_text(&mut self, path: &str, text: &str) -> Result<(), StoreError> {
        validate_path(path)?;
        if path.is_empty() {
            return Err(StoreError::Unsupported("cannot write the App root".into()));
        }
        if text.len() > MAX_FILE_BYTES {
            return Err(StoreError::Full);
        }
        let mut state = self.state.borrow_mut();
        if self.is_dir(&state, path) {
            return Err(StoreError::Unsupported("path is a directory".into()));
        }
        self.ensure_parents(&mut state, path);
        state.files.insert(self.key(path), text.as_bytes().to_vec());
        state.dirs.remove(&self.key(path));
        Ok(())
    }

    fn append_text(&mut self, path: &str, text: &str) -> Result<(), StoreError> {
        validate_path(path)?;
        if path.is_empty() {
            return Err(StoreError::Unsupported("cannot write the App root".into()));
        }
        let mut state = self.state.borrow_mut();
        if self.is_dir(&state, path) {
            return Err(StoreError::Unsupported("path is a directory".into()));
        }
        let key = self.key(path);
        let current = state.files.get(&key).map_or(0, Vec::len);
        if current + text.len() > MAX_FILE_BYTES {
            return Err(StoreError::Full);
        }
        self.ensure_parents(&mut state, path);
        state
            .files
            .entry(key)
            .or_default()
            .extend_from_slice(text.as_bytes());
        Ok(())
    }

    fn exists(&self, path: &str) -> Result<bool, StoreError> {
        validate_path(path)?;
        let state = self.state.borrow();
        Ok(self.is_file(&state, path)
            || self.is_dir(&state, path)
            || self.has_children(&state, path))
    }

    fn remove(&mut self, path: &str) -> Result<bool, StoreError> {
        validate_path(path)?;
        if path.is_empty() {
            return Ok(false);
        }
        let mut state = self.state.borrow_mut();
        let key = self.key(path);
        let prefix = format!("{key}/");
        let file_removed = state.files.remove(&key).is_some();
        let dir_removed = state.dirs.remove(&key);
        let descendants: Vec<String> = state
            .files
            .keys()
            .filter(|candidate| candidate.starts_with(&prefix))
            .cloned()
            .collect();
        for descendant in descendants {
            state.files.remove(&descendant);
        }
        let dirs: Vec<String> = state
            .dirs
            .iter()
            .filter(|candidate| candidate.starts_with(&prefix))
            .cloned()
            .collect();
        for dir in dirs {
            state.dirs.remove(&dir);
        }
        Ok(file_removed || dir_removed)
    }

    fn list(&self, path: &str) -> Result<Vec<String>, StoreError> {
        validate_path(path)?;
        let state = self.state.borrow();
        if self.is_file(&state, path) {
            return Err(StoreError::Unsupported("path is a file".into()));
        }
        let root = self.key(path);
        let prefix = format!("{root}/");
        let mut entries = BTreeSet::new();
        for key in state.files.keys() {
            let Some(rest) = key.strip_prefix(&prefix) else {
                continue;
            };
            let mut parts = rest.split('/');
            let Some(first) = parts.next().filter(|part| !part.is_empty()) else {
                continue;
            };
            if parts.next().is_some() || state.dirs.contains(&format!("{prefix}{first}")) {
                entries.insert(format!("{first}/"));
            } else {
                entries.insert(first.to_owned());
            }
            if entries.len() == MAX_LIST_ENTRIES {
                break;
            }
        }
        if entries.len() < MAX_LIST_ENTRIES {
            for key in &state.dirs {
                let Some(rest) = key.strip_prefix(&prefix) else {
                    continue;
                };
                let mut parts = rest.split('/');
                let Some(first) = parts.next().filter(|part| !part.is_empty()) else {
                    continue;
                };
                let _ = parts.next();
                entries.insert(format!("{first}/"));
                if entries.len() == MAX_LIST_ENTRIES {
                    break;
                }
            }
        }
        Ok(entries.into_iter().take(MAX_LIST_ENTRIES).collect())
    }

    fn mkdir(&mut self, path: &str) -> Result<(), StoreError> {
        validate_path(path)?;
        let mut state = self.state.borrow_mut();
        if self.is_file(&state, path) {
            return Err(StoreError::Unsupported("path is a file".into()));
        }
        let mut current = self.app_id.clone();
        state.dirs.insert(current.clone());
        if path.is_empty() {
            return Ok(());
        }
        for component in path.split('/') {
            current.push('/');
            current.push_str(component);
            state.dirs.insert(current.clone());
        }
        Ok(())
    }

    fn size(&self, path: &str) -> Result<Option<u64>, StoreError> {
        validate_path(path)?;
        let state = self.state.borrow();
        if let Some(bytes) = state.files.get(&self.key(path)) {
            return Ok(Some(bytes.len() as u64));
        }
        if self.is_dir(&state, path) {
            return Ok(Some(0));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_escaping_and_ambiguous_paths() {
        for path in [
            "/etc/passwd",
            "\\windows\\system32",
            "../other/secret.txt",
            "notes/../../other/secret.txt",
            "notes//today.txt",
            "./today.txt",
            "notes/.",
            "C:notes.txt",
            "notes\\today.txt",
            "bad\0name",
            "trailing.",
            " leading",
            "trailing ",
            "CON",
            "com1.txt",
        ] {
            assert!(validate_path(path).is_err(), "{path:?} should be rejected");
        }
    }

    #[test]
    fn accepts_relative_app_paths() {
        for path in [
            "",
            "notes.txt",
            "notes/today.txt",
            "2026-10-26.json",
            "dir with space/file name.txt",
            "a/b/c/d/e/f/g/h.txt",
        ] {
            validate_path(path).expect(path);
        }
    }

    #[test]
    fn rejects_overlong_names_and_deep_paths() {
        assert!(validate_path(&"a".repeat(MAX_SEGMENT_BYTES + 1)).is_err());
        assert!(validate_path(&"a/".repeat(MAX_PATH_DEPTH + 1)).is_err());
        assert!(validate_path(&format!("{}a", "a/".repeat(MAX_PATH_DEPTH - 1))).is_ok());
    }

    #[test]
    fn memory_store_isolates_apps_and_hides_parent_paths() {
        let store = MemoryFileStore::new();
        let mut a = store.open("app-a").expect("open a");
        let mut b = store.open("app-b").expect("open b");
        a.write_text("notes/today.txt", "a").expect("write a");
        b.write_text("notes/today.txt", "b").expect("write b");

        assert_eq!(a.read_text("notes/today.txt").unwrap(), Some("a".into()));
        assert_eq!(b.read_text("notes/today.txt").unwrap(), Some("b".into()));
        assert_eq!(
            a.list("").unwrap(),
            vec!["notes/".to_owned()],
            "root lists only a directory"
        );
        assert_eq!(
            a.list("notes").unwrap(),
            vec!["today.txt".to_owned()],
            "directory lists only the file"
        );
        assert_eq!(b.size("notes/today.txt").unwrap(), Some(1));
    }

    #[test]
    fn memory_store_rejects_traversal_before_touching_storage() {
        let store = MemoryFileStore::new();
        let mut a = store.open("app-a").expect("open a");
        let mut b = store.open("app-b").expect("open b");
        b.write_text("secret.txt", "secret").expect("write b");
        assert!(a.read_text("../app-b/secret.txt").is_err());
        assert!(a.write_text("../app-b/secret.txt", "pwn").is_err());
        assert!(a.remove("../app-b/secret.txt").is_err());
        assert_eq!(b.read_text("secret.txt").unwrap(), Some("secret".into()));
    }

    #[test]
    fn memory_store_caps_files_and_lists() {
        let store = MemoryFileStore::new();
        let mut a = store.open("app-a").expect("open a");
        a.write_text("big.txt", &"x".repeat(MAX_FILE_BYTES))
            .expect("max size");
        assert!(
            a.write_text("bigger.txt", &"x".repeat(MAX_FILE_BYTES + 1))
                .is_err()
        );
        for index in 0..(MAX_LIST_ENTRIES + 5) {
            a.write_text(&format!("file{index}.txt"), "x")
                .expect("write");
        }
        assert_eq!(a.list("").unwrap().len(), MAX_LIST_ENTRIES);
    }
}
