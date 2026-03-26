ThisBuild / scalaVersion := "3.3.7"

val scalusVersion = "0.16.0+78-667d85de-SNAPSHOT"

val commonSettings = Seq(
  libraryDependencies += "org.scalus" %% "scalus" % scalusVersion,
  autoCompilerPlugins := true,
  addCompilerPlugin("org.scalus" % "scalus-plugin" % scalusVersion cross CrossVersion.full),
)

// Existing loader: reads rustus JSON, transforms to scalus SIR, lowers to UPLC
lazy val loader = project.in(file("loader")).settings(
  commonSettings,
  name := "rustus-scala-loader",
  libraryDependencies ++= Seq(
    "com.github.plokhotnyuk.jsoniter-scala" %% "jsoniter-scala-core" % "2.38.8",
    "com.github.plokhotnyuk.jsoniter-scala" %% "jsoniter-scala-macros" % "2.38.8" % "compile",
  ),
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
  libraryDependencies ++= Seq(
    "com.github.plokhotnyuk.jsoniter-scala" %% "jsoniter-scala-core" % "2.38.8",
    "com.github.plokhotnyuk.jsoniter-scala" %% "jsoniter-scala-macros" % "2.38.8" % "compile",
  ),
).dependsOn(loader, examples)

lazy val root = project.in(file(".")).aggregate(loader, examples, tests)
