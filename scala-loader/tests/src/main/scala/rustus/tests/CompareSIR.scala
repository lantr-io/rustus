package rustus.tests

import com.github.plokhotnyuk.jsoniter_scala.core.*
import rustus.loader.RustusJsonCodec.*
import rustus.loader.{CompiledValidator, RustusLoader}
import scalus.*
import scalus.uplc.*
import scalus.uplc.builtin.ByteString.*
import scalus.uplc.builtin.Data
import scalus.uplc.builtin.Data.toData
import scalus.uplc.eval.*
import scalus.cardano.onchain.plutus.v1.*
import scalus.cardano.onchain.plutus.prelude.List.{Cons, Nil}

/** Compares the Scalus-compiled PubKeyValidator with the Rust one.
  *
  * The Rust validator is compiled from its SIR JSON exactly as rustus compiles it, through
  * `RustusLoader`'s `convert`, `link` and `lower`; each stage is printed as it completes. Write
  * the JSON with `cargo run --example pubkey-validator -- <out.sir.json>`.
  *
  * Each validator is evaluated, on the VM for the protocol version it was compiled for, on a
  * context signed by the owner, which it must accept, and on one signed by someone else, which
  * it must reject. The run fails if either validator gets a case wrong.
  */
object CompareSIR:

  private val usage =
    "Usage: CompareSIR <path-to-pubkey_validator.sir.json>\n" +
      "Write the JSON with: cargo run --example pubkey-validator -- <path>"

  private val owner = PubKeyHash(hex"deadbeef")

  /** (case, the transaction's only signatory, whether the validator must accept) */
  private val cases = List(
    ("signed by the owner", owner, true),
    ("signed by someone else", PubKeyHash(hex"cafe"), false)
  )

  def main(args: Array[String]): Unit =
    val jsonPath = args match
      case Array(path) => path
      case _           => sys.error(usage)

    println("=" * 60)
    println("SCALA-COMPILED PubKeyValidator")
    println("=" * 60)

    println(s"\n--- Scala SIR ---")
    println(scalus.show(rustus.examples.PubKeyValidatorCompiled.sir))

    val scalaValidator = CompiledValidator(
      rustus.examples.PubKeyValidatorCompiled.uplcTerm,
      rustus.examples.PubKeyValidatorCompiled.options.targetProtocolVersion
    )
    println(s"\n--- Scala UPLC ---")
    println(scalaValidator.term.pretty.render(80))

    println(s"\n--- Scala Evaluation ---")
    val scalaWrong = evaluate(scalaValidator, "Scala")

    val rustValidator = compileRust(jsonPath)
    println(s"\n--- Rust Evaluation ---")
    val rustWrong = evaluate(rustValidator, "Rust")

    val wrong = scalaWrong ++ rustWrong
    println()
    if wrong.nonEmpty then
      sys.error(s"PubKeyValidator gets ${wrong.size} case(s) wrong: ${wrong.mkString("; ")}")
    else println("OK: the Scala and the Rust validator both accept and reject as expected")

  /** Compiles the Rust validator from `jsonPath` as `RustusLoader.compile` does, printing the
    * result of each stage before running the next, so a failing stage still shows its input.
    */
  private def compileRust(jsonPath: String): CompiledValidator =
    println("\n" + "=" * 60)
    println("RUST-GENERATED PubKeyValidator")
    println("=" * 60)

    val bytes = java.nio.file.Files.readAllBytes(java.nio.file.Path.of(jsonPath))
    val rmodule = readFromArray[RModule](bytes)
    val converted = RustusLoader.convert(rmodule)

    println(s"\n--- Rust SIR ---")
    for binding <- converted.module.defs do
      println(s"\n  ${binding.name}: ${binding.tp.show}")
      println(s"  ${binding.value.show}")

    val linked = RustusLoader.link(converted.main.value, converted.supportModules)
    println(s"\n--- Rust linked SIR ---")
    println(scalus.show(linked))

    val validator = RustusLoader.lower(linked, rmodule.options)
    println(s"\n--- Rust UPLC ---")
    println(validator.term.pretty.render(80))
    validator

  /** Evaluates `validator` on every case; returns the cases it gets wrong. */
  private def evaluate(validator: CompiledValidator, label: String): List[String] =
    // OwnerDatum { owner = PubKeyHash(deadbeef) }
    val datumData = Data.Constr(0, scalus.cardano.onchain.plutus.prelude.List(owner.toData))

    cases.flatMap { (name, signer, mustAccept) =>
      // V1 ScriptContext whose only signatory is `signer`
      val scriptContext = ScriptContext(
        TxInfo(Nil, Nil, Value.zero, Value.zero, Nil, Nil, Interval.always,
          Cons(signer, Nil), Nil, TxId(hex"bb")),
        ScriptPurpose.Spending(TxOutRef(TxId(hex"deadbeef"), 0))
      )
      val result = validator.evaluate(List(datumData, Data.unit, scriptContext.toData))
      val accepted = result match
        case Result.Success(t, budget, _, logs) =>
          println(s"$label, $name: SUCCESS ${t.pretty.render(80)}, budget $budget")
          if logs.nonEmpty then println(s"  logs: ${logs.mkString(", ")}")
          true
        case Result.Failure(ex, budget, _, logs) =>
          println(s"$label, $name: FAILED ${ex.getMessage}, budget $budget")
          if logs.nonEmpty then println(s"  logs: ${logs.mkString(", ")}")
          false
      if accepted == mustAccept then None
      else Some(s"$label ${if mustAccept then "rejects" else "accepts"} a transaction $name")
    }
