//! Reads `scalusVersion` from scala-loader/build.sbt into `RUSTUS_SCALUS_VERSION`, so that
//! `ScalusVM` can refuse a loader JAR built against another Scalus version. build.sbt stays the
//! only place the version is written. Outside this repository there is no build.sbt, the
//! variable is not set, and the JAR is not checked.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo");
    let build_sbt = PathBuf::from(manifest_dir).join("../scala-loader/build.sbt");
    let Ok(text) = std::fs::read_to_string(&build_sbt) else {
        return;
    };
    println!("cargo:rerun-if-changed={}", build_sbt.display());
    match scalus_version(&text) {
        Some(version) => println!("cargo:rustc-env=RUSTUS_SCALUS_VERSION={version}"),
        None => println!(
            "cargo:warning=no `val scalusVersion = \"...\"` in {}; the loader JAR's Scalus version will not be checked",
            build_sbt.display()
        ),
    }
}

/// The value of `val scalusVersion = "<version>"`.
fn scalus_version(build_sbt: &str) -> Option<&str> {
    build_sbt.lines().find_map(|line| {
        let value = line
            .trim()
            .strip_prefix("val scalusVersion")?
            .trim_start()
            .strip_prefix('=')?
            .trim_start()
            .strip_prefix('"')?;
        value.split('"').next()
    })
}
