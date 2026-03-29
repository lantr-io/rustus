use std::path::PathBuf;

use crate::RustusError;

/// Locate libjvm shared library on the system.
///
/// Search order:
/// 1. JAVA_HOME environment variable
/// 2. `which java` → resolve symlinks → derive JAVA_HOME
pub fn find_libjvm() -> Result<PathBuf, RustusError> {
    // 1. Check JAVA_HOME
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let path = PathBuf::from(&java_home);
        if let Some(libjvm) = probe_java_home(&path) {
            return Ok(libjvm);
        }
    }

    // 2. Try `which java` and resolve symlinks
    if let Ok(output) = std::process::Command::new("which").arg("java").output() {
        if output.status.success() {
            let java_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(resolved) = std::fs::canonicalize(&java_path) {
                // java is typically at $JAVA_HOME/bin/java
                if let Some(bin_dir) = resolved.parent() {
                    if let Some(java_home) = bin_dir.parent() {
                        if let Some(libjvm) = probe_java_home(java_home) {
                            return Ok(libjvm);
                        }
                    }
                }
            }
        }
    }

    Err(RustusError::JvmNotFound(
        "JDK 11+ required. Set JAVA_HOME or install from https://adoptium.net".into(),
    ))
}

/// Probe a JAVA_HOME directory for libjvm in known locations.
fn probe_java_home(java_home: &std::path::Path) -> Option<PathBuf> {
    let candidates = [
        // Linux
        java_home.join("lib/server/libjvm.so"),
        // macOS
        java_home.join("lib/server/libjvm.dylib"),
        // Windows
        java_home.join("bin/server/jvm.dll"),
        // Some JDK layouts
        java_home.join("jre/lib/server/libjvm.so"),
        java_home.join("jre/lib/amd64/server/libjvm.so"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return Some(candidate.clone());
        }
    }
    None
}

/// Create a JVM instance with the given classpath.
pub fn create_jvm(classpath: &std::path::Path) -> Result<jni::JavaVM, RustusError> {
    let classpath_str = classpath
        .to_str()
        .ok_or_else(|| RustusError::JvmInit("Invalid classpath encoding".into()))?;

    let cp_option = format!("-Djava.class.path={classpath_str}");
    let jvm_args = jni::InitArgsBuilder::new()
        .version(jni::JNIVersion::V8)
        .option(&cp_option)
        .option("-Xmx512m")
        .build()
        .map_err(|e| RustusError::JvmInit(format!("Failed to build JVM args: {e}")))?;

    jni::JavaVM::new(jvm_args)
        .map_err(|e| RustusError::JvmInit(format!("Failed to create JVM: {e}")))
}
