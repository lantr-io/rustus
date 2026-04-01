package rustus.loader

import com.github.plokhotnyuk.jsoniter_scala.core.*
import rustus.loader.RustusJsonCodec.*
import scalus.cardano.ledger.MajorProtocolVersion
import scalus.compiler.sir.*
import scalus.compiler.sir.linking.{SIRLinker, SIRLinkerOptions}
import scalus.compiler.sir.lowering.SirToUplcV3Lowering
import scalus.uplc.{Constant, DeBruijn, Program => UplcProgram, Term}
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
    val result = RustusToScalus.transform(rmodule)
    val module = result.module

    result.mainBinding match
      case Some(binding) =>
        def buildModuleMap(defs: List[Binding]): Map[String, Module] =
          defs
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

        // All non-main bindings go to linker (intrinsic resolver needs them in scope too)
        val allSupportModules = buildModuleMap(
          module.defs.filter(b => b.name != binding.name)
        )

        val opts = rmodule.options

        val linkerOptions = SIRLinkerOptions(
          useUniversalDataConversion = true,
          printErrors = true,
          debugLevel = 0
        )
        val linker = new SIRLinker(linkerOptions, allSupportModules)
        val linkedSir = linker.link(binding.value, SIRPosition.empty)
        val lowering = SirToUplcV3Lowering(
          linkedSir,
          generateErrorTraces = opts.generate_error_traces,
          targetProtocolVersion = MajorProtocolVersion(opts.target_protocol_version),
          intrinsicModules = scalus.compiler.sir.lowering.IntrinsicResolver.defaultIntrinsicModules,
          supportModules = scalus.compiler.sir.lowering.IntrinsicResolver.defaultSupportModules
        )
        val term = lowering.lower()
        CompiledValidator(term, MajorProtocolVersion(opts.target_protocol_version))

      case None =>
        throw new RuntimeException("No main binding found in module")

/** Holds a compiled UPLC term with annotations for source mapping.
  * Rust holds a JNI GlobalRef to instances of this class.
  */
class CompiledValidator(val term: Term, protocolVersion: MajorProtocolVersion = MajorProtocolVersion.changPV):

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
    import scalus.uplc.TermDSL.given

    // Parse Data arguments from JSON
    val argsRaw = readFromString[List[Any]](argsJson)(using RustusJsonCodec.anyListCodec)
    val dataArgs = argsRaw.map(RustusJsonCodec.parseRustusData)

    // Apply arguments to term
    var applied = term
    for arg <- dataArgs do
      applied = Term.Apply(applied, Term.Const(Constant.Data(arg)))

    // Run CEK machine
    val program = UplcProgram.plutusV3(applied)
    val deBruijned = DeBruijn.deBruijnProgram(program)
    given PlutusVM = plutusVM
    val result = deBruijned.evaluateDebug

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
