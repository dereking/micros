use micro_store::{
    AppMeta, AppStore, KvStore, KvValue, Migration, NativeConfigStore, NativeStore, StoreError,
    migrate_legacy_root_from,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct TempDir(PathBuf);

impl TempDir {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn tempdir() -> TempDir {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "micro-store-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("tempdir");
    TempDir(path)
}

fn fresh_store() -> (TempDir, NativeStore) {
    let dir = tempdir();
    let store = NativeStore::new(dir.path());
    (dir, store)
}

fn counter_meta() -> AppMeta {
    AppMeta {
        id: "counter".to_owned(),
        name: "Counter".to_owned(),
        version: 1,
    }
}

#[test]
fn install_list_read_round_trip() {
    let (_dir, mut store) = fresh_store();
    store
        .install(counter_meta(), b"MBC1-app-bytes")
        .expect("install");

    let listed = store.list().expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "counter");
    assert_eq!(listed[0].name, "Counter");
    assert_eq!(listed[0].version, 1);

    assert_eq!(store.read("counter").expect("read"), b"MBC1-app-bytes");
}

#[test]
fn install_overwrites_existing_id() {
    let (_dir, mut store) = fresh_store();
    store.install(counter_meta(), b"one").expect("first");
    store.install(counter_meta(), b"two").expect("second");

    assert_eq!(store.list().expect("list").len(), 1);
    assert_eq!(store.read("counter").expect("read"), b"two");
}

#[test]
fn read_missing_app_is_not_found() {
    let (_dir, store) = fresh_store();
    assert_eq!(store.read("counter"), Err(StoreError::NotFound));
}

#[test]
fn uninstall_removes_blob_and_manifest_entry() {
    let (_dir, mut store) = fresh_store();
    store
        .install(counter_meta(), b"MBC1-app-bytes")
        .expect("install");
    store.uninstall("counter").expect("uninstall");

    assert!(store.list().expect("list").is_empty());
    assert_eq!(store.read("counter"), Err(StoreError::NotFound));
}

#[test]
fn uninstall_missing_app_is_not_found() {
    let (_dir, mut store) = fresh_store();
    assert_eq!(store.uninstall("ghost"), Err(StoreError::NotFound));
}

#[test]
fn app_store_persists_across_instances() {
    let dir = tempdir();
    {
        let mut store = NativeStore::new(dir.path());
        store
            .install(counter_meta(), b"MBC1-app-bytes")
            .expect("install");
    }
    let store = NativeStore::new(dir.path());
    assert_eq!(store.list().expect("list").len(), 1);
    assert_eq!(store.read("counter").expect("read"), b"MBC1-app-bytes");
}

#[test]
fn kv_round_trips_all_scalar_types() {
    let (_dir, store) = fresh_store();
    let mut kv = store.open("counter").expect("open");
    kv.set("count", &KvValue::Number(42.0)).expect("set count");
    kv.set("name", &KvValue::String("hi".to_owned()))
        .expect("set name");
    kv.set("ok", &KvValue::Bool(true)).expect("set ok");
    kv.set("nil", &KvValue::Null).expect("set nil");

    assert_eq!(kv.get("count"), Ok(Some(KvValue::Number(42.0))));
    assert_eq!(kv.get("name"), Ok(Some(KvValue::String("hi".to_owned()))));
    assert_eq!(kv.get("ok"), Ok(Some(KvValue::Bool(true))));
    assert_eq!(kv.get("nil"), Ok(Some(KvValue::Null)));
}

#[test]
fn kv_missing_key_is_none() {
    let (_dir, store) = fresh_store();
    let kv = store.open("counter").expect("open");
    assert_eq!(kv.get("nope"), Ok(None));
}

#[test]
fn kv_remove_deletes_the_key() {
    let (_dir, store) = fresh_store();
    let mut kv = store.open("counter").expect("open");
    kv.set("count", &KvValue::Number(1.0)).expect("set");
    kv.remove("count").expect("remove");
    assert_eq!(kv.get("count"), Ok(None));
}

#[test]
fn kv_namespaces_are_isolated() {
    let (_dir, store) = fresh_store();
    let mut alpha = store.open("alpha").expect("open alpha");
    let beta = store.open("beta").expect("open beta");
    alpha
        .set("shared", &KvValue::Number(1.0))
        .expect("set alpha");
    assert_eq!(beta.get("shared"), Ok(None));
}

#[test]
fn kv_persists_across_open() {
    let dir = tempdir();
    {
        let store = NativeStore::new(dir.path());
        let mut kv = store.open("counter").expect("open");
        kv.set("count", &KvValue::Number(42.0)).expect("set");
    }
    let store = NativeStore::new(dir.path());
    let kv = store.open("counter").expect("open");
    assert_eq!(kv.get("count"), Ok(Some(KvValue::Number(42.0))));
}

#[test]
fn invalid_identifiers_are_rejected() {
    let (_dir, mut store) = fresh_store();

    let evil = AppMeta {
        id: "../evil".to_owned(),
        name: "Evil".to_owned(),
        version: 1,
    };
    assert!(matches!(
        store.install(evil, b"bytes"),
        Err(StoreError::Unsupported(_))
    ));
    assert!(matches!(
        store.read("../evil"),
        Err(StoreError::Unsupported(_))
    ));
    assert!(matches!(store.open("a/b"), Err(StoreError::Unsupported(_))));
}

#[test]
fn empty_identifier_is_rejected() {
    let (_dir, store) = fresh_store();
    assert!(matches!(store.open(""), Err(StoreError::Unsupported(_))));
}

#[test]
fn non_finite_kv_number_is_rejected() {
    let (_dir, store) = fresh_store();
    let mut kv = store.open("ns").expect("open");
    assert!(matches!(
        kv.set("nan", &KvValue::Number(f64::NAN)),
        Err(StoreError::Unsupported(_))
    ));
}

#[test]
fn corrupt_manifest_is_reported_corrupt() {
    let dir = tempdir();
    fs::create_dir_all(dir.path().join("apps")).expect("mkdir");
    fs::write(dir.path().join("apps/manifest.json"), b"not json").expect("write");
    let store = NativeStore::new(dir.path());
    assert!(matches!(store.list(), Err(StoreError::Corrupt(_))));
}

#[test]
fn corrupt_kv_file_is_reported_corrupt() {
    let dir = tempdir();
    fs::create_dir_all(dir.path().join("kv")).expect("mkdir");
    fs::write(dir.path().join("kv/counter.json"), b"not json").expect("write");
    let store = NativeStore::new(dir.path());
    let kv = store.open("counter").expect("open");
    assert!(matches!(kv.get("k"), Err(StoreError::Corrupt(_))));
}

#[test]
fn config_file_lives_in_the_app_directory() {
    let (dir, _store) = fresh_store();
    let config = NativeConfigStore::new(dir.path());
    assert_eq!(
        config.app_dir("momo"),
        dir.path().join("momo"),
        "config is per-App: <root>/<app-id>"
    );

    let mut kv = config.open("momo").expect("open config");
    kv.set("maimemoToken", &KvValue::String("secret".into()))
        .expect("set token");
    kv.set("goal", &KvValue::Number(5.0)).expect("set goal");

    let raw = fs::read_to_string(dir.path().join("momo/config.json")).expect("config file");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("json");
    assert_eq!(parsed["maimemoToken"], "secret");
    assert_eq!(parsed["goal"], 5.0);
    assert!(
        raw.contains('\n'),
        "the native config file stays pretty-printed for hand editing"
    );
}

#[test]
fn config_persists_across_open_and_is_namespaced_by_app() {
    let dir = tempdir();
    {
        let store = NativeConfigStore::new(dir.path());
        let mut momo = store.open("momo").expect("open momo");
        momo.set("maimemoToken", &KvValue::String("abc".into()))
            .expect("set");
    }
    let store = NativeConfigStore::new(dir.path());
    let momo = store.open("momo").expect("open momo");
    let counter = store.open("counter").expect("open counter");
    assert_eq!(
        momo.get("maimemoToken"),
        Ok(Some(KvValue::String("abc".into())))
    );
    assert_eq!(counter.get("maimemoToken"), Ok(None));
}

#[test]
fn config_remove_and_missing_keys() {
    let dir = tempdir();
    let store = NativeConfigStore::new(dir.path());
    let mut config = store.open("momo").expect("open");
    assert_eq!(config.get("nope"), Ok(None));
    config
        .set("note", &KvValue::String("hi".into()))
        .expect("set");
    config.remove("note").expect("remove");
    assert_eq!(config.get("note"), Ok(None));
}

#[test]
fn config_rejects_escaping_app_ids_and_corrupt_json() {
    let dir = tempdir();
    let store = NativeConfigStore::new(dir.path());
    assert!(matches!(
        store.open("../evil"),
        Err(StoreError::Unsupported(_))
    ));
    assert!(matches!(store.open(""), Err(StoreError::Unsupported(_))));

    fs::create_dir_all(dir.path().join("momo")).expect("mkdir");
    fs::write(dir.path().join("momo/config.json"), b"not json").expect("write");
    let config = store.open("momo").expect("open");
    assert!(matches!(config.get("k"), Err(StoreError::Corrupt(_))));
}

#[test]
fn legacy_root_is_moved_into_the_current_root() {
    let dir = tempdir();
    let legacy = dir.path().join(".micrapp");
    let current = dir.path().join(".micros");
    fs::create_dir_all(legacy.join("kv")).expect("legacy kv");
    fs::create_dir_all(legacy.join("apps")).expect("legacy apps");
    fs::write(legacy.join("kv/momo.json"), br#"{"goal":3}"#).expect("write kv");
    fs::write(legacy.join("apps/manifest.json"), b"[]").expect("write manifest");

    assert_eq!(
        migrate_legacy_root_from(&legacy, &current).expect("migrate"),
        Migration::Moved
    );
    assert!(!legacy.exists(), "a moved tree does not stay behind");
    assert_eq!(
        fs::read_to_string(current.join("kv/momo.json")).expect("new kv"),
        r#"{"goal":3}"#
    );
    assert!(current.join("apps/manifest.json").is_file());
}

#[test]
fn legacy_root_merges_without_overwriting_the_current_install() {
    let dir = tempdir();
    let legacy = dir.path().join(".micrapp");
    let current = dir.path().join(".micros");
    fs::create_dir_all(legacy.join("kv")).expect("legacy kv");
    fs::create_dir_all(current.join("kv")).expect("current kv");
    fs::write(legacy.join("kv/momo.json"), br#"{"goal":3}"#).expect("legacy momo");
    fs::write(legacy.join("kv/counter.json"), br#"{"count":7}"#).expect("legacy counter");
    fs::write(current.join("kv/momo.json"), br#"{"goal":8}"#).expect("current momo");

    assert_eq!(
        migrate_legacy_root_from(&legacy, &current).expect("migrate"),
        Migration::Merged
    );
    assert_eq!(
        fs::read_to_string(current.join("kv/momo.json")).expect("new momo"),
        r#"{"goal":8}"#,
        "the new root wins"
    );
    assert_eq!(
        fs::read_to_string(current.join("kv/counter.json")).expect("new counter"),
        r#"{"count":7}"#,
        "missing files are filled in"
    );
    assert!(legacy.exists(), "the legacy tree stays as a backup");
}

#[test]
fn a_missing_legacy_root_is_a_no_op() {
    let dir = tempdir();
    let current = dir.path().join(".micros");
    assert_eq!(
        migrate_legacy_root_from(&dir.path().join(".micrapp"), &current).expect("migrate"),
        Migration::NotNeeded
    );
    assert!(!current.exists());
}
