package rustus.tests

import com.github.plokhotnyuk.jsoniter_scala.core.*
import rustus.loader.RustusJsonCodec.*
import rustus.loader.RustusToScalus
import scalus.*
import scalus.compiler.Options
import scalus.compiler.sir.*
import scalus.compiler.sir.TargetLoweringBackend
import scalus.compiler.sir.linking.{SIRLinker, SIRLinkerOptions}
import scalus.compiler.sir.lowering.SirToUplcV3Lowering
import scalus.uplc.*
import scalus.uplc.eval.*

/** Compare Scala-compiled SIR with Rust-generated SIR for PubKeyValidator */
object CompareSIR:

  given Options = Options(
    targetLoweringBackend = TargetLoweringBackend.SirToUplcV3Lowering,
    generateErrorTraces = true,
    optimizeUplc = false,
    debug = false
  )

  def main(args: Array[String]): Unit =
    // --- Part 1: Scala-compiled validator ---
    println("=" * 60)
    println("SCALA-COMPILED PubKeyValidator")
    println("=" * 60)

    val scalaSir = rustus.examples.PubKeyValidatorCompiled.sir
    println(s"\n--- Scala SIR ---")
    println(scalus.show(scalaSir))

    val scalaTerm = rustus.examples.PubKeyValidatorCompiled.uplcTerm
    println(s"\n--- Scala UPLC ---")
    println(scalaTerm.pretty.render(80))

    println(s"\n--- Scala Evaluation ---")
    evaluateValidator(scalaTerm, "Scala")

    // --- Part 2: Rust-generated validator ---
    val jsonPath = args.headOption.getOrElse {
      println("\nSkipping Rust comparison — no JSON path provided")
      println("Usage: CompareSIR <path-to-pubkey_validator.sir.json>")
      return
    }

    println("\n" + "=" * 60)
    println("RUST-GENERATED PubKeyValidator")
    println("=" * 60)

    val bytes = java.nio.file.Files.readAllBytes(java.nio.file.Path.of(jsonPath))
    val rmodule = readFromArray[RModule](bytes)
    val result = RustusToScalus.transform(rmodule)
    val module = result.module

    for binding <- module.defs do
      println(s"\n  ${binding.name}: ${binding.tp.show}")
      println(s"  ${binding.value.show}")

    result.mainBinding match
      case Some(binding) =>
        // Link support functions
        val supportModules: Map[String, Module] =
          module.defs
            .filter(_.name != binding.name)
            .groupBy(b =>
              rmodule.defs.find(_.name == b.name).flatMap(_.module_name).getOrElse(module.name)
            )
            .map { (modName, bindings) =>
              modName -> Module(module.version, modName, false, None, bindings)
            }

        val linkerOptions = SIRLinkerOptions(
          useUniversalDataConversion = true,
          printErrors = true,
          debugLevel = 0
        )
        val linker = new SIRLinker(linkerOptions, supportModules)
        val linkedSir = linker.link(binding.value, SIRPosition.empty)

        println(s"\n--- Rust linked SIR ---")
        println(scalus.show(linkedSir))

        println(s"\n--- Rust UPLC ---")
        try
          val rustTerm = SirToUplcV3Lowering(linkedSir, generateErrorTraces = true).lower()
          println(rustTerm.pretty.render(80))

          println(s"\n--- Rust Evaluation ---")
          evaluateValidator(rustTerm, "Rust")
        catch
          case e: Exception =>
            println(s"Lowering failed: ${e.getMessage}")

      case None =>
        println("No main binding found")

  private def evaluateValidator(term: Term, label: String): Unit =
    import scalus.uplc.builtin.ByteString.*
    import scalus.uplc.builtin.Data
    import scalus.uplc.builtin.Data.toData
    import scalus.cardano.onchain.plutus.v1.*
    import scalus.cardano.onchain.plutus.prelude.List.{Cons, Nil}

    val ownerPkh = PubKeyHash(hex"deadbeef")

    // V1 ScriptContext with signatories containing our owner
    val scriptContext = ScriptContext(
      TxInfo(Nil, Nil, Value.zero, Value.zero, Nil, Nil, Interval.always,
        Cons(ownerPkh, Nil), Nil, TxId(hex"bb")),
      ScriptPurpose.Spending(TxOutRef(TxId(hex"deadbeef"), 0))
    )

    // OwnerDatum { owner = PubKeyHash(deadbeef) }
    val datumData = Data.Constr(0, scalus.cardano.onchain.plutus.prelude.List(ownerPkh.toData))

    import TermDSL.given
    val applied = term $ datumData $ Data.unit $ scriptContext.toData
    given PlutusVM = PlutusVM.makePlutusV3VM()
    val result = scalus.uplc.Program.plutusV3(applied).deBruijnedProgram.evaluateDebug
    result match
      case Result.Success(t, budget, _, logs) =>
        println(s"$label SUCCESS: ${t.pretty.render(80)}")
        println(s"$label Budget: $budget")
        if logs.nonEmpty then println(s"$label Logs: ${logs.mkString(", ")}")
      case Result.Failure(ex, budget, _, logs) =>
        println(s"$label FAILED: ${ex.getMessage}")
        println(s"$label Budget: $budget")
        if logs.nonEmpty then println(s"$label Logs: ${logs.mkString(", ")}")
