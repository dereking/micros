fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_NATIVE");
    if std::env::var_os("CARGO_FEATURE_NATIVE").is_none() {
        return;
    }
    println!("cargo:rerun-if-changed=../../native");
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let dependency_cache = manifest.join("../../target/native-deps");
    let destination = cmake::Config::new("../../native")
        .define("CMAKE_BUILD_TYPE", "Release")
        .define("FETCHCONTENT_BASE_DIR", dependency_cache)
        .build();
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
