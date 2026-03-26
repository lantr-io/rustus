package rustus.loader

import com.github.plokhotnyuk.jsoniter_scala.core.*
import rustus.loader.RustusJsonCodec.*
import scalus.compiler.sir.*
import scalus.compiler.sir.linking.{SIRLinker, SIRLinkerOptions}
import scalus.compiler.sir.lowering.SirToUplcV3Lowering
import scalus.show
import scalus.uplc.*
import scalus.uplc.TermDSL
import scalus.uplc.eval.*

object Main:

  def main(args: Array[String]): Unit =
    val jsonPath = args.headOption.getOrElse {
      System.err.println("Usage: rustus-scala-loader <path-to-sir-json>")
      System.exit(1)
      ""
    }

    // 1. Read JSON
    println(s"Loading SIR from: $jsonPath")
    val bytes = java.nio.file.Files.readAllBytes(java.nio.file.Path.of(jsonPath))
    val rmodule = readFromArray[RModule](bytes)
    println(s"Parsed module '${rmodule.name}' v${rmodule.version._1}.${rmodule.version._2}")
    println(s"  Data declarations: ${rmodule.data_decls.keys.mkString(", ")}")
    println(s"  Bindings: ${rmodule.defs.map(_.name).mkString(", ")}")

    // 2. Transform to scalus SIR
    println("\n--- Transforming to scalus SIR ---")
    val result = RustusToScalus.transform(rmodule)
    val module = result.module

    // 3. Pretty-print SIR
    println("\n--- SIR ---")
    for binding <- module.defs do
      println(s"${binding.name}: ${binding.tp.show}")
      println(binding.value.show)

    // 4. Link and lower to UPLC
    println("\n--- Linking & Lowering to UPLC ---")
    result.mainBinding match
      case Some(binding) =>
        try
          // Build support modules from all non-main bindings, grouped by module_name
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

          // Use scalus linker to resolve ExternalVar references
          val linkerOptions = SIRLinkerOptions(
            useUniversalDataConversion = true,
            printErrors = true,
            debugLevel = 0
          )
          val linker = new SIRLinker(linkerOptions, supportModules)
          val linkedSir = linker.link(binding.value, SIRPosition.empty)

          println("Linked SIR:")
          println(scalus.show(linkedSir))

          val lowering = SirToUplcV3Lowering(linkedSir, generateErrorTraces = true)
          val term = lowering.lower()
          println(s"UPLC term:")
          println(term.pretty.render(80))

          // 5. Apply to test arguments and evaluate (V1 style: 3 args)
          println("\n--- Applying to test arguments (V1) ---")
          import scalus.uplc.builtin.ByteString.*
          import scalus.uplc.builtin.Data
          import scalus.uplc.builtin.Data.toData
          import scalus.cardano.onchain.plutus.v1.*
          import scalus.cardano.onchain.plutus.prelude.List.{Cons, Nil}

          // Build V1 ScriptContext using scalus's own typed constructors
          val ownerPkh = PubKeyHash(hex"deadbeef")
          val scriptContext = ScriptContext(
            TxInfo(
              Nil,                                         // inputs
              Nil,                                         // outputs
              Value.zero,                                  // fee
              Value.zero,                                  // mint
              Nil,                                         // dcert
              Nil,                                         // withdrawals
              Interval.always,                             // valid_range
              Cons(ownerPkh, Nil),                         // signatories — owner is here!
              Nil,                                         // data
              TxId(hex"bb")                                // id
            ),
            ScriptPurpose.Spending(TxOutRef(TxId(hex"deadbeef"), 0))
          )

          // OwnerDatum { owner: Data } — datum contains the owner pkh as Data
          val datumData = Data.Constr(0, scalus.cardano.onchain.plutus.prelude.List(ownerPkh.toData))

          import TermDSL.given
          val appliedTerm = term $ datumData $ Data.unit $ scriptContext.toData
          println(s"Applied: validator(datum)(unit)(scriptContext)")

          println("\n--- Evaluation (PlutusV3) ---")
          val program = scalus.uplc.Program.plutusV3(appliedTerm)
          val deBruijned = DeBruijn.deBruijnProgram(program)
          given PlutusVM = PlutusVM.makePlutusV3VM()
          val evalResult = deBruijned.evaluateDebug
          evalResult match
            case Result.Success(resTerm, budget, _, logs) =>
              println(s"SUCCESS! Result: ${resTerm.pretty.render(80)}")
              println(s"Budget: $budget")
              if logs.nonEmpty then println(s"Logs: ${logs.mkString(", ")}")
            case Result.Failure(ex, budget, _, logs) =>
              println(s"FAILED: ${ex.getMessage}")
              println(s"Budget: $budget")
              if logs.nonEmpty then println(s"Logs: ${logs.mkString(", ")}")
        catch
          case e: Exception =>
            println(s"Error during lowering/evaluation: ${e.getMessage}")
            e.printStackTrace()

      case None =>
        println("No bindings to lower")
