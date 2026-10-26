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
    println!(
        "cargo:rustc-link-arg=-Wl,-rpath,{}",
        library_directory.display()
    );
}
