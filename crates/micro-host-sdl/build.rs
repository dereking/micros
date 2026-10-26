use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_NATIVE");
    println!("cargo:rerun-if-env-changed=MICRO_NATIVE_SDL3_SOURCE");
    println!("cargo:rerun-if-env-changed=MICRO_NATIVE_LVGL_SOURCE");
    println!("cargo:rerun-if-env-changed=MICRO_NATIVE_OFFLINE");
    if std::env::var_os("CARGO_FEATURE_NATIVE").is_none() {
        return;
    }
    println!("cargo:rerun-if-changed=../../native");
    println!("cargo:rerun-if-changed=../../assets/fonts/lvgl");
    println!("cargo:rerun-if-changed=../../assets/app-icon/icon-512.png");
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let dependency_cache = manifest.join("../../target/native-deps");
    let mut configure = cmake::Config::new("../../native");
    configure
        .define("CMAKE_BUILD_TYPE", "Release")
        .define("FETCHCONTENT_BASE_DIR", dependency_cache);
    // Offline / mirrored builds can point CMake at already-cloned sources
    // instead of letting FetchContent reach GitHub.
    if let Some(source) = std::env::var_os("MICRO_NATIVE_SDL3_SOURCE") {
        configure.define("FETCHCONTENT_SOURCE_DIR_SDL3", source);
    }
    if let Some(source) = std::env::var_os("MICRO_NATIVE_LVGL_SOURCE") {
        configure.define("FETCHCONTENT_SOURCE_DIR_LVGL", source);
    }
    if std::env::var("MICRO_NATIVE_OFFLINE").as_deref() == Ok("1") {
        configure.define("FETCHCONTENT_FULLY_DISCONNECTED", "ON");
    }
    let destination = configure.build();
    let library_directory = destination.join("lib");
    println!(
        "cargo:rustc-link-search=native={}",
        library_directory.display()
    );
    println!("cargo:rustc-link-lib=dylib=micro_native");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        copy_windows_runtime(&destination, &library_directory);
    } else {
        println!(
            "cargo:rustc-link-arg=-Wl,-rpath,{}",
            library_directory.display()
        );
    }
}

/// CMake installs the DLL under `lib`/`bin`; the Windows loader does not use
/// rpath, so stage it beside the Cargo executable and test harnesses.
fn copy_windows_runtime(destination: &Path, library_directory: &Path) {
    let mut dll = None;
    for candidate in [
        library_directory.join("micro_native.dll"),
        destination.join("bin").join("micro_native.dll"),
        destination.join("micro_native.dll"),
    ] {
        if candidate.is_file() {
            dll = Some(candidate);
            break;
        }
    }
    let Some(dll) = dll else {
        println!("cargo:warning=micro_native.dll was not produced by CMake");
        return;
    };
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let Some(profile_dir) = out_dir.ancestors().nth(3) else {
        println!("cargo:warning=could not locate Cargo profile directory");
        return;
    };
    for target in [profile_dir.to_path_buf(), profile_dir.join("deps")] {
        if let Err(error) = std::fs::create_dir_all(&target)
            .and_then(|()| std::fs::copy(&dll, target.join("micro_native.dll")))
        {
            println!(
                "cargo:warning=could not stage {} into {}: {error}",
                dll.display(),
                target.display()
            );
        }
    }
}
