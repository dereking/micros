use micro_store::{AppMeta, AppStore, KvStore, KvValue, NativeStore, StoreError};
use std::fs;
use tempfile::TempDir;

fn fresh_store() -> (TempDir, NativeStore) {
    let dir = tempfile::tempdir().expect("tempdir");
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
    let dir = tempfile::tempdir().expect("tempdir");
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
    let dir = tempfile::tempdir().expect("tempdir");
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
    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("apps")).expect("mkdir");
    fs::write(dir.path().join("apps/manifest.json"), b"not json").expect("write");
    let store = NativeStore::new(dir.path());
    assert!(matches!(store.list(), Err(StoreError::Corrupt(_))));
}

#[test]
fn corrupt_kv_file_is_reported_corrupt() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("kv")).expect("mkdir");
    fs::write(dir.path().join("kv/counter.json"), b"not json").expect("write");
    let store = NativeStore::new(dir.path());
    let kv = store.open("counter").expect("open");
    assert!(matches!(kv.get("k"), Err(StoreError::Corrupt(_))));
}
