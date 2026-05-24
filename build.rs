fn main() {
    let version = std::env::var("DEVM8_VERSION").unwrap_or_else(|_| {
        let pkg = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "unknown".into());
        format!("v{pkg}")
    });
    println!("cargo:rustc-env=DEVM8_VERSION={version}");
    println!("cargo:rerun-if-env-changed=DEVM8_VERSION");
}
