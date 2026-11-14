ThisBuild / scalaVersion := "3.3.9"

val scalusVersion = "1.5.0"

// The loader uses these directly. Keep them at or above what Scalus brings in, or sbt silently
// evicts them to Scalus's versions: check `sbt loader/evicted` after a Scalus upgrade.
val jsoniterVersion = "2.41.6"
val bouncyCastleVersion = "1.86"

val jsoniterDeps = Seq(
  "com.github.plokhotnyuk.jsoniter-scala" %% "jsoniter-scala-core" % jsoniterVersion,
  "com.github.plokhotnyuk.jsoniter-scala" %% "jsoniter-scala-macros" % jsoniterVersion % "compile",
)

val commonSettings = Seq(
  libraryDependencies += "org.scalus" %% "scalus" % scalusVersion,
  autoCompilerPlugins := true,
  addCompilerPlugin("org.scalus" % "scalus-plugin" % scalusVersion cross CrossVersion.full),
)

// Existing loader: reads rustus JSON, transforms to scalus SIR, lowers to UPLC
lazy val loader = project.in(file("loader")).settings(
  commonSettings,
  name := "rustus-scala-loader",
  assembly / mainClass := Some("rustus.loader.Main"),
  assembly / assemblyJarName := "rustus-scalus.jar",
  // Outside target/scala-<version>/, so rustus-jvm finds it whatever the Scala version.
  assembly / assemblyOutputPath := target.value / (assembly / assemblyJarName).value,
  assembly / assemblyMergeStrategy := {
    case PathList("META-INF", _*) => MergeStrategy.discard
    case _ => MergeStrategy.first
  },
  libraryDependencies ++= jsoniterDeps :+ ("org.bouncycastle" % "bcprov-jdk18on" % bouncyCastleVersion),
  // The hash of the sources the JAR is built from, which rustus-jvm checks (RustusLoader.loaderFingerprint).
  Compile / resourceGenerators += Def.task {
    val file = (Compile / resourceManaged).value / "rustus-loader-fingerprint.txt"
    IO.write(file, LoaderFingerprint((ThisBuild / baseDirectory).value))
    Seq(file)
  }.taskValue,
)

// Scala examples: scalus validators that are analogs of the Rust validators
lazy val examples = project.in(file("examples")).settings(
  commonSettings,
  name := "rustus-scala-examples",
)

// Tests: compile scala examples, load Rust SIR, compare side by side
lazy val tests = project.in(file("tests")).settings(
  commonSettings,
  name := "rustus-scala-tests",
  libraryDependencies ++= jsoniterDeps,
).dependsOn(loader, examples)

lazy val root = project.in(file(".")).aggregate(loader, examples, tests)
