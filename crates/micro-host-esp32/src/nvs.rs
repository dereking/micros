//! NVS-backed `ScopedKv` for the ESP32 host.
//!
//! The firmware component (`micro_kv.c`) owns NVS access and treats values as
//! opaque NUL-terminated strings. This module adds the scalar type tag so a
//! number written by `kv.setNumber` comes back as a number, not text:
//!
//! | value  | NVS string |
//! |--------|------------|
//! | number | `n:3.5`    |
//! | string | `s:hello`  |
//! | bool   | `b:1` / `b:0` |
//! | null   | `z`        |
//!
//! Keys are rejected if they are empty or longer than NVS can represent, which
//! keeps untrusted App code from silently aliasing another key after NVS
//! truncation.

#[cfg(target_os = "espidf")]
use std::ffi::CString;

#[cfg(target_os = "espidf")]
use micro_store::ConfigDoc;
use micro_store::KvValue;
#[cfg(target_os = "espidf")]
use micro_store::{ScopedKv, StoreError};

/// NVS namespace holding every App's config document. Keeping one namespace
/// lets the key be the App id, which already fits NVS's 15-byte limit.
#[cfg(target_os = "espidf")]
const CONFIG_NAMESPACE: &str = "micro_config";
/// The `micro_kv` component reads into a fixed buffer; config documents are
/// small, so a document that does not fit is reported as `StoreError::Full`
/// instead of being truncated.
#[cfg(target_os = "espidf")]
const CONFIG_BUFFER: usize = 4096;

pub(crate) fn encode_value(value: &KvValue) -> Option<String> {
    match value {
        KvValue::Number(number) if number.is_finite() => Some(format!("n:{number}")),
        KvValue::Number(_) => None,
        KvValue::String(text) => Some(format!("s:{text}")),
        KvValue::Bool(flag) => Some(if *flag { "b:1" } else { "b:0" }.to_owned()),
        KvValue::Null => Some("z".to_owned()),
    }
}

pub(crate) fn decode_value(raw: &str) -> Option<KvValue> {
    if raw == "z" {
        return Some(KvValue::Null);
    }
    if raw.len() < 2 {
        return None;
    }
    let (tag, rest) = raw.split_at(2);
    match tag {
        "n:" => rest.parse::<f64>().ok().map(KvValue::Number),
        "s:" => Some(KvValue::String(rest.to_owned())),
        "b:" => match rest {
            "1" => Some(KvValue::Bool(true)),
            "0" => Some(KvValue::Bool(false)),
            _ => None,
        },
        _ => None,
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() < 16 && !name.contains('\0')
}

#[cfg(target_os = "espidf")]
unsafe extern "C" {
    fn micro_kv_get(
        namespace_id: *const std::ffi::c_char,
        key: *const std::ffi::c_char,
        out: *mut std::ffi::c_char,
        cap: usize,
    ) -> std::ffi::c_int;
    fn micro_kv_set(
        namespace_id: *const std::ffi::c_char,
        key: *const std::ffi::c_char,
        value: *const std::ffi::c_char,
    ) -> std::ffi::c_int;
    fn micro_kv_remove(
        namespace_id: *const std::ffi::c_char,
        key: *const std::ffi::c_char,
    ) -> std::ffi::c_int;
}

#[cfg(target_os = "espidf")]
pub struct NvsKv {
    namespace: String,
}

#[cfg(target_os = "espidf")]
impl NvsKv {
    pub fn new(namespace: &str) -> Self {
        Self {
            namespace: namespace.to_owned(),
        }
    }

    fn namespace_c(&self) -> Result<CString, StoreError> {
        if !valid_name(&self.namespace) {
            return Err(StoreError::Unsupported(format!(
                "invalid NVS namespace `{}`",
                self.namespace
            )));
        }
        CString::new(self.namespace.as_str())
            .map_err(|_| StoreError::Unsupported("NVS namespace contains NUL".into()))
    }

    fn key_c(key: &str) -> Result<CString, StoreError> {
        if !valid_name(key) {
            return Err(StoreError::Unsupported(format!("invalid NVS key `{key}`")));
        }
        CString::new(key).map_err(|_| StoreError::Unsupported("NVS key contains NUL".into()))
    }
}

#[cfg(target_os = "espidf")]
impl micro_store::ScopedKv for NvsKv {
    fn get(&self, key: &str) -> Result<Option<KvValue>, StoreError> {
        let namespace = self.namespace_c()?;
        let key = Self::key_c(key)?;
        let mut buffer = [0u8; 4096];
        let length = unsafe {
            micro_kv_get(
                namespace.as_ptr(),
                key.as_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            )
        };
        if length == 0 {
            return Ok(None);
        }
        if length < 0 {
            return Err(StoreError::Io(format!("NVS read failed ({length})")));
        }
        let raw = std::str::from_utf8(&buffer[..length as usize])
            .map_err(|_| StoreError::Corrupt("NVS value is not UTF-8".into()))?;
        decode_value(raw)
            .map(Some)
            .ok_or_else(|| StoreError::Corrupt("unknown NVS value tag".into()))
    }

    fn set(&mut self, key: &str, value: &KvValue) -> Result<(), StoreError> {
        let namespace = self.namespace_c()?;
        let key = Self::key_c(key)?;
        let encoded = encode_value(value)
            .ok_or_else(|| StoreError::Unsupported("value cannot be stored".into()))?;
        let encoded = CString::new(encoded)
            .map_err(|_| StoreError::Unsupported("value contains NUL".into()))?;
        let result = unsafe { micro_kv_set(namespace.as_ptr(), key.as_ptr(), encoded.as_ptr()) };
        if result == 0 {
            Ok(())
        } else {
            Err(StoreError::Io(format!("NVS write failed ({result})")))
        }
    }

    fn remove(&mut self, key: &str) -> Result<(), StoreError> {
        let namespace = self.namespace_c()?;
        let key = Self::key_c(key)?;
        let result = unsafe { micro_kv_remove(namespace.as_ptr(), key.as_ptr()) };
        if result == 0 {
            Ok(())
        } else {
            Err(StoreError::Io(format!("NVS erase failed ({result})")))
        }
    }
}

/// NVS-backed `ScopedKv` for the unified `config` API.
///
/// Each App's config is one JSON document (`ConfigDoc`, the same format as the
/// native `config.json`) stored as a single NVS string on the `micro_cfg`
/// partition. The NVS key is the App id inside the shared `micro_config`
/// namespace, so no per-App namespace can exceed the 15-byte limit.
#[cfg(target_os = "espidf")]
pub struct NvsConfigKv {
    app_id: String,
}

#[cfg(target_os = "espidf")]
impl NvsConfigKv {
    pub fn new(app_id: &str) -> Self {
        Self {
            app_id: app_id.to_owned(),
        }
    }

    fn namespace_c(&self) -> Result<CString, StoreError> {
        CString::new(CONFIG_NAMESPACE)
            .map_err(|_| StoreError::Unsupported("NVS namespace contains NUL".into()))
    }

    fn key_c(&self) -> Result<CString, StoreError> {
        if !valid_name(&self.app_id) {
            return Err(StoreError::Unsupported(format!(
                "invalid NVS config key `{}`",
                self.app_id
            )));
        }
        CString::new(self.app_id.as_str())
            .map_err(|_| StoreError::Unsupported("NVS config key contains NUL".into()))
    }

    fn load(&self) -> Result<ConfigDoc, StoreError> {
        let namespace = self.namespace_c()?;
        let key = self.key_c()?;
        let mut buffer = vec![0_u8; CONFIG_BUFFER];
        let length = unsafe {
            micro_kv_get(
                namespace.as_ptr(),
                key.as_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            )
        };
        if length == 0 {
            return Ok(ConfigDoc::default());
        }
        if length < 0 {
            return Err(StoreError::Io(format!("NVS config read failed ({length})")));
        }
        let raw = std::str::from_utf8(&buffer[..length as usize])
            .map_err(|_| StoreError::Corrupt("NVS config is not UTF-8".into()))?;
        ConfigDoc::parse(raw)
    }

    fn save(&self, document: &ConfigDoc) -> Result<(), StoreError> {
        let namespace = self.namespace_c()?;
        let key = self.key_c()?;
        let json = document.to_json();
        if json.len() + 1 > CONFIG_BUFFER {
            return Err(StoreError::Full);
        }
        let value = CString::new(json)
            .map_err(|_| StoreError::Unsupported("config value contains NUL".into()))?;
        let result = unsafe { micro_kv_set(namespace.as_ptr(), key.as_ptr(), value.as_ptr()) };
        if result == 0 {
            Ok(())
        } else {
            Err(StoreError::Io(format!(
                "NVS config write failed ({result})"
            )))
        }
    }
}

#[cfg(target_os = "espidf")]
impl ScopedKv for NvsConfigKv {
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

#[cfg(test)]
mod tests {
    use super::{decode_value, encode_value, valid_name};
    use micro_store::KvValue;

    #[test]
    fn values_round_trip_with_their_type() {
        for value in [
            KvValue::Number(3.5),
            KvValue::String("abandon".into()),
            KvValue::Bool(true),
            KvValue::Bool(false),
            KvValue::Null,
        ] {
            let encoded = encode_value(&value).expect("encode");
            assert_eq!(decode_value(&encoded), Some(value.clone()), "{value:?}");
        }
    }

    #[test]
    fn non_finite_numbers_are_rejected() {
        assert!(encode_value(&KvValue::Number(f64::NAN)).is_none());
        assert!(encode_value(&KvValue::Number(f64::INFINITY)).is_none());
    }

    #[test]
    fn unknown_tags_are_rejected() {
        assert!(decode_value("x:1").is_none());
        assert!(decode_value("n:not-a-number").is_none());
        assert!(decode_value("b:2").is_none());
    }

    #[test]
    fn nvs_names_follow_the_component_limits() {
        assert!(valid_name("momo"));
        assert!(valid_name("s0"));
        assert!(!valid_name(""));
        assert!(!valid_name("0123456789abcdef"));
        assert!(!valid_name("bad\0key"));
    }
}
