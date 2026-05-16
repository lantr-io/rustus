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

          // Pass Scalus's intrinsic modules so types like PubKeyHash/TxId/AssocMap/Value
          // pick up their `@UplcRepr` annotations during lowering (Scalus 0.17 moved these
          // from hardcoded `OneElementWrapper` defaults to annotation-driven dispatch).
          val lowering = SirToUplcV3Lowering(
            linkedSir,
            generateErrorTraces = true,
            intrinsicModules = scalus.compiler.sir.lowering.IntrinsicResolver.defaultIntrinsicModules,
            supportModules = scalus.compiler.sir.lowering.IntrinsicResolver.defaultSupportModules
          )
          val term = lowering.lower()
          println(s"UPLC term:")
          println(term.pretty.render(80))

          // 5. Apply to test arguments and evaluate
          // The Rust validator takes `Data` and uses `Datum::from_data(...).unwrap()` inside,
          // which lowers to `SIR.Apply(UDC.fromData, datum)` — a noop in V3. So we can pass the
          // datum as a plain `Data.Constr`, matching the actual Plutus runtime calling convention.
          println("\n--- Applying to test arguments ---")
          import scalus.uplc.builtin.Data
          import scalus.cardano.onchain.plutus.prelude.{List => ScList}

          val owner = scalus.uplc.builtin.ByteString.fromArray(Array[Byte](1, 2, 3))
          val colorRed = Data.Constr(0, ScList.Nil)
          val datumData = Data.Constr(0, ScList(Data.B(owner), colorRed))

          import TermDSL.given
          val appliedTerm = term $ datumData $ Data.unit $ Data.unit
          println(s"Applied: validator(datum=Red)(unit)(unit)")

          println("\n--- Evaluation (PlutusV3) ---")
          // Plutus V3 validators must return Unit (throwing on failure). The Rust validator
          // returns Bool, so wrap: `if result then () else error`.
          val unitTerm = scalus.uplc.Term.Const(scalus.uplc.Constant.Unit)
          val wrappedTerm = scalus.uplc.Term.Force(
            scalus.uplc.Term.Apply(
              scalus.uplc.Term.Apply(
                scalus.uplc.Term.Apply(
                  scalus.uplc.Term.Force(scalus.uplc.Term.Builtin(DefaultFun.IfThenElse)),
                  appliedTerm
                ),
                scalus.uplc.Term.Delay(unitTerm)
              ),
              scalus.uplc.Term.Delay(scalus.uplc.Term.Error())
            )
          )
          val program = scalus.uplc.Program.plutusV3(wrappedTerm)
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
