//! Reads what the loader JAR must have been built from, so that `ScalusVM` can refuse a stale one:
//!
//! - `RUSTUS_SCALUS_VERSION`: `scalusVersion` from scala-loader/build.sbt, the only place the
//!   version is written;
//! - `RUSTUS_LOADER_FINGERPRINT`: a hash of build.sbt and scala-loader/loader/src/, which
//!   build.sbt also writes into the JAR (scala-loader/project/LoaderFingerprint.scala).
//!
//! Outside this repository there is no scala-loader/, neither variable is set, and the JAR is
//! not checked.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo");
    let scala_loader = PathBuf::from(manifest_dir).join("../scala-loader");
    let build_sbt = scala_loader.join("build.sbt");
    let Ok(text) = std::fs::read_to_string(&build_sbt) else {
        return;
    };
    println!("cargo:rerun-if-changed={}", build_sbt.display());
    println!("cargo:rerun-if-changed={}", scala_loader.join("loader/src").display());

    match scalus_version(&text) {
        Some(version) => println!("cargo:rustc-env=RUSTUS_SCALUS_VERSION={version}"),
        None => println!(
            "cargo:warning=no `val scalusVersion = \"...\"` in {}; the loader JAR's Scalus version will not be checked",
            build_sbt.display()
        ),
    }
    match loader_fingerprint(&scala_loader) {
        Ok(fingerprint) => println!("cargo:rustc-env=RUSTUS_LOADER_FINGERPRINT={fingerprint}"),
        Err(e) => println!(
            "cargo:warning=cannot hash {}: {e}; the loader JAR's sources will not be checked",
            scala_loader.display()
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

/// The hash scala-loader/project/LoaderFingerprint.scala computes; see there for the definition.
/// The two must agree exactly.
fn loader_fingerprint(scala_loader: &Path) -> std::io::Result<String> {
    let mut names = vec!["build.sbt".to_string()];
    collect_files(scala_loader, "loader/src", &mut names)?;
    names.retain(|name| {
        !name
            .split('/')
            .any(|c| c.starts_with('.') || c.ends_with('~'))
    });
    names.sort();

    let mut hasher = Sha256::new();
    for name in &names {
        let content = std::fs::read(scala_loader.join(name))?;
        hasher.update(name.as_bytes());
        hasher.update(format!("\n{}\n", content.len()).as_bytes());
        hasher.update(&content);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Adds to `names` every regular file under `scala_loader/dir`, named relative to `scala_loader`
/// with `/` separators. Like Java's `Files.walk` + `isRegularFile`: a link to a file counts, a
/// link to a directory is not followed.
fn collect_files(scala_loader: &Path, dir: &str, names: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(scala_loader.join(dir))? {
        let entry = entry?;
        let name = format!("{dir}/{}", entry.file_name().to_string_lossy());
        if entry.file_type()?.is_dir() {
            collect_files(scala_loader, &name, names)?;
        } else if std::fs::metadata(entry.path()).is_ok_and(|m| m.is_file()) {
            names.push(name);
        }
    }
    Ok(())
}
