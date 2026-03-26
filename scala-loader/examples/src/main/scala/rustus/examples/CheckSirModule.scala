package rustus.examples

/** Check if sirModule is available on the PubKeyValidator companion object */
object CheckSirModule:
  def main(args: Array[String]): Unit =
    import scalus.show
    // The scalus plugin adds sirModule to @Compile objects
    val module = PubKeyValidator.sirModule
    println(s"Module name: ${module.name}")
    println(s"Module version: ${module.version}")
    println(s"Bindings: ${module.defs.size}")
    for binding <- module.defs do
      println(s"\n  ${binding.name}: ${binding.tp.show}")
      println(s"  ${show(binding.value)}")
