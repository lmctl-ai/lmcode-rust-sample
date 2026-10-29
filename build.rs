fn main() {
    println!("cargo:rerun-if-env-changed=SAMPLE_VERSION");
    let version = std::env::var("SAMPLE_VERSION").unwrap_or_else(|_| std::env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=SAMPLE_BUILD_VERSION={version}");
}
