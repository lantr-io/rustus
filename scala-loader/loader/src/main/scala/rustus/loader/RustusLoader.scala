package rustus.loader

import com.github.plokhotnyuk.jsoniter_scala.core.*
import rustus.loader.RustusJsonCodec.*
import scalus.cardano.ledger.MajorProtocolVersion
import scalus.compiler.sir.*
import scalus.compiler.sir.linking.{SIRLinker, SIRLinkerOptions}
import scalus.compiler.sir.lowering.SirToUplcV3Lowering
import scalus.compiler.sir.transform.EraseSpecifications
import scalus.uplc.{Constant, DeBruijn, Program => UplcProgram, Term}
import scalus.uplc.builtin.Data
import scalus.uplc.eval.*

/** JNI entry point for the rustus-jvm Rust crate.
  * Methods are called via JNI from Rust.
  */
object RustusLoader:

  /** Compile SIR JSON string to a CompiledValidator.
    * Called from Rust via JNI: ScalusVM::compile()
    */
  def compile(sirJson: String): CompiledValidator =
    val rmodule = readFromArray[RModule](sirJson.getBytes("UTF-8"))
    val converted = convert(rmodule)
    lower(link(converted.main.value, converted.supportModules), rmodule.options)

  // `compile` runs three stages: `convert`, `link`, `lower`. Tools that show each stage (`Main`,
  // `CompareSIR`) call them in turn, so they compile exactly as `compile` does, and can show what
  // a stage produced before the next one fails.

  /** A Rust module as Scalus SIR: the module, its main binding, and the modules holding every
    * other binding, which the main binding is linked against.
    */
  final case class Converted(module: Module, main: Binding, supportModules: Map[String, Module])

  /** Converts `rmodule` to Scalus SIR and picks its main binding. */
  def convert(rmodule: RModule): Converted =
    val result = RustusToScalus.transform(rmodule)
    val module = result.module
    val main = result.mainBinding.getOrElse(
      throw new RuntimeException("No main binding found in module")
    )

    // All non-main bindings go to linker (intrinsic resolver needs them in scope too)
    val supportModules: Map[String, Module] =
      module.defs
        .filter(_.name != main.name)
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
    Converted(module, main, supportModules)

  /** Links `main` against `supportModules`, then erases the clauses of Scalus's `Spec` module
    * (`Spec.expects`, `Spec.ensures`, `ensuring`), as Scalus's own `UplcPipeline` does before
    * lowering. Without that step a clause in a linked Scalus function would be compiled into the
    * script and evaluated on-chain.
    */
  def link(main: SIR, supportModules: Map[String, Module]): SIR =
    val linkerOptions = SIRLinkerOptions(
      useUniversalDataConversion = true,
      printErrors = true,
      debugLevel = 0
    )
    val linked = new SIRLinker(linkerOptions, supportModules).link(main, SIRPosition.empty)
    EraseSpecifications(linked)

  /** Lowers linked SIR to UPLC with the module's compiler options. */
  def lower(linked: SIR, options: RCompilerOptions): CompiledValidator =
    val protocolVersion = MajorProtocolVersion(options.target_protocol_version)
    // Scalus's intrinsic modules give types like PubKeyHash/TxId/AssocMap/Value their
    // `@UplcRepr` annotations during lowering.
    val term = SirToUplcV3Lowering(
      linked,
      generateErrorTraces = options.generate_error_traces,
      targetProtocolVersion = protocolVersion,
      intrinsicModules = scalus.compiler.sir.lowering.IntrinsicResolver.defaultIntrinsicModules,
      supportModules = scalus.compiler.sir.lowering.IntrinsicResolver.defaultSupportModules
    ).lower()
    CompiledValidator(term, protocolVersion)

  /** The version of the Scalus library this JAR was built with.
    * Called from Rust via JNI, which refuses a JAR built against another Scalus version.
    */
  def scalusVersion: String = scalus.utils.BuildInfo.version

  /** A hash of the sources this JAR was built from, written into it by build.sbt
    * (project/LoaderFingerprint.scala). Called from Rust via JNI: rustus-jvm's build.rs computes
    * the same hash of its checkout, and a JAR whose hash differs is refused. Empty if the JAR
    * carries none.
    */
  def loaderFingerprint: String =
    Option(getClass.getResourceAsStream("/" + LoaderFingerprintResource)) match
      case Some(in) => try new String(in.readAllBytes(), "UTF-8").trim finally in.close()
      case None     => ""

  /** The resource holding [[loaderFingerprint]]; build.sbt writes it under this name. */
  val LoaderFingerprintResource = "rustus-loader-fingerprint.txt"

/** Holds a compiled UPLC term with annotations for source mapping.
  * Rust holds a JNI GlobalRef to instances of this class.
  */
class CompiledValidator(val term: Term, val protocolVersion: MajorProtocolVersion = MajorProtocolVersion.changPV):

  private lazy val flatBytes: Array[Byte] = UplcProgram.plutusV3(term).flatEncoded
  private lazy val plutusVM: PlutusVM = PlutusVM.makePlutusV3VM(protocolVersion)

  /** UPLC flat-encoded bytes (strips annotations). For on-chain submission. */
  def toFlat: Array[Byte] = flatBytes

  /** UPLC pretty-printed with annotations. For debugging/inspection. */
  def toText: String =
    term.pretty.render(120)

  /** Evaluate with Data arguments passed as JSON.
    * JSON format: array of Rust Data enum values, e.g.:
    *   [{"type":"Constr","tag":0,"args":[...]}, {"type":"I","value":"42"}, ...]
    * Returns JSON-encoded EvalResult.
    */
  def eval(argsJson: String): String =
    // Parse Data arguments from JSON
    val argsRaw = readFromString[List[Any]](argsJson)(using RustusJsonCodec.anyListCodec)
    val result = evaluate(argsRaw.map(RustusJsonCodec.parseRustusData))

    // Build result JSON using jsoniter for proper escaping
    result match
      case Result.Success(resTerm, budget, _, logs) =>
        writeToString(EvalResultJson(
          success = true,
          cpu = budget.steps,
          mem = budget.memory,
          logs = logs.toList,
          error = None
        ))(using RustusJsonCodec.evalResultCodec)
      case Result.Failure(ex, budget, _, logs) =>
        writeToString(EvalResultJson(
          success = false,
          cpu = budget.steps,
          mem = budget.memory,
          logs = logs.toList,
          error = Some(ex.getMessage)
        ))(using RustusJsonCodec.evalResultCodec)

  /** Applies the validator to `args` and runs it in the CEK machine, on a PlutusV3 VM for
    * `protocolVersion`. What `eval` does, for callers holding Scalus `Data`.
    */
  def evaluate(args: List[Data]): Result =
    val applied = args.foldLeft(term)((t, arg) => Term.Apply(t, Term.Const(Constant.Data(arg))))
    given PlutusVM = plutusVM
    DeBruijn.deBruijnProgram(UplcProgram.plutusV3(applied)).evaluateDebug

  /** Script hash — blake2b-224 of the PlutusV3 script envelope. */
  def scriptHash: Array[Byte] =
    val flat = toFlat
    // PlutusV3 script envelope: CBOR tag(3, bytes(flat))
    // Script hash = blake2b-224(0x03 ++ flat)
    import org.bouncycastle.crypto.digests.Blake2bDigest
    val envelope = Array[Byte](0x03) ++ flat
    val digest = new Blake2bDigest(224)
    digest.update(envelope, 0, envelope.length)
    val out = new Array[Byte](28)
    digest.doFinal(out, 0)
    out
