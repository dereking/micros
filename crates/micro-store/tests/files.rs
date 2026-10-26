use std::path::PathBuf;

use micro_store::{FileStore, MAX_FILE_BYTES, NativeFileStore, StoreError};

fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "micros-files-{label}-{}-{}",
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
fn native_store_round_trips_files_and_directories() {
    let root = temp_root("round-trip");
    let store = NativeFileStore::new(root.join("sdcard"));
    let mut files = store.open("counter").expect("open");
    assert!(files.available());

    files.write_text("notes/today.txt", "hello").expect("write");
    assert_eq!(
        files.read_text("notes/today.txt").expect("read"),
        Some("hello".into())
    );
    files
        .append_text("notes/today.txt", " world")
        .expect("append");
    assert_eq!(files.size("notes/today.txt").expect("size"), Some(11));
    assert_eq!(
        files.list("").expect("list root"),
        vec!["notes/".to_owned()]
    );
    assert_eq!(
        files.list("notes").expect("list notes"),
        vec!["today.txt".to_owned()]
    );
    assert!(files.exists("notes").expect("exists notes"));
    assert!(files.exists("").expect("exists root"));
    assert_eq!(files.size("").expect("root size"), Some(0));

    files.mkdir("deep/nested").expect("mkdir");
    assert!(files.exists("deep/nested").expect("nested exists"));
    assert!(files.remove("notes").expect("remove"));
    assert!(!files.exists("notes").expect("removed"));
    assert!(!files.remove("notes").expect("remove missing"));
    assert_eq!(files.read_text("notes/today.txt").expect("missing"), None);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn native_store_isolates_apps() {
    let root = temp_root("isolation");
    let store = NativeFileStore::new(root.join("sdcard"));
    let mut a = store.open("app-a").expect("open a");
    let mut b = store.open("app-b").expect("open b");

    a.write_text("notes/today.txt", "a").expect("write a");
    b.write_text("notes/today.txt", "b").expect("write b");
    assert_eq!(a.read_text("notes/today.txt").unwrap(), Some("a".into()));
    assert_eq!(b.read_text("notes/today.txt").unwrap(), Some("b".into()));
    assert_eq!(
        a.read_text("../app-b/notes/today.txt"),
        Err(StoreError::Unsupported(
            "path traversal is not allowed".into()
        ))
    );
    assert!(
        root.join("sdcard/micros/apps/app-a/notes/today.txt")
            .is_file()
    );
    assert!(
        root.join("sdcard/micros/apps/app-b/notes/today.txt")
            .is_file()
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn native_store_rejects_oversized_files() {
    let root = temp_root("oversize");
    let store = NativeFileStore::new(root.join("sdcard"));
    let mut files = store.open("counter").expect("open");
    files
        .write_text("max.txt", &"x".repeat(MAX_FILE_BYTES))
        .expect("max size");
    assert!(matches!(
        files.write_text("too-big.txt", &"x".repeat(MAX_FILE_BYTES + 1)),
        Err(StoreError::Full)
    ));
    assert!(matches!(files.read_text("max.txt"), Ok(Some(text)) if text.len() == MAX_FILE_BYTES));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn invalid_app_ids_are_rejected_before_a_directory_is_created() {
    let root = temp_root("app-id");
    let store = NativeFileStore::new(root.join("sdcard"));
    for app_id in [
        "",
        "../escape",
        "a/b",
        "a b",
        ".hidden",
        "x".repeat(33).as_str(),
    ] {
        assert!(store.open(app_id).is_err(), "{app_id:?} should be rejected");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn native_store_rejects_symlink_escapes() {
    use std::os::unix::fs::symlink;

    let root = temp_root("symlink");
    let store = NativeFileStore::new(root.join("sdcard"));
    let mut a = store.open("app-a").expect("open a");
    let mut b = store.open("app-b").expect("open b");
    b.write_text("secret.txt", "secret").expect("write b");

    let app_a = root.join("sdcard/micros/apps/app-a");
    let app_b = root.join("sdcard/micros/apps/app-b");
    symlink(&app_b, app_a.join("other-app")).expect("dir symlink");
    symlink(app_b.join("secret.txt"), app_a.join("secret.txt")).expect("file symlink");
    symlink("/etc/passwd", app_a.join("host.txt")).expect("host symlink");

    assert!(a.read_text("other-app/secret.txt").is_err());
    assert!(a.write_text("other-app/pwn.txt", "pwn").is_err());
    assert!(a.list("other-app").is_err());
    assert!(a.exists("other-app").is_err());
    assert!(a.read_text("secret.txt").is_err());
    assert!(a.read_text("host.txt").is_err());
    assert_eq!(b.read_text("secret.txt").unwrap(), Some("secret".into()));

    let _ = std::fs::remove_dir_all(&root);
}
