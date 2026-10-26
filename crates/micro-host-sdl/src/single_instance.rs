//! Single-instance guard for the desktop host.
//!
//! Micro OS owns the desktop window: two visible copies would fight over the
//! same display and the same `~/.micros` store. The first launch therefore
//! takes an advisory `flock` on `<MICROS_HOME>/instance.lock` and records its
//! PID in that file. A duplicate reads the PID, shows the native "already
//! running" notice, activates the running window, and exits without ever
//! creating a window of its own.
//!
//! `flock` is released by the kernel when the owning process exits, so a crash
//! can never leave a stale guard behind; a leftover lock file is harmless.
//! Headless `--smoke` / `--os-smoke` runs are test harnesses and never take the
//! lock, so they can run while a desktop instance is open.

use std::ffi::c_int;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

/// Lock file below the native store root (`$MICROS_HOME` / `~/.micros`).
const LOCK_FILE: &str = "instance.lock";

/// The owner writes its PID immediately after locking; a duplicate that starts
/// in that window retries briefly instead of reporting an unknown owner.
const OWNER_PID_ATTEMPTS: usize = 20;
const OWNER_PID_RETRY_DELAY: Duration = Duration::from_millis(25);

/// Outcome of trying to become the one visible host.
pub enum Claim {
    /// This process owns the window and holds the lock until it exits.
    Primary(Guard),
    /// Another live process owns the window; `owner_pid` is its PID when the
    /// lock file could be read.
    Duplicate { owner_pid: Option<u32> },
}

/// Held by the primary instance for the lifetime of the event loop.
pub struct Guard {
    /// Kept open: closing the descriptor releases the `flock`.
    _file: File,
}

unsafe extern "C" {
    /// macOS notice + activation bridge; a no-op stub on other platforms.
    fn micro_host_single_instance_warn_and_activate(owner_pid: c_int);
}

/// Try to become the single visible host, using the default store root.
pub fn claim() -> Result<Claim, String> {
    claim_at(&micro_store::native::default_root())
}

/// Try to become the single visible host, placing the lock in `root`.
///
/// `root` is normally the native store root, so `MICROS_HOME` also selects an
/// isolated instance (used by tests and side-by-side development builds).
pub fn claim_at(root: &Path) -> Result<Claim, String> {
    std::fs::create_dir_all(root)
        .map_err(|error| format!("cannot create {}: {error}", root.display()))?;
    let path = root.join(LOCK_FILE);
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| format!("cannot open {}: {error}", path.display()))?;

    if take_lock(&file)? {
        /* The lock is what enforces one instance; the PID is only the
         * activation target. Keep the guard even if recording the PID fails so
         * a read-only or unusual filesystem still cannot open a second window. */
        if let Err(error) = record_owner(&mut file) {
            eprintln!("micro-host-sdl: cannot record the instance owner: {error}");
        }
        Ok(Claim::Primary(Guard { _file: file }))
    } else {
        let owner_pid = read_owner_pid(&mut file);
        Ok(Claim::Duplicate { owner_pid })
    }
}

/// Rewrite the lock file with the current PID for a duplicate to read.
fn record_owner(file: &mut File) -> std::io::Result<()> {
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    writeln!(file, "{}", std::process::id())?;
    file.flush()
}

/// Show the "already running" notice (macOS) and bring the owner forward.
///
/// Blocks until the user dismisses the notice; the caller exits immediately
/// afterwards, without opening a window.
pub fn notify_and_activate(owner_pid: Option<u32>) {
    match owner_pid {
        Some(pid) => eprintln!(
            "micro-host-sdl: Micro OS is already running (pid {pid}); activating that instance"
        ),
        None => eprintln!("micro-host-sdl: Micro OS is already running; activating that instance"),
    }
    let pid = owner_pid
        .and_then(|pid| c_int::try_from(pid).ok())
        .unwrap_or(0);
    unsafe { micro_host_single_instance_warn_and_activate(pid) };
}

/// Read the PID the lock owner recorded, retrying while it is still writing.
fn read_owner_pid(file: &mut File) -> Option<u32> {
    for attempt in 0..OWNER_PID_ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(OWNER_PID_RETRY_DELAY);
        }
        if file.seek(SeekFrom::Start(0)).is_err() {
            return None;
        }
        let mut text = String::new();
        if file.read_to_string(&mut text).is_err() {
            return None;
        }
        if let Some(pid) = text
            .lines()
            .next()
            .and_then(|line| line.trim().parse().ok())
        {
            return Some(pid);
        }
    }
    None
}

#[cfg(unix)]
fn take_lock(file: &File) -> Result<bool, String> {
    use std::os::fd::AsRawFd;

    // The BSD/Linux `flock` ABI is shared by every unix the native host builds
    // on. Values are from <sys/file.h>.
    const LOCK_EX: c_int = 2;
    const LOCK_NB: c_int = 4;

    unsafe extern "C" {
        fn flock(fd: c_int, operation: c_int) -> c_int;
    }

    if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } == 0 {
        return Ok(true);
    }
    let error = std::io::Error::last_os_error();
    if error.kind() == std::io::ErrorKind::WouldBlock {
        Ok(false)
    } else {
        Err(format!("cannot lock the instance file: {error}"))
    }
}

#[cfg(not(unix))]
fn take_lock(_file: &File) -> Result<bool, String> {
    // Windows has no flock; the single-instance guard is a unix host feature
    // because the desktop host currently targets macOS (and Linux for the SDL
    // fallback). Failing open keeps a Windows port runnable.
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::{Claim, claim_at};

    fn temp_root(name: &str) -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "micros-instance-{}-{name}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn primary(root: &std::path::Path) -> super::Guard {
        match claim_at(root).unwrap() {
            Claim::Primary(guard) => guard,
            Claim::Duplicate { owner_pid } => {
                panic!("first claim must win, got duplicate {owner_pid:?}")
            }
        }
    }

    #[test]
    fn first_claim_wins_and_records_its_pid() {
        let root = temp_root("primary");
        let guard = primary(&root);

        // A second open()+flock() in this process is a distinct file
        // description, exactly like a second host process would use.
        let recorded = std::fs::read_to_string(root.join("instance.lock")).unwrap();
        assert_eq!(recorded.trim().parse::<u32>().unwrap(), std::process::id());

        drop(guard);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn duplicate_reports_the_owner_and_does_not_take_the_lock() {
        let root = temp_root("duplicate");
        let guard = primary(&root);

        match claim_at(&root).unwrap() {
            Claim::Primary(_) => panic!("a second claim must not win"),
            Claim::Duplicate { owner_pid } => {
                assert_eq!(owner_pid, Some(std::process::id()));
            }
        }

        // Releasing the owner frees the instance slot for the next launch.
        drop(guard);
        match claim_at(&root).unwrap() {
            Claim::Primary(next) => drop(next),
            Claim::Duplicate { owner_pid } => {
                panic!("lock must be free after the owner exits, got {owner_pid:?}")
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
