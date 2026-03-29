# JVM Integration Design: rustus-jvm

## Overview

Add a `rustus-jvm` crate that bridges Rust and the JVM to provide compilation (SIR -> UPLC)
and evaluation (CEK machine) by calling Scalus directly via JNI. This eliminates the current
two-step workflow (Rust generates JSON file, then a separate `sbt run` consumes it) and gives
users a single Rust API: `rustus::compile() -> Validator`.

## Architecture

```
User's Rust code
    |
    | #[compile] macro (macro expansion time)
    v
Pre-SIR + TypeDict (registered via inventory)
    |
    | build_module() (runtime)
    v
SIR Module (rustus-core)
    |
    | serialize to JSON string
    v
rustus-jvm: JNI call to Scalus
    |
    | RustusLoader.compile(sirJson) on JVM
    v
PlutusV3 object (lives on JVM heap)
    |
    | wrapped as Validator (Rust side holds GlobalRef)
    v
Validator
    |--- .to_flat()  -> Vec<u8>      (UPLC flat-encoded, for on-chain)
    |--- .to_text()  -> String        (UPLC pretty-printed, for debugging)
    |--- .eval(args) -> EvalResult    (CEK machine execution with source-mapped errors)
    |--- .hash()     -> [u8; 28]      (script hash)
```

## Crate Structure

```
rustus/                      # facade crate (what external users depend on)
  src/lib.rs                 # re-exports + compile() / eval() convenience API
  Cargo.toml                 # depends on rustus-core, rustus-macros, rustus-prelude, rustus-jvm

rustus-core/                 # IR types, registry, lowering (unchanged)
rustus-macros/               # proc macros (unchanged)
rustus-prelude/              # standard library (unchanged)

rustus-jvm/                  # NEW: JVM integration
  Cargo.toml                 # depends on jni crate, rustus-core
  src/
    lib.rs                   # public API: ScalusVM, Validator, EvalResult
    jvm.rs                   # JVM lifecycle (find libjvm, create/cache JavaVM)
    jar.rs                   # uber-JAR extraction and classpath management

scala-loader/                # MODIFIED: add uber-JAR build + RustusLoader entry point
  loader/src/main/scala/rustus/loader/
    RustusLoader.scala       # NEW: @JvmStatic compile/eval methods for JNI
    Main.scala               # existing CLI entry point (kept for standalone use)
    RustusJsonCodec.scala    # unchanged
    RustusToScalus.scala     # unchanged
  build.sbt                  # add sbt-assembly plugin for uber-JAR
```

## Rust API

### `rustus-jvm` crate

```rust
// rustus-jvm/src/lib.rs

use rustus_core::module::Module;
use rustus_core::data::Data;

/// Handle to the JVM running Scalus.
/// Created lazily on first use, cached for the process lifetime.
pub struct ScalusVM {
    jvm: jni::JavaVM,
}

impl ScalusVM {
    /// Create a new JVM instance.
    /// Locates libjvm via JAVA_HOME or `which java`, sets classpath to the uber-JAR.
    pub fn new() -> Result<Self, RustusError> { ... }
}

/// A compiled smart contract. Wraps a PlutusV3 object on the JVM.
/// Provides access to UPLC in various formats and CEK evaluation.
pub struct Validator {
    vm: Arc<ScalusVM>,
    compiled: jni::objects::GlobalRef,  // reference to PlutusV3[Any] on JVM
}

impl Validator {
    /// UPLC flat-encoded bytes for on-chain submission.
    pub fn to_flat(&self) -> Result<Vec<u8>, RustusError> { ... }

    /// UPLC pretty-printed text for debugging/inspection.
    pub fn to_text(&self) -> Result<String, RustusError> { ... }

    /// Execute in CEK machine with given arguments.
    /// Errors include source locations mapped back to Rust source files.
    pub fn eval(&self, args: &[Data]) -> Result<EvalResult, RustusError> { ... }

    /// Script hash (blake2b-224 of serialized script envelope).
    pub fn hash(&self) -> Result<[u8; 28], RustusError> { ... }
}

/// Result of CEK machine evaluation.
pub struct EvalResult {
    pub success: bool,
    pub cpu: u64,
    pub mem: u64,
    pub logs: Vec<String>,
    pub error: Option<EvalError>,
}

pub struct EvalError {
    pub message: String,
    pub source_location: Option<SourceLoc>,
}

pub struct SourceLoc {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug)]
pub enum RustusError {
    /// JAVA_HOME not set and java not found on PATH
    JvmNotFound(String),
    /// JVM initialization failed
    JvmInit(String),
    /// Scalus compilation error
    Compilation(String),
    /// CEK evaluation error (not a script failure — an infra error)
    Eval(String),
    /// JSON serialization error
    Serialization(String),
}
```

### `rustus` facade crate (updated)

```rust
// rustus/src/lib.rs

pub use rustus_macros::{compile, FromData, ToData};
pub use rustus_macros::rustus_module as module;
pub use rustus_core::*;
pub use rustus_jvm::{Validator, EvalResult, EvalError, SourceLoc, RustusError};

use std::sync::OnceLock;

static SCALUS_VM: OnceLock<Arc<rustus_jvm::ScalusVM>> = OnceLock::new();

fn scalus_vm() -> Result<&Arc<rustus_jvm::ScalusVM>, RustusError> {
    SCALUS_VM.get_or_try_init(|| {
        rustus_jvm::ScalusVM::new().map(Arc::new)
    })
}

/// Compile a named validator to a Validator object.
/// Requires JAVA_HOME or java on PATH.
pub fn compile(name: &str) -> Result<Validator, RustusError> {
    let module = rustus_core::registry::build_module(name);
    let vm = scalus_vm()?.clone();
    vm.compile(&module)
}

/// Compile and write UPLC flat-encoded bytes to a file.
/// For production deployment without JVM.
pub fn compile_to_file(name: &str, path: &std::path::Path) -> Result<(), RustusError> {
    let validator = compile(name)?;
    let flat = validator.to_flat()?;
    std::fs::write(path, &flat).map_err(|e| RustusError::Io(e.to_string()))?;
    Ok(())
}
```

## Scala Side

### `RustusLoader.scala` (new file)

```scala
package rustus.loader

import com.github.plokhotnyuk.jsoniter_scala.core.*
import rustus.loader.RustusJsonCodec.*
import scalus.compiler.sir.*
import scalus.compiler.sir.linking.{SIRLinker, SIRLinkerOptions}
import scalus.compiler.sir.lowering.SirToUplcV3Lowering
import scalus.uplc.*
import scalus.uplc.eval.*

/** JNI entry point for rustus-jvm crate.
  * Each method is called via JNI from Rust.
  */
object RustusLoader {

  /** Compile SIR JSON string to a PlutusV3 object.
    * Returns a reference that Rust holds as a GlobalRef.
    * Called from: ScalusVM::compile()
    */
  @JvmStatic
  def compile(sirJson: String): CompiledValidator = {
    val rmodule = readFromArray[RModule](sirJson.getBytes("UTF-8"))
    val result = RustusToScalus.transform(rmodule)
    val module = result.module

    result.mainBinding match {
      case Some(binding) =>
        val supportModules: Map[String, Module] =
          module.defs
            .filter(_.name != binding.name)
            .groupBy(b =>
              rmodule.defs.find(_.name == b.name).flatMap(_.module_name).getOrElse(module.name)
            )
            .map { (modName, bindings) =>
              modName -> Module(
                version = module.version,
                name = modName,
                linked = false,
                requireBackend = None,
                defs = bindings
              )
            }

        val linkerOptions = SIRLinkerOptions(
          useUniversalDataConversion = true,
          printErrors = true,
          debugLevel = 0
        )
        val linker = new SIRLinker(linkerOptions, supportModules)
        val linkedSir = linker.link(binding.value, SIRPosition.empty)
        val lowering = SirToUplcV3Lowering(linkedSir, generateErrorTraces = true)
        val term = lowering.lower()
        CompiledValidator(term)

      case None =>
        throw new RuntimeException("No main binding found in module")
    }
  }
}

/** Holds a compiled UPLC term with annotations for source mapping.
  * Rust holds a GlobalRef to instances of this class.
  */
class CompiledValidator(val term: Term) {

  /** UPLC flat-encoded bytes (strips annotations). */
  def toFlat: Array[Byte] = {
    val program = Program.plutusV3(term)
    program.flatEncoded
  }

  /** UPLC pretty-printed with annotations. */
  def toText: String = {
    term.pretty.render(120)
  }

  /** Evaluate with Data arguments. Returns JSON-encoded EvalResult. */
  def eval(args: Array[Array[Byte]]): String = {
    import scalus.uplc.TermDSL.given
    import com.github.plokhotnyuk.jsoniter_scala.core.*

    // Decode Data arguments from CBOR or flat
    val dataArgs = args.map(bytes => scalus.uplc.FlatInstantces.decodeFlat[scalus.builtin.Data](bytes))

    // Apply arguments to term
    var applied = term
    for arg <- dataArgs do
      applied = Term.Apply(applied, Term.Const(Constant.Data(arg)))

    // Run CEK machine
    val program = Program.plutusV3(applied)
    val deBruijned = DeBruijn.deBruijnProgram(program)
    given PlutusVM = PlutusVM.makePlutusV3VM()
    val result = deBruijned.evaluateDebug

    // Encode result as JSON for Rust
    result match {
      case Result.Success(resTerm, budget, _, logs) =>
        s"""{"success":true,"cpu":${budget.cpu},"mem":${budget.memory},"logs":[${logs.map(l => s""""$l"""").mkString(",")}]}"""
      case Result.Failure(ex, budget, _, logs) =>
        val msg = ex.getMessage.replace("\"", "\\\"")
        s"""{"success":false,"cpu":${budget.cpu},"mem":${budget.memory},"logs":[${logs.map(l => s""""$l"""").mkString(",")}],"error":"$msg"}"""
    }
  }

  /** Script hash — blake2b-224 of the flat-encoded script. */
  def hash: Array[Byte] = {
    // PlutusV3 script envelope: 0x03 prefix + flat bytes
    val flat = toFlat
    val envelope = Array[Byte](0x03) ++ flat
    // blake2b-224
    import org.bouncycastle.crypto.digests.Blake2bDigest
    val digest = new Blake2bDigest(224)
    digest.update(envelope, 0, envelope.length)
    val out = new Array[Byte](28)
    digest.doFinal(out, 0)
    out
  }
}
```

### `build.sbt` changes

Add sbt-assembly plugin to produce an uber-JAR:

```scala
// project/plugins.sbt
addSbtPlugin("com.eed3si9n" % "sbt-assembly" % "2.2.0")

// build.sbt — loader project
lazy val loader = project.in(file("loader")).settings(
  commonSettings,
  name := "rustus-scala-loader",
  assembly / mainClass := Some("rustus.loader.Main"),
  assembly / assemblyJarName := "scalus-loader.jar",
  // merge strategy for conflicts
  assembly / assemblyMergeStrategy := {
    case PathList("META-INF", _*) => MergeStrategy.discard
    case _ => MergeStrategy.first
  },
  libraryDependencies ++= Seq(
    "com.github.plokhotnyuk.jsoniter-scala" %% "jsoniter-scala-core" % "2.38.8",
    "com.github.plokhotnyuk.jsoniter-scala" %% "jsoniter-scala-macros" % "2.38.8" % "compile",
  ),
)
```

Build with: `sbt loader/assembly` -> produces `scala-loader/loader/target/scala-3.3.7/scalus-loader.jar`

## JVM Lifecycle

### Finding the JVM

```rust
// rustus-jvm/src/jvm.rs

fn find_libjvm() -> Result<PathBuf, RustusError> {
    // 1. JAVA_HOME env var
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let path = Path::new(&java_home);
        // Linux: lib/server/libjvm.so
        // macOS: lib/server/libjvm.dylib
        // Windows: bin/server/jvm.dll
        for candidate in &[
            path.join("lib/server/libjvm.so"),
            path.join("lib/server/libjvm.dylib"),
            path.join("bin/server/jvm.dll"),
        ] {
            if candidate.exists() { return Ok(candidate.clone()); }
        }
    }

    // 2. `which java` -> resolve symlinks -> derive JAVA_HOME
    // java is often at $JAVA_HOME/bin/java
    ...

    Err(RustusError::JvmNotFound(
        "JDK 11+ required. Set JAVA_HOME or install from https://adoptium.net".into()
    ))
}
```

### uber-JAR Location

The uber-JAR (`scalus-loader.jar`) needs to be findable at runtime. Strategy:

1. Check `RUSTUS_JAR` env var (explicit override)
2. Check relative to the `rustus-jvm` crate's manifest dir (for development)
3. Check `~/.rustus/lib/scalus-loader.jar` (installed location)
4. Fail with instructions to run `sbt loader/assembly` or download

For published crates, the JAR could be downloaded as part of a post-install step
or bundled via `include_bytes!` (though it may be 30-50MB).

## Usage Modes

### Development: compile + eval (JVM required)

```rust
use rustus::prelude::*;

#[derive(ToData, FromData)]
struct MyDatum { owner: PubKeyHash }

#[rustus::compile]
fn my_validator(datum: Data, _redeemer: Data, ctx: Data) {
    let d: MyDatum = FromData::from_data(&datum).unwrap();
    let sc: ScriptContext = FromData::from_data(&ctx).unwrap();
    let signed = list::contains(sc.tx_info.signatories, d.owner);
    require!(signed, "Not signed by owner")
}

#[test]
fn test_validator() {
    let validator = rustus::compile("my_validator").unwrap();

    // Inspect UPLC
    println!("{}", validator.to_text().unwrap());

    // Run in CEK machine
    let datum = MyDatum { owner: my_pkh }.to_data();
    let redeemer = Data::unit();
    let ctx = mock_script_context();
    let result = validator.eval(&[datum, redeemer, ctx]).unwrap();
    assert!(result.succeeded());
    println!("Cost: {} CPU, {} MEM", result.cpu, result.mem);
}
```

### Export: compile to flat file (no JVM needed at runtime)

```rust
fn main() {
    // Compile and export
    rustus::compile_to_file("my_validator", Path::new("validators/my_validator.flat")).unwrap();
}
```

### Production: use pre-compiled flat bytes (no JVM)

```rust
const MY_VALIDATOR: &[u8] = include_bytes!("../validators/my_validator.flat");

fn main() {
    tx_builder.with_plutus_script(MY_VALIDATOR);
}
```

## Implementation Plan

### Phase 1: Scala side — RustusLoader + uber-JAR

1. Create `RustusLoader.scala` with `compile()` method wrapping existing `Main.scala` logic
2. Add `CompiledValidator` class wrapping `PlutusV3` term
3. Add sbt-assembly plugin, produce uber-JAR
4. Verify: `java -cp scalus-loader.jar rustus.loader.Main my_validator.sir.json` still works

### Phase 2: rustus-jvm crate — JVM lifecycle

1. Create `rustus-jvm` crate with `jni` dependency
2. Implement `find_libjvm()` — locate JDK
3. Implement `ScalusVM::new()` — create JavaVM with classpath pointing to uber-JAR
4. Implement JAR location strategy

### Phase 3: rustus-jvm crate — Validator API

1. Implement `ScalusVM::compile()` — JNI call to `RustusLoader.compile()`
2. Implement `Validator` struct holding `GlobalRef` to `CompiledValidator`
3. Implement `to_flat()`, `to_text()`, `eval()`, `hash()` via JNI
4. Implement `EvalResult` parsing from JSON response

### Phase 4: Facade integration

1. Update `rustus/Cargo.toml` to depend on `rustus-jvm`
2. Update `rustus/src/lib.rs` with `compile()`, `compile_to_file()` convenience functions
3. Add `OnceLock`-based lazy JVM singleton

### Phase 5: Examples + tests

1. Update `examples/pubkey-validator/main.rs` to use new API
2. Add test that compiles and evaluates via CEK
3. Add test that exports to flat and verifies bytes match
4. Verify error messages include Rust source locations

## Open Questions

1. **JAR distribution for published crates**: `include_bytes!` (large binary), download on first use,
   or require user to build from source? Probably download + cache for now.

2. **Scalus version pinning**: The uber-JAR bundles a specific Scalus version. How to handle
   version updates? Probably just rebuild and re-publish.

3. **Data argument encoding for eval()**: Currently sketched as flat-encoded Data bytes.
   Need to verify what encoding Scalus expects (CBOR? flat? JSON?). May be simpler to
   pass Data as JSON and decode on Scala side.

4. **Source location propagation**: Scalus annotations carry SIRPosition. Need to verify
   these survive through linking and lowering to UPLC, and that CEK errors surface them.

5. **Script hash calculation**: Exact envelope format depends on Plutus era (V1/V2/V3).
   May delegate to Scalus rather than reimplementing in Rust.
