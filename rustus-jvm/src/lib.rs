mod jar;
mod jvm;

use std::sync::Arc;

use jni::objects::{GlobalRef, JString, JValueGen};
use jni::JavaVM;
use rustus_core::data::Data;
use rustus_core::module::Module;
use serde::Deserialize;

/// Error types for rustus-jvm operations.
#[derive(Debug, thiserror::Error)]
pub enum RustusError {
    #[error("JVM not found: {0}")]
    JvmNotFound(String),

    #[error("JVM initialization failed: {0}")]
    JvmInit(String),

    #[error("Scalus compilation error: {0}")]
    Compilation(String),

    #[error("CEK evaluation error: {0}")]
    Eval(String),

    #[error("JSON serialization error: {0}")]
    Serialization(String),

    #[error("uber-JAR not found: {0}")]
    JarNotFound(String),

    #[error("IO error: {0}")]
    Io(String),
}

/// Handle to the JVM running Scalus.
/// Created once and cached for the process lifetime.
pub struct ScalusVM {
    jvm: Arc<JavaVM>,
}

impl ScalusVM {
    /// Create a new JVM instance with the Scalus uber-JAR on the classpath.
    ///
    /// Locates libjvm via JAVA_HOME or `which java`.
    /// Locates the uber-JAR via RUSTUS_JAR env, development path, or ~/.rustus/lib/.
    pub fn new() -> Result<Self, RustusError> {
        jvm::find_libjvm()?; // validate JVM exists before attempting to create
        let jar_path = jar::find_jar()?;
        let jvm = jvm::create_jvm(&jar_path)?;
        Ok(ScalusVM { jvm: Arc::new(jvm) })
    }

    /// Compile a SIR Module to a Validator by calling Scalus via JNI.
    pub fn compile(&self, module: &Module) -> Result<Validator, RustusError> {
        let json = serde_json::to_string(module)
            .map_err(|e| RustusError::Serialization(e.to_string()))?;

        let mut env = self
            .jvm
            .attach_current_thread()
            .map_err(|e| RustusError::JvmInit(format!("Failed to attach thread: {e}")))?;

        let json_jstr = env
            .new_string(&json)
            .map_err(|e| RustusError::JvmInit(format!("Failed to create JVM string: {e}")))?;

        // Call: RustusLoader.compile(String) -> CompiledValidator
        let result = env
            .call_static_method(
                "rustus/loader/RustusLoader",
                "compile",
                "(Ljava/lang/String;)Lrustus/loader/CompiledValidator;",
                &[JValueGen::Object(&json_jstr)],
            )
            .map_err(|e| {
                // Try to extract exception message
                let msg = extract_exception_message(&mut env)
                    .unwrap_or_else(|| format!("JNI call failed: {e}"));
                RustusError::Compilation(msg)
            })?;

        let obj = result
            .l()
            .map_err(|e| RustusError::Compilation(format!("Expected object result: {e}")))?;

        let global_ref = env
            .new_global_ref(&obj)
            .map_err(|e| RustusError::JvmInit(format!("Failed to create global ref: {e}")))?;

        Ok(Validator {
            jvm: Arc::clone(&self.jvm),
            compiled: global_ref,
        })
    }
}

/// A compiled smart contract. Wraps a JVM-side CompiledValidator
/// holding an annotated UPLC Term with source location information.
pub struct Validator {
    jvm: Arc<JavaVM>,
    compiled: GlobalRef,
}

impl Validator {
    /// UPLC flat-encoded bytes for on-chain submission.
    pub fn to_flat(&self) -> Result<Vec<u8>, RustusError> {
        let mut env = self
            .jvm
            .attach_current_thread()
            .map_err(|e| RustusError::JvmInit(format!("Failed to attach thread: {e}")))?;

        let result = env
            .call_method(self.compiled.as_obj(), "toFlat", "()[B", &[])
            .map_err(|e| {
                let msg = extract_exception_message(&mut env)
                    .unwrap_or_else(|| format!("toFlat failed: {e}"));
                RustusError::Compilation(msg)
            })?;

        let jobj = result
            .l()
            .map_err(|e| RustusError::Compilation(format!("Expected byte array: {e}")))?;

        let byte_array = jni::objects::JByteArray::from(jobj);
        env.convert_byte_array(byte_array)
            .map_err(|e| RustusError::Compilation(format!("Failed to convert bytes: {e}")))
    }

    /// UPLC pretty-printed text for debugging/inspection.
    pub fn to_text(&self) -> Result<String, RustusError> {
        let mut env = self
            .jvm
            .attach_current_thread()
            .map_err(|e| RustusError::JvmInit(format!("Failed to attach thread: {e}")))?;

        let result = env
            .call_method(
                self.compiled.as_obj(),
                "toText",
                "()Ljava/lang/String;",
                &[],
            )
            .map_err(|e| {
                let msg = extract_exception_message(&mut env)
                    .unwrap_or_else(|| format!("toText failed: {e}"));
                RustusError::Compilation(msg)
            })?;

        let jstr = JString::from(result.l().map_err(|e| {
            RustusError::Compilation(format!("Expected string result: {e}"))
        })?);

        let text: String = env
            .get_string(&jstr)
            .map_err(|e| RustusError::Compilation(format!("Failed to get string: {e}")))?
            .into();

        Ok(text)
    }

    /// Execute in CEK machine with given Data arguments.
    /// Returns evaluation result with success/failure, budget costs, and logs.
    /// Errors include source locations mapped back to Rust source files.
    pub fn eval(&self, args: &[Data]) -> Result<EvalResult, RustusError> {
        let args_json =
            serde_json::to_string(args).map_err(|e| RustusError::Serialization(e.to_string()))?;

        let mut env = self
            .jvm
            .attach_current_thread()
            .map_err(|e| RustusError::JvmInit(format!("Failed to attach thread: {e}")))?;

        let args_jstr = env
            .new_string(&args_json)
            .map_err(|e| RustusError::JvmInit(format!("Failed to create JVM string: {e}")))?;

        let result = env
            .call_method(
                self.compiled.as_obj(),
                "eval",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValueGen::Object(&args_jstr)],
            )
            .map_err(|e| {
                let msg = extract_exception_message(&mut env)
                    .unwrap_or_else(|| format!("eval failed: {e}"));
                RustusError::Eval(msg)
            })?;

        let result_jstr = JString::from(result.l().map_err(|e| {
            RustusError::Eval(format!("Expected string result: {e}"))
        })?);

        let result_json: String = env
            .get_string(&result_jstr)
            .map_err(|e| RustusError::Eval(format!("Failed to get string: {e}")))?
            .into();

        serde_json::from_str::<EvalResult>(&result_json)
            .map_err(|e| RustusError::Eval(format!("Failed to parse eval result: {e}")))
    }

    /// Script hash (blake2b-224 of PlutusV3 script envelope).
    pub fn hash(&self) -> Result<[u8; 28], RustusError> {
        let mut env = self
            .jvm
            .attach_current_thread()
            .map_err(|e| RustusError::JvmInit(format!("Failed to attach thread: {e}")))?;

        let result = env
            .call_method(self.compiled.as_obj(), "scriptHash", "()[B", &[])
            .map_err(|e| {
                let msg = extract_exception_message(&mut env)
                    .unwrap_or_else(|| format!("scriptHash failed: {e}"));
                RustusError::Compilation(msg)
            })?;

        let jobj = result
            .l()
            .map_err(|e| RustusError::Compilation(format!("Expected byte array: {e}")))?;

        let byte_array = jni::objects::JByteArray::from(jobj);
        let bytes = env
            .convert_byte_array(byte_array)
            .map_err(|e| RustusError::Compilation(format!("Failed to convert bytes: {e}")))?;

        let mut out = [0u8; 28];
        if bytes.len() != 28 {
            return Err(RustusError::Compilation(format!(
                "Expected 28-byte hash, got {} bytes",
                bytes.len()
            )));
        }
        out.copy_from_slice(&bytes);
        Ok(out)
    }
}

/// Result of CEK machine evaluation.
#[derive(Debug, Clone, Deserialize)]
pub struct EvalResult {
    pub success: bool,
    pub cpu: u64,
    pub mem: u64,
    #[serde(default)]
    pub logs: Vec<String>,
    pub error: Option<String>,
}

impl EvalResult {
    pub fn succeeded(&self) -> bool {
        self.success
    }

    pub fn failed(&self) -> bool {
        !self.success
    }
}

/// Extract exception message from JVM, clearing the pending exception.
fn extract_exception_message(env: &mut jni::JNIEnv) -> Option<String> {
    if env.exception_check().ok()? {
        let exc = env.exception_occurred().ok()?;
        env.exception_clear().ok()?;
        let msg = env
            .call_method(&exc, "getMessage", "()Ljava/lang/String;", &[])
            .ok()?;
        let jstr = JString::from(msg.l().ok()?);
        let s: String = env.get_string(&jstr).ok()?.into();
        Some(s)
    } else {
        None
    }
}
