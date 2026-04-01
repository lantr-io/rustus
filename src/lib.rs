pub use rustus_core::*;

// Re-export proc macros so users can write:
//   #[rustus::compile]        instead of #[rustus_macros::compile]
//   #[rustus::module("...")]  instead of #[rustus_macros::rustus_module("...")]
//   #[derive(rustus::ToData)] instead of #[derive(rustus_macros::ToData)]
pub use rustus_macros::compile;
pub use rustus_macros::rustus_module as module;
pub use rustus_macros::FromData;
pub use rustus_macros::ToData;
pub use rustus_macros::functional_typeclass;

// JVM integration: Scalus compilation and CEK evaluation
pub use rustus_jvm::{EvalResult, RustusError, ScalusVM, Validator};

use std::sync::{Arc, Mutex, OnceLock};

static SCALUS_VM: OnceLock<Arc<ScalusVM>> = OnceLock::new();
static SCALUS_VM_INIT: Mutex<()> = Mutex::new(());

fn scalus_vm() -> Result<&'static Arc<ScalusVM>, RustusError> {
    if let Some(vm) = SCALUS_VM.get() {
        return Ok(vm);
    }
    let _lock = SCALUS_VM_INIT.lock().unwrap();
    if let Some(vm) = SCALUS_VM.get() {
        return Ok(vm);
    }
    let vm = Arc::new(ScalusVM::new()?);
    SCALUS_VM.set(vm).ok();
    Ok(SCALUS_VM.get().unwrap())
}

/// Compile a named module to a Validator.
/// Lazily initializes the JVM on first call.
/// Requires JAVA_HOME or java on PATH, and the rustus-scalus uber-JAR.
pub fn compile_module(name: &str) -> Result<Validator, RustusError> {
    compile_module_with_options(name, rustus_core::module::CompilerOptions::default())
}

/// Compile a named module with custom compiler options.
pub fn compile_module_with_options(
    name: &str,
    options: rustus_core::module::CompilerOptions,
) -> Result<Validator, RustusError> {
    let mut module = rustus_core::registry::build_module(name);
    // Verify no Unresolved types remain — catch bugs before sending to scalus
    for binding in &module.defs {
        if let Err(errors) = rustus_core::typing::verify_complete(&binding.value) {
            let msgs: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
            return Err(RustusError::Compilation(format!(
                "Unresolved types in {}: {}",
                binding.name,
                msgs.join(", ")
            )));
        }
    }
    module.options = options;
    let vm = scalus_vm()?;
    vm.compile(&module)
}

/// Compile a named module and write UPLC flat-encoded bytes to a file.
/// For production deployment without JVM.
pub fn compile_to_file(
    name: &str,
    path: &std::path::Path,
) -> Result<(), RustusError> {
    let validator = compile_module(name)?;
    let flat = validator.to_flat()?;
    std::fs::write(path, &flat).map_err(|e| RustusError::Io(e.to_string()))?;
    Ok(())
}
