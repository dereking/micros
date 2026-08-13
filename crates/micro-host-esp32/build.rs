fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_CMAKE_BUILD_LINK_LIBRARIES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("espidf") {
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=newlib");
    }
}
