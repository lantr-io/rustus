package rustus.examples

import scalus.Compile
import scalus.uplc.builtin.{ByteString, Data}
import scalus.uplc.builtin.Data.{fromData, toData}
import scalus.cardano.onchain.plutus.prelude.*
import scalus.cardano.onchain.plutus.v1.*

/** Analog of our Rust PubKeyValidator.
  *
  * V1 style: 3 arguments (datum, redeemer, ctx).
  * Checks that the signatories list in the ScriptContext
  * contains the owner's PubKeyHash from the datum.
  */
@Compile
object PubKeyValidator {

  /** The datum: owner's public key hash */
  case class OwnerDatum(owner: PubKeyHash)

  given scalus.uplc.builtin.ToData[OwnerDatum] = scalus.uplc.builtin.ToData.derived
  given scalus.uplc.builtin.FromData[OwnerDatum] = scalus.uplc.builtin.FromData.derived

  /** The validator: check signatories contains owner */
  def validator(datum: Data, redeemer: Data, ctx: Data): Unit = {
    val ownerDatum = datum.to[OwnerDatum]
    val scriptContext = ctx.to[ScriptContext]
    val signed = scriptContext.txInfo.signatories.contains(ownerDatum.owner)
    require(signed, "Not signed by owner")
  }
}

/** Helper to access the compiled SIR and UPLC from other projects */
object PubKeyValidatorCompiled {
  import scalus.compiler.{compile, Options}
  import scalus.compiler.sir.TargetLoweringBackend

  given Options = Options(
    targetLoweringBackend = TargetLoweringBackend.SirToUplcV3Lowering,
    generateErrorTraces = true,
    optimizeUplc = false,
  )

  lazy val sir: scalus.compiler.sir.SIR = compile { PubKeyValidator.validator }
  lazy val uplcTerm: scalus.uplc.Term = scalus.toUplc(sir)()
}
