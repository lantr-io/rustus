import java.io.File
import java.nio.file.{Files, Path}
import java.security.MessageDigest

import scala.collection.JavaConverters._

/** A hash of what the loader JAR is built from: `build.sbt` and every file under `loader/src/`.
  *
  * build.sbt writes it into the JAR, and rustus-jvm/build.rs computes it over the same checkout;
  * rustus-jvm refuses a JAR whose hash differs, as a JAR built from other sources. The two
  * implementations must agree exactly:
  *   - files: `build.sbt`, and every regular file under `loader/src/` (symbolic links to files
  *     count, links to directories are not followed), leaving out any whose path has a component
  *     starting with `.` or ending with `~` (editor and OS litter);
  *   - each named by its path relative to the scala-loader directory, with `/` separators;
  *   - SHA-256 over the files in byte order of their names, of `name`, `\n`, the content's length
  *     in decimal, `\n`, then the content; written as lowercase hex.
  */
object LoaderFingerprint {

  def apply(scalaLoaderDir: File): String = {
    val root = scalaLoaderDir.toPath
    val sources = root.resolve("loader/src")
    val underSources: Seq[Path] =
      if (Files.isDirectory(sources)) {
        val walk = Files.walk(sources)
        try walk.iterator().asScala.filter(p => Files.isRegularFile(p)).toList
        finally walk.close()
      } else Nil
    val files = (root.resolve("build.sbt") +: underSources)
      .map(p => root.relativize(p).toString.replace('\\', '/') -> p)
      .filterNot { case (name, _) =>
        name.split('/').exists(c => c.startsWith(".") || c.endsWith("~"))
      }
      .sortBy(_._1)

    val digest = MessageDigest.getInstance("SHA-256")
    for ((name, path) <- files) {
      val content = Files.readAllBytes(path)
      digest.update(name.getBytes("UTF-8"))
      digest.update(s"\n${content.length}\n".getBytes("UTF-8"))
      digest.update(content)
    }
    digest.digest().map(b => "%02x".format(b & 0xff)).mkString
  }
}
