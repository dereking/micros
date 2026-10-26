//! Native launcher registry: simulated `/sdcard/apps` discovery and the
//! built-in-wins id collision policy.

#![cfg(feature = "native")]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use micro_host_sdl::external_apps::{
    InstalledApp, MAX_LAUNCHER_APPS, discover_external, external_app_dir, load_app, merge_registry,
};
use micro_ir::{decode, encode};

fn counter_bytes() -> Vec<u8> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/counter/dist/counter.mbc");
    std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "{} is missing; run `npm run build:app` first",
            path.display()
        )
    })
}

fn unique_dir(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("micros-{label}-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rewrite(bytes: &[u8], id: &str, name: &str) -> Vec<u8> {
    let mut image = decode(bytes).expect("fixture MBC");
    image.metadata.id = id.to_owned();
    image.metadata.name = name.to_owned();
    encode(&image).expect("encode fixture MBC")
}

fn write_app(dir: &Path, file: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(file);
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn simulated_card_is_below_the_native_store_root() {
    let dir = external_app_dir();
    assert!(dir.ends_with(Path::new("sdcard").join("apps").as_path()));
}

#[test]
fn missing_card_directory_is_ignored() {
    let dir = unique_dir("missing-card");
    let absent = dir.join("not-created");
    assert!(discover_external(&absent).is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn external_apps_are_listed_after_builtins() {
    let dir = unique_dir("external-list");
    let counter = counter_bytes();
    let builtin_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/counter/dist/counter.mbc");
    let builtin = load_app(&builtin_path, false).unwrap();
    write_app(&dir, "zeta.mbc", &rewrite(&counter, "zeta", "Zeta App"));
    write_app(&dir, "alpha.mbc", &rewrite(&counter, "alpha", "Alpha App"));

    let external = discover_external(&dir);
    assert_eq!(
        external
            .iter()
            .map(|app| app.id.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "zeta"],
        "external files are scanned in file-name order"
    );
    assert!(external.iter().all(|app| app.external));

    let merged = merge_registry(vec![builtin], external);
    assert_eq!(
        merged.iter().map(|app| app.id.as_str()).collect::<Vec<_>>(),
        ["counter", "alpha", "zeta"]
    );
    assert!(!merged[0].external);
    assert!(merged[1..].iter().all(|app| app.external));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn builtin_wins_an_id_collision() {
    let dir = unique_dir("builtin-wins");
    let counter = counter_bytes();
    let builtin_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/counter/dist/counter.mbc");
    let builtin = load_app(&builtin_path, false).unwrap();
    write_app(
        &dir,
        "sd-counter.mbc",
        &rewrite(&counter, "COUNTER", "Fake Counter"),
    );

    let merged = merge_registry(vec![builtin], discover_external(&dir));
    assert_eq!(merged.len(), 1, "the SD copy is skipped");
    assert_eq!(merged[0].id, "counter");
    assert!(!merged[0].external);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn reserved_shell_id_is_never_listed() {
    let shell = InstalledApp {
        id: "shell".into(),
        name: "Fake Home".into(),
        icon: "apps".into(),
        path: PathBuf::from("/tmp/shell.mbc"),
        external: true,
    };
    assert!(merge_registry(Vec::new(), vec![shell]).is_empty());
}

#[test]
fn duplicate_external_ids_keep_the_first_file() {
    let dir = unique_dir("external-duplicate");
    let counter = counter_bytes();
    write_app(&dir, "a.mbc", &rewrite(&counter, "same", "Same A"));
    write_app(&dir, "b.mbc", &rewrite(&counter, "same", "Same B"));

    let merged = merge_registry(Vec::new(), discover_external(&dir));
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].name, "Same A");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unsafe_external_ids_are_skipped() {
    let dir = unique_dir("unsafe-id");
    let counter = counter_bytes();
    write_app(
        &dir,
        "unsafe.mbc",
        &rewrite(&counter, "../escape", "Unsafe"),
    );
    assert!(discover_external(&dir).is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unusable_mbc_files_are_skipped() {
    let dir = unique_dir("invalid");
    write_app(&dir, "broken.mbc", b"not an MBC");
    write_app(&dir, "notes.txt", b"ignored extension");
    assert!(discover_external(&dir).is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn launcher_cap_is_enforced() {
    let dir = unique_dir("cap");
    let counter = counter_bytes();
    for index in 0..=MAX_LAUNCHER_APPS {
        let id = format!("external{index}");
        write_app(
            &dir,
            &format!("{id}.mbc"),
            &rewrite(&counter, &id, &format!("External {index}")),
        );
    }
    let external: Vec<InstalledApp> = discover_external(&dir);
    assert_eq!(external.len(), MAX_LAUNCHER_APPS + 1);
    let merged = merge_registry(Vec::new(), external);
    assert_eq!(merged.len(), MAX_LAUNCHER_APPS);
    std::fs::remove_dir_all(&dir).unwrap();
}
