use std::fs;
use std::process::Command;

#[test]
fn cli_writes_mbc_and_reports_source_errors() {
    let directory = std::env::temp_dir().join(format!("microc-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    let valid = directory.join("valid.ts");
    let invalid = directory.join("invalid.ts");
    let output = directory.join("nested/app.mbc");
    fs::write(&valid, include_str!("../../../apps/counter/app.ts")).unwrap();
    fs::write(&invalid, "class App {}").unwrap();

    let success = Command::new(env!("CARGO_BIN_EXE_microc"))
        .args([&valid, &output])
        .output()
        .unwrap();
    assert!(
        success.status.success(),
        "{}",
        String::from_utf8_lossy(&success.stderr)
    );
    assert_eq!(&fs::read(&output).unwrap()[..4], b"MBC1");

    let failed_output = directory.join("failed.mbc");
    let failure = Command::new(env!("CARGO_BIN_EXE_microc"))
        .args([&invalid, &failed_output])
        .output()
        .unwrap();
    assert_eq!(failure.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&failure.stderr).contains("MTS001"));
    assert!(!failed_output.exists());

    fs::remove_dir_all(directory).unwrap();
}
