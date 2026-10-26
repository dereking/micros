//! Discovery of external Apps on the native host's simulated SD card.
//!
//! The device scans `/sdcard/apps/*.mbc` for installable programs. macOS has no
//! removable volume, so the native host mirrors the same layout below its store
//! root: `<MICROS_HOME>/sdcard/apps/*.mbc` (default
//! `~/.micros/sdcard/apps`). Built-in Apps always win an id collision; duplicate
//! external ids, invalid images and overflow past the launcher's eight slots are
//! skipped and logged.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use micro_ir::decode;
use micro_store::default_root;

/// Matches the shell's fixed launcher slots (`apps/shell/app.ts`) and the
/// ESP32 host registry cap.
pub const MAX_LAUNCHER_APPS: usize = 8;

/// One launcher-visible App: metadata plus the MBC path to load on launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub path: PathBuf,
    /// True for an MBC discovered on the simulated SD card.
    pub external: bool,
}

/// The simulated card's installable-App directory.
pub fn external_app_dir() -> PathBuf {
    default_root().join("sdcard").join("apps")
}

/// Read one MBC and return its launcher metadata.
pub fn load_app(path: &Path, external: bool) -> Result<InstalledApp, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let image =
        decode(&bytes).map_err(|error| format!("cannot decode {}: {error}", path.display()))?;
    Ok(InstalledApp {
        id: image.metadata.id,
        name: image.metadata.name,
        icon: image.metadata.icon,
        path: path.to_path_buf(),
        external,
    })
}

/// Scan `dir` for `*.mbc` files, sorted by path. Missing directories (no card)
/// return an empty list; unusable files are skipped with a warning.
pub fn discover_external(dir: &Path) -> Vec<InstalledApp> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            eprintln!("[apps] cannot scan {}: {error}", dir.display());
            return Vec::new();
        }
    };

    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("mbc"))
        })
        .collect::<Vec<_>>();
    paths.sort();

    let mut apps = Vec::new();
    for path in paths {
        match load_app(&path, true) {
            Ok(app) if valid_external_id(&app.id) => apps.push(app),
            Ok(app) => eprintln!(
                "[apps] ignoring SD App {}: id '{}' is not a safe identifier",
                path.display(),
                app.id
            ),
            Err(error) => eprintln!("[apps] ignoring SD App: {error}"),
        }
    }
    apps
}

/// A launcher-visible external App id must be a short ASCII identifier so it is
/// safe as a per-App storage namespace and matches the ESP32 validator.
pub fn valid_external_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    if bytes.is_empty() || bytes.len() >= 32 || !bytes[0].is_ascii_alphanumeric() {
        return false;
    }
    bytes
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

/// Combine built-in and external Apps, built-ins first. The comparison key is
/// the manifest id (case-insensitive), never the display name. Built-ins win,
/// duplicate externals are skipped, and the list is capped at
/// [`MAX_LAUNCHER_APPS`] so the shell never has an off-screen entry.
pub fn merge_registry(
    builtin: Vec<InstalledApp>,
    external: Vec<InstalledApp>,
) -> Vec<InstalledApp> {
    let mut merged = Vec::new();
    let mut seen = HashSet::new();

    for app in builtin {
        if merged.len() == MAX_LAUNCHER_APPS {
            eprintln!(
                "[apps] launcher is full; built-in App '{}' is not listed",
                app.id
            );
            continue;
        }
        seen.insert(app.id.to_ascii_lowercase());
        merged.push(app);
    }

    for app in external {
        if !valid_external_id(&app.id) {
            eprintln!(
                "[apps] ignoring SD App {}: id '{}' is not a safe identifier",
                app.path.display(),
                app.id
            );
            continue;
        }
        let key = app.id.to_ascii_lowercase();
        if key == "shell" || seen.contains(&key) {
            eprintln!(
                "[apps] ignoring SD App {}: id '{}' is already registered or reserved",
                app.path.display(),
                app.id
            );
            continue;
        }
        if merged.len() == MAX_LAUNCHER_APPS {
            eprintln!("[apps] launcher is full; SD App '{}' is not listed", app.id);
            continue;
        }
        seen.insert(key);
        merged.push(app);
    }

    merged
}

/// Convenience entry point used by the OS-mode host: load built-ins from the
/// command line/bundle, then append the simulated card's Apps.
pub fn installed_registry(builtin_paths: &[PathBuf]) -> Vec<InstalledApp> {
    let mut builtin = Vec::new();
    for path in builtin_paths {
        match load_app(path, false) {
            Ok(app) => builtin.push(app),
            Err(error) => eprintln!("[apps] cannot load built-in App: {error}"),
        }
    }
    merge_registry(builtin, discover_external(&external_app_dir()))
}
