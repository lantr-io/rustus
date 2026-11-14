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

    // 2. Relative to crate manifest dir (development). build.sbt writes the JAR outside
    //    target/scala-<version>/, so this path does not change with the Scala version.
    let dev_jar = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../scala-loader/loader/target/rustus-scalus.jar");
    if dev_jar.exists() {
        return Ok(dev_jar);
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
