use std::path::PathBuf;

use crate::RustusError;

/// Locate the rustus-scalus uber-JAR.
///
/// Search order:
/// 1. RUSTUS_JAR environment variable (explicit override)
/// 2. Relative to this crate's source dir (development)
/// 3. ~/.rustus/lib/rustus-scalus.jar (installed location)
pub fn find_jar() -> Result<PathBuf, RustusError> {
    // 1. Explicit env var
    if let Ok(jar_path) = std::env::var("RUSTUS_JAR") {
        let path = PathBuf::from(&jar_path);
        if path.exists() {
            return Ok(path);
        }
        return Err(RustusError::JarNotFound(format!(
            "RUSTUS_JAR set to '{jar_path}' but file does not exist"
        )));
    }

    // 2. Relative to crate manifest dir (development: ../scala-loader/loader/target/...)
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dev_candidates = [
        PathBuf::from(manifest_dir)
            .join("../scala-loader/loader/target/scala-3.3.7/rustus-scalus.jar"),
        PathBuf::from(manifest_dir)
            .join("../scala-loader/loader/target/scala-3.3.7/rustus-scala-loader-assembly-0.1.0-SNAPSHOT.jar"),
    ];
    for candidate in &dev_candidates {
        if candidate.exists() {
            return Ok(candidate.clone());
        }
    }

    // 3. Home directory
    if let Ok(home) = std::env::var("HOME") {
        let path = PathBuf::from(home).join(".rustus/lib/rustus-scalus.jar");
        if path.exists() {
            return Ok(path);
        }
    }

    Err(RustusError::JarNotFound(
        "rustus-scalus.jar not found. Build it with: cd scala-loader && sbt loader/assembly"
            .into(),
    ))
}
