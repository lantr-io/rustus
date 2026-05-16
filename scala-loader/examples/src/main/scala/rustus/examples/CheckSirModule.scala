package rustus.examples

/** Check the compiled SIR module for PubKeyValidator. */
object CheckSirModule:
  def main(args: Array[String]): Unit =
    import scalus.show
    val modules = scalus.compiler.compiledModules("rustus.examples.PubKeyValidator")
    val module = modules.getOrElse(
      "rustus.examples.PubKeyValidator",
      sys.error(s"PubKeyValidator module not found. Available: ${modules.keys.mkString(", ")}")
    )
    println(s"Module name: ${module.name}")
    println(s"Module version: ${module.version}")
    println(s"Bindings: ${module.defs.size}")
    for binding <- module.defs do
      println(s"\n  ${binding.name}: ${binding.tp.show}")
      println(s"  ${show(binding.value)}")
