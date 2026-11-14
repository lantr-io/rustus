package rustus.loader

import com.github.plokhotnyuk.jsoniter_scala.core.*
import rustus.loader.RustusJsonCodec.*
import scalus.compiler.sir.*
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

    // 2-4. Convert, link and lower, as RustusLoader.compile does, printing each stage
    try
      println("\n--- Transforming to scalus SIR ---")
      val converted = RustusLoader.convert(rmodule)

      println("\n--- SIR ---")
      for binding <- converted.module.defs do
        println(s"${binding.name}: ${binding.tp.show}")
        println(binding.value.show)

      println("\n--- Linking & Lowering to UPLC ---")
      val linkedSir = RustusLoader.link(converted.main.value, converted.supportModules)
      println("Linked SIR:")
      println(scalus.show(linkedSir))

      val validator = RustusLoader.lower(linkedSir, rmodule.options)
      println(s"UPLC term:")
      println(validator.term.pretty.render(80))

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
      val appliedTerm = validator.term $ datumData $ Data.unit $ Data.unit
      println(s"Applied: validator(datum=Red)(unit)(unit)")

      println("\n--- Evaluation (PlutusV3) ---")
      // Plutus V3 validators must return Unit (throwing on failure). The Rust validator
      // returns Bool, so wrap: `if result then () else error`.
      val unitTerm = Term.Const(Constant.Unit)
      val wrappedTerm = Term.Force(
        Term.Apply(
          Term.Apply(
            Term.Apply(Term.Force(Term.Builtin(DefaultFun.IfThenElse)), appliedTerm),
            Term.Delay(unitTerm)
          ),
          Term.Delay(Term.Error())
        )
      )
      // On the VM for the protocol version the validator was lowered for, as `eval` runs it
      CompiledValidator(wrappedTerm, validator.protocolVersion).evaluate(Nil) match
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
