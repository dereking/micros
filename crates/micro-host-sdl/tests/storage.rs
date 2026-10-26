//! Off-GUI checks for the native `storage.*` host calls. The real SD card path
//! is exercised by the ESP32 firmware; here the same `micro-store` sandbox is
//! backed by `<temp>/sdcard/micros/apps/<app>`.

use micro_host_sdl::host::NativeHost;
use micro_ir::{HostCallKind, HostRequest, ScalarType};
use micro_vm::{HostAccess, Value};

fn request(kind: HostCallKind, arg_kinds: Vec<ScalarType>) -> HostRequest {
    HostRequest::sync(kind, arg_kinds, None)
}

fn text(value: &str) -> Value {
    Value::String(value.to_owned())
}

fn temp_root() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "micros-sdl-storage-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

#[test]
fn native_storage_round_trips_and_isolates_apps() {
    let root = temp_root();
    let mut a = NativeHost::new_for_app_at(&root, "app-a");
    let mut b = NativeHost::new_for_app_at(&root, "app-b");

    let available = a
        .call(&request(HostCallKind::StorageAvailable, vec![]), &[])
        .expect("available");
    assert_eq!(available, Some(Value::Bool(true)));

    let write = a
        .call(
            &request(
                HostCallKind::StorageWriteText,
                vec![ScalarType::String, ScalarType::String],
            ),
            &[text("notes/today.txt"), text("hello")],
        )
        .expect("write");
    assert_eq!(write, Some(Value::Bool(true)));
    let append = a
        .call(
            &request(
                HostCallKind::StorageAppendText,
                vec![ScalarType::String, ScalarType::String],
            ),
            &[text("notes/today.txt"), text(" world")],
        )
        .expect("append");
    assert_eq!(append, Some(Value::Bool(true)));

    let read = a
        .call(
            &request(HostCallKind::StorageReadText, vec![ScalarType::String]),
            &[text("notes/today.txt")],
        )
        .expect("read");
    assert_eq!(read, Some(Value::String("hello world".into())));
    let size = a
        .call(
            &request(HostCallKind::StorageSize, vec![ScalarType::String]),
            &[text("notes/today.txt")],
        )
        .expect("size");
    assert_eq!(size, Some(Value::Number(11.0)));

    let list = a
        .call(
            &request(HostCallKind::StorageList, vec![ScalarType::String]),
            &[text("")],
        )
        .expect("list");
    assert_eq!(list, Some(Value::String("notes/".into())));

    // The same relative path in another App is a different file.
    let other_read = b
        .call(
            &request(HostCallKind::StorageReadText, vec![ScalarType::String]),
            &[text("notes/today.txt")],
        )
        .expect("cross-app read");
    assert_eq!(other_read, Some(Value::String(String::new())));
    let other_exists = b
        .call(
            &request(HostCallKind::StorageExists, vec![ScalarType::String]),
            &[text("notes/today.txt")],
        )
        .expect("cross-app exists");
    assert_eq!(other_exists, Some(Value::Bool(false)));

    // Traversal is rejected before it reaches the filesystem.
    let escape = a
        .call(
            &request(
                HostCallKind::StorageWriteText,
                vec![ScalarType::String, ScalarType::String],
            ),
            &[text("../app-b/secret.txt"), text("pwn")],
        )
        .expect("escape write");
    assert_eq!(escape, Some(Value::Bool(false)));
    assert!(!root.join("sdcard/micros/apps/app-b/secret.txt").exists());

    let remove = a
        .call(
            &request(HostCallKind::StorageRemove, vec![ScalarType::String]),
            &[text("notes")],
        )
        .expect("remove");
    assert_eq!(remove, Some(Value::Bool(true)));
    let gone = a
        .call(
            &request(HostCallKind::StorageExists, vec![ScalarType::String]),
            &[text("notes")],
        )
        .expect("exists after remove");
    assert_eq!(gone, Some(Value::Bool(false)));

    let _ = std::fs::remove_dir_all(&root);
}
