//! SD-card backend for the App-private `storage.*` API.
//!
//! The `micro_sd` firmware component owns the SPI bus, the CH422G chip-select
//! line and the FATFS mount. This module only translates the portable
//! [`ScopedFiles`] contract into its C ABI; the C side independently re-checks
//! the App id and relative path before prepending
//! `/sdcard/micros/apps/<app-id>/`, so a bug in the Rust layer cannot turn into
//! a cross-App read.

use std::ffi::{CString, c_char};

use micro_store::{
    MAX_FILE_BYTES, MAX_LIST_ENTRIES, ScopedFiles, StoreError, validate_app_id, validate_path,
};

unsafe extern "C" {
    fn micro_sd_available() -> i32;
    fn micro_sd_read(
        app_id: *const c_char,
        path: *const c_char,
        out: *mut *mut c_char,
        out_len: *mut usize,
    ) -> i32;
    fn micro_sd_write(
        app_id: *const c_char,
        path: *const c_char,
        data: *const c_char,
        data_len: usize,
        append: i32,
    ) -> i32;
    fn micro_sd_exists(app_id: *const c_char, path: *const c_char, out: *mut i32) -> i32;
    fn micro_sd_remove(app_id: *const c_char, path: *const c_char, out: *mut i32) -> i32;
    fn micro_sd_list(
        app_id: *const c_char,
        path: *const c_char,
        out: *mut *mut c_char,
        out_len: *mut usize,
    ) -> i32;
    fn micro_sd_mkdir(app_id: *const c_char, path: *const c_char, out: *mut i32) -> i32;
    fn micro_sd_size(app_id: *const c_char, path: *const c_char, out: *mut i64) -> i32;
    fn micro_sd_free(ptr: *mut c_char);
}

pub(crate) struct SdFiles {
    app_id: CString,
}

impl SdFiles {
    pub(crate) fn new(app_id: &str) -> Option<Self> {
        validate_app_id(app_id).ok()?;
        Some(Self {
            app_id: CString::new(app_id).ok()?,
        })
    }

    fn path(&self, path: &str) -> Result<CString, StoreError> {
        validate_path(path)?;
        CString::new(path).map_err(|_| StoreError::Unsupported("path contains NUL".into()))
    }
}

fn io_error(operation: &str) -> StoreError {
    StoreError::Io(format!("SD {operation} failed"))
}

impl ScopedFiles for SdFiles {
    fn available(&self) -> bool {
        // The C side returns 0 before the card mounts and after a failed mount.
        unsafe { micro_sd_available() != 0 }
    }

    fn read_text(&self, path: &str) -> Result<Option<String>, StoreError> {
        if path.is_empty() {
            return Ok(Some(String::new()));
        }
        let path = self.path(path)?;
        let mut out: *mut c_char = std::ptr::null_mut();
        let mut len = 0_usize;
        if unsafe { micro_sd_read(self.app_id.as_ptr(), path.as_ptr(), &mut out, &mut len) } != 0 {
            return Err(io_error("read"));
        }
        if out.is_null() {
            return Ok(None);
        }
        if len > MAX_FILE_BYTES {
            unsafe { micro_sd_free(out) };
            return Err(StoreError::Full);
        }
        let bytes = unsafe { std::slice::from_raw_parts(out as *const u8, len) };
        let text = String::from_utf8(bytes.to_vec())
            .map_err(|_| StoreError::Corrupt("file is not UTF-8".into()));
        unsafe { micro_sd_free(out) };
        text.map(Some)
    }

    fn write_text(&mut self, path: &str, text: &str) -> Result<(), StoreError> {
        self.write(path, text, false)
    }

    fn append_text(&mut self, path: &str, text: &str) -> Result<(), StoreError> {
        self.write(path, text, true)
    }

    fn exists(&self, path: &str) -> Result<bool, StoreError> {
        if path.is_empty() {
            return Ok(self.available());
        }
        let path = self.path(path)?;
        let mut out = 0_i32;
        if unsafe { micro_sd_exists(self.app_id.as_ptr(), path.as_ptr(), &mut out) } != 0 {
            return Err(io_error("exists"));
        }
        Ok(out != 0)
    }

    fn remove(&mut self, path: &str) -> Result<bool, StoreError> {
        if path.is_empty() {
            return Ok(false);
        }
        let path = self.path(path)?;
        let mut out = 0_i32;
        if unsafe { micro_sd_remove(self.app_id.as_ptr(), path.as_ptr(), &mut out) } != 0 {
            return Err(io_error("remove"));
        }
        Ok(out != 0)
    }

    fn list(&self, path: &str) -> Result<Vec<String>, StoreError> {
        let path = self.path(path)?;
        let mut out: *mut c_char = std::ptr::null_mut();
        let mut len = 0_usize;
        if unsafe { micro_sd_list(self.app_id.as_ptr(), path.as_ptr(), &mut out, &mut len) } != 0 {
            return Err(io_error("list"));
        }
        if out.is_null() {
            return Ok(Vec::new());
        }
        let bytes = unsafe { std::slice::from_raw_parts(out as *const u8, len) };
        let text = String::from_utf8_lossy(bytes).into_owned();
        unsafe { micro_sd_free(out) };
        Ok(text
            .lines()
            .filter(|line| !line.is_empty())
            .take(MAX_LIST_ENTRIES)
            .map(str::to_owned)
            .collect())
    }

    fn mkdir(&mut self, path: &str) -> Result<(), StoreError> {
        let path = self.path(path)?;
        let mut out = 0_i32;
        if unsafe { micro_sd_mkdir(self.app_id.as_ptr(), path.as_ptr(), &mut out) } != 0 {
            return Err(io_error("mkdir"));
        }
        Ok(())
    }

    fn size(&self, path: &str) -> Result<Option<u64>, StoreError> {
        let path = self.path(path)?;
        let mut out = -1_i64;
        if unsafe { micro_sd_size(self.app_id.as_ptr(), path.as_ptr(), &mut out) } != 0 {
            return Err(io_error("size"));
        }
        Ok((out >= 0).then_some(out as u64))
    }
}

impl SdFiles {
    fn write(&mut self, path: &str, text: &str, append: bool) -> Result<(), StoreError> {
        if path.is_empty() {
            return Err(StoreError::Unsupported("cannot write the App root".into()));
        }
        if text.len() > MAX_FILE_BYTES {
            return Err(StoreError::Full);
        }
        let path = self.path(path)?;
        let result = unsafe {
            micro_sd_write(
                self.app_id.as_ptr(),
                path.as_ptr(),
                text.as_ptr().cast::<c_char>(),
                text.len(),
                i32::from(append),
            )
        };
        if result != 0 {
            return Err(io_error("write"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::SdFiles;
    use micro_store::ScopedFiles;

    #[test]
    fn empty_paths_are_the_app_root() {
        let files = SdFiles::new("counter").expect("valid app");
        assert!(files.exists("").expect("exists"));
        assert_eq!(files.read_text("").expect("read"), Some(String::new()));
        assert!(!files.remove("").expect("remove"));
    }
}
