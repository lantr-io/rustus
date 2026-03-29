package rustus.loader

import com.github.plokhotnyuk.jsoniter_scala.core.*
import com.github.plokhotnyuk.jsoniter_scala.macros.*

/** Intermediate types matching the Rust JSON format exactly.
  * These are parsed from JSON, then transformed to scalus SIR types.
  */
object RustusJsonCodec:

  case class RSourcePos(
      file: String,
      start_line: Int,
      start_column: Int,
      end_line: Int,
      end_column: Int
  )

  case class RAnnotationsDecl(
      pos: RSourcePos,
      comment: Option[String] = None,
      data: Map[String, Any] = Map.empty
  )

  case class RTypeBinding(
      name: String,
      tp: RSIRType
  )

  case class RTypeVar(
      name: String,
      opt_id: Option[Long] = None,
      is_builtin: Boolean = false
  )

  case class RConstrDecl(
      name: String,
      params: List[RTypeBinding],
      type_params: List[RTypeVar],
      parent_type_args: List[RSIRType],
      annotations: RAnnotationsDecl
  )

  case class RDataDecl(
      name: String,
      constructors: List[RConstrDecl],
      type_params: List[RTypeVar],
      annotations: RAnnotationsDecl
  )

  // --- Tagged unions: custom codecs for SIRType and SIR ---

  enum RSIRType:
    case Integer
    case Boolean
    case ByteString
    case StringType
    case Unit
    case Data
    case Fun(from: RSIRType, to: RSIRType)
    case SumCaseClass(decl_name: String, type_args: List[RSIRType])
    case CaseClass(constr_name: String, decl_name: String, type_args: List[RSIRType])
    case TypeVar(name: String, opt_id: Option[Long], is_builtin: scala.Boolean)
    case Unresolved

  case class RLetFlags(
      is_rec: scala.Boolean = false,
      is_lazy: scala.Boolean = false
  )

  case class RBinding(
      name: String,
      module_name: Option[String] = None,
      tp: RSIRType,
      value: RSIR
  )

  enum RPattern:
    case Constr(
        constr_name: String,
        decl_name: String,
        bindings: List[String],
        type_params_bindings: List[RSIRType]
    )
    case Wildcard

  case class RCase(
      pattern: RPattern,
      body: RSIR,
      anns: RAnnotationsDecl
  )

  enum RUplcConstant:
    case Integer(value: Long)
    case ByteString(value: List[Int])
    case StringConst(value: String)
    case Bool(value: scala.Boolean)
    case UnitConst

  enum RSIR:
    case Var(name: String, tp: RSIRType, anns: RAnnotationsDecl)
    case ExternalVar(module_name: String, name: String, tp: RSIRType, anns: RAnnotationsDecl)
    case Const(uplc_const: RUplcConstant, tp: RSIRType, anns: RAnnotationsDecl)
    case LamAbs(
        param: RSIR,
        term: RSIR,
        type_params: List[RTypeVar],
        anns: RAnnotationsDecl
    )
    case Apply(f: RSIR, arg: RSIR, tp: RSIRType, anns: RAnnotationsDecl)
    case Let(
        bindings: List[RBinding],
        body: RSIR,
        flags: RLetFlags,
        anns: RAnnotationsDecl
    )
    case Constr(
        name: String,
        data: RDataDecl,
        args: List[RSIR],
        tp: RSIRType,
        anns: RAnnotationsDecl
    )
    case Match(
        scrutinee: RSIR,
        cases: List[RCase],
        tp: RSIRType,
        anns: RAnnotationsDecl
    )
    case IfThenElse(
        cond: RSIR,
        t: RSIR,
        f: RSIR,
        tp: RSIRType,
        anns: RAnnotationsDecl
    )
    case Builtin(builtin_fun: String, tp: RSIRType, anns: RAnnotationsDecl)
    case Error(msg: RSIR, anns: RAnnotationsDecl)
    case Decl(data: RDataDecl, term: RSIR)
    case Select(scrutinee: RSIR, field: String, tp: RSIRType, anns: RAnnotationsDecl)

  case class RModule(
      version: (Int, Int),
      name: String,
      linked: scala.Boolean,
      require_backend: Option[String] = None,
      data_decls: Map[String, RDataDecl],
      defs: List[RBinding],
      anns: RAnnotationsDecl
  )

  // --- jsoniter-scala codecs ---
  // For leaf types, JsonCodecMaker.make works directly.
  // For tagged unions (RSIRType, RSIR, etc.), we need custom codecs
  // since serde uses {"type": "Variant", ...fields...} format.

  given JsonValueCodec[RSourcePos] = JsonCodecMaker.make
  given JsonValueCodec[RTypeVar] = JsonCodecMaker.make
  given JsonValueCodec[RLetFlags] = JsonCodecMaker.make

  // Custom codec for RSIRType (tagged union with "type" discriminator)
  given rsirTypeCodec: JsonValueCodec[RSIRType] = new JsonValueCodec[RSIRType]:
    def nullValue: RSIRType = null

    def decodeValue(in: JsonReader, default: RSIRType): RSIRType =
      val obj = readObjectAsMap(in)
      val tpe = obj("type").asInstanceOf[String]
      tpe match
        case "Integer"    => RSIRType.Integer
        case "Boolean"    => RSIRType.Boolean
        case "ByteString" => RSIRType.ByteString
        case "String"     => RSIRType.StringType
        case "Unit"       => RSIRType.Unit
        case "Data"       => RSIRType.Data
        case "Fun" =>
          RSIRType.Fun(
            parseType(obj("from")),
            parseType(obj("to"))
          )
        case "SumCaseClass" =>
          RSIRType.SumCaseClass(
            obj("decl_name").asInstanceOf[String],
            obj("type_args").asInstanceOf[List[Any]].map(parseType)
          )
        case "CaseClass" =>
          RSIRType.CaseClass(
            obj("constr_name").asInstanceOf[String],
            obj("decl_name").asInstanceOf[String],
            obj("type_args").asInstanceOf[List[Any]].map(parseType)
          )
        case "TypeVar" =>
          RSIRType.TypeVar(
            obj("name").asInstanceOf[String],
            obj.get("opt_id").flatMap(v =>
              if v == null then None else Some(v.asInstanceOf[Number].longValue)
            ),
            obj.getOrElse("is_builtin", false).asInstanceOf[scala.Boolean]
          )
        case other => throw new RuntimeException(s"Unknown SIRType: $other")

    def encodeValue(x: RSIRType, out: JsonWriter): scala.Unit =
      throw new UnsupportedOperationException("Encoding not needed")

  private def parseType(v: Any): RSIRType = v match
    case m: Map[_, _] =>
      val obj = m.asInstanceOf[Map[String, Any]]
      val tpe = obj("type").asInstanceOf[String]
      tpe match
        case "Integer"    => RSIRType.Integer
        case "Boolean"    => RSIRType.Boolean
        case "ByteString" => RSIRType.ByteString
        case "String"     => RSIRType.StringType
        case "Unit"       => RSIRType.Unit
        case "Data"       => RSIRType.Data
        case "Fun" =>
          RSIRType.Fun(parseType(obj("from")), parseType(obj("to")))
        case "SumCaseClass" =>
          RSIRType.SumCaseClass(
            obj("decl_name").asInstanceOf[String],
            obj("type_args").asInstanceOf[List[Any]].map(parseType)
          )
        case "CaseClass" =>
          RSIRType.CaseClass(
            obj("constr_name").asInstanceOf[String],
            obj("decl_name").asInstanceOf[String],
            obj("type_args").asInstanceOf[List[Any]].map(parseType)
          )
        case "TypeVar" =>
          RSIRType.TypeVar(
            obj("name").asInstanceOf[String],
            obj.get("opt_id").flatMap(v =>
              if v == null then None else Some(v.asInstanceOf[Number].longValue)
            ),
            obj.getOrElse("is_builtin", false).asInstanceOf[scala.Boolean]
          )
        case other => throw new RuntimeException(s"Unknown SIRType: $other")
    case _ => throw new RuntimeException(s"Expected map for SIRType, got: $v")

  // Read a JSON object into a Map[String, Any] for manual dispatch
  private def readObjectAsMap(in: JsonReader): Map[String, Any] =
    import scala.collection.mutable
    val result = mutable.Map[String, Any]()
    if in.isNextToken('{') then
      if !in.isNextToken('}') then
        in.rollbackToken()
        while {
          val key = in.readKeyAsString()
          val value = readValue(in)
          result(key) = value
          in.isNextToken(',')
        } do ()
    result.toMap

  private def readValue(in: JsonReader): Any =
    val b = in.nextToken()
    in.rollbackToken()
    b match
      case '"'  => in.readString(null)
      case 't' | 'f' => in.readBoolean()
      case 'n'  => in.readNullOrError(null, "null expected"); null
      case '['  =>
        import scala.collection.mutable.ListBuffer
        val buf = ListBuffer[Any]()
        if in.isNextToken('[') then
          if !in.isNextToken(']') then
            in.rollbackToken()
            while {
              buf += readValue(in)
              in.isNextToken(',')
            } do ()
        buf.toList
      case '{' => readObjectAsMap(in)
      case _   =>
        // Number
        in.readDouble()

  // We'll parse the whole file using ujson-style approach: read into generic map, then extract
  given rmoduleCodec: JsonValueCodec[RModule] = new JsonValueCodec[RModule]:
    def nullValue: RModule = null

    def decodeValue(in: JsonReader, default: RModule): RModule =
      val obj = readObjectAsMap(in)
      val version = obj("version").asInstanceOf[List[Any]]
      val dataDecls = obj("data_decls").asInstanceOf[Map[String, Any]]
      val defs = obj("defs").asInstanceOf[List[Any]]
      val anns = parseAnnotationsDecl(obj("anns"))

      RModule(
        version = (version(0).asInstanceOf[Number].intValue, version(1).asInstanceOf[Number].intValue),
        name = obj("name").asInstanceOf[String],
        linked = obj("linked").asInstanceOf[scala.Boolean],
        require_backend = obj.get("require_backend").flatMap(v =>
          if v == null then None else Some(v.asInstanceOf[String])
        ),
        data_decls = dataDecls.map((k, v) => k -> parseDataDecl(v)),
        defs = defs.map(v => parseBinding(v)),
        anns = anns
      )

    def encodeValue(x: RModule, out: JsonWriter): scala.Unit =
      throw new UnsupportedOperationException("Encoding not needed")

  private def parseAnnotationsDecl(v: Any): RAnnotationsDecl =
    val obj = v.asInstanceOf[Map[String, Any]]
    val posObj = obj("pos").asInstanceOf[Map[String, Any]]
    RAnnotationsDecl(
      pos = RSourcePos(
        file = posObj("file").asInstanceOf[String],
        start_line = posObj("start_line").asInstanceOf[Number].intValue,
        start_column = posObj("start_column").asInstanceOf[Number].intValue,
        end_line = posObj("end_line").asInstanceOf[Number].intValue,
        end_column = posObj("end_column").asInstanceOf[Number].intValue,
      ),
      comment = obj.get("comment").flatMap(v =>
        if v == null then None else Some(v.asInstanceOf[String])
      ),
      data = obj.get("data") match
        case Some(m: Map[_, _]) => m.asInstanceOf[Map[String, Any]]
        case _ => Map.empty
    )

  private def parseTypeVar(v: Any): RTypeVar =
    val obj = v.asInstanceOf[Map[String, Any]]
    RTypeVar(
      name = obj("name").asInstanceOf[String],
      opt_id = obj.get("opt_id").flatMap(v =>
        if v == null then None else Some(v.asInstanceOf[Number].longValue)
      ),
      is_builtin = obj.getOrElse("is_builtin", false).asInstanceOf[scala.Boolean]
    )

  private def parseTypeBinding(v: Any): RTypeBinding =
    val obj = v.asInstanceOf[Map[String, Any]]
    RTypeBinding(
      name = obj("name").asInstanceOf[String],
      tp = parseType(obj("tp"))
    )

  private def parseConstrDecl(v: Any): RConstrDecl =
    val obj = v.asInstanceOf[Map[String, Any]]
    RConstrDecl(
      name = obj("name").asInstanceOf[String],
      params = obj("params").asInstanceOf[List[Any]].map(parseTypeBinding),
      type_params = obj("type_params").asInstanceOf[List[Any]].map(parseTypeVar),
      parent_type_args = obj("parent_type_args").asInstanceOf[List[Any]].map(parseType),
      annotations = parseAnnotationsDecl(obj("annotations"))
    )

  private def parseDataDecl(v: Any): RDataDecl =
    val obj = v.asInstanceOf[Map[String, Any]]
    RDataDecl(
      name = obj("name").asInstanceOf[String],
      constructors = obj("constructors").asInstanceOf[List[Any]].map(parseConstrDecl),
      type_params = obj("type_params").asInstanceOf[List[Any]].map(parseTypeVar),
      annotations = parseAnnotationsDecl(obj("annotations"))
    )

  private def parseBinding(v: Any): RBinding =
    val obj = v.asInstanceOf[Map[String, Any]]
    RBinding(
      name = obj("name").asInstanceOf[String],
      module_name = obj.get("module_name").flatMap(v =>
        if v == null then None else Some(v.asInstanceOf[String])
      ),
      tp = parseType(obj("tp")),
      value = parseSIR(obj("value"))
    )

  private def parseUplcConstant(v: Any): RUplcConstant =
    val obj = v.asInstanceOf[Map[String, Any]]
    obj("type").asInstanceOf[String] match
      case "Integer" =>
        val value = obj("value") match
          case n: Number => n.longValue
          case signAndDigits: List[_] =>
            // num-bigint serde: [sign, [digit, ...]]
            val parts = signAndDigits.asInstanceOf[List[Any]]
            val sign = parts(0).asInstanceOf[Number].intValue
            val digits = parts(1).asInstanceOf[List[Any]].map(_.asInstanceOf[Number].longValue)
            if digits.isEmpty then 0L
            else
              var result = BigInt(0)
              for (d, i) <- digits.zipWithIndex do
                result = result + (BigInt(d) << (32 * i))
              val signed = if sign == 2 then -result else result
              signed.toLong
          case other => throw new RuntimeException(s"Unexpected Integer value: $other")
        RUplcConstant.Integer(value)
      case "ByteString" => RUplcConstant.ByteString(
        obj("value").asInstanceOf[List[Any]].map(_.asInstanceOf[Number].intValue)
      )
      case "String" => RUplcConstant.StringConst(obj("value").asInstanceOf[String])
      case "Bool" => RUplcConstant.Bool(obj("value").asInstanceOf[scala.Boolean])
      case "Unit" => RUplcConstant.UnitConst
      case other => throw new RuntimeException(s"Unknown UplcConstant: $other")

  private def parsePattern(v: Any): RPattern =
    val obj = v.asInstanceOf[Map[String, Any]]
    obj("type").asInstanceOf[String] match
      case "Constr" =>
        RPattern.Constr(
          constr_name = obj("constr_name").asInstanceOf[String],
          decl_name = obj("decl_name").asInstanceOf[String],
          bindings = obj("bindings").asInstanceOf[List[Any]].map(_.asInstanceOf[String]),
          type_params_bindings = obj("type_params_bindings").asInstanceOf[List[Any]].map(parseType)
        )
      case "Wildcard" => RPattern.Wildcard
      case other => throw new RuntimeException(s"Unknown Pattern: $other")

  private def parseCase(v: Any): RCase =
    val obj = v.asInstanceOf[Map[String, Any]]
    RCase(
      pattern = parsePattern(obj("pattern")),
      body = parseSIR(obj("body")),
      anns = parseAnnotationsDecl(obj("anns"))
    )

  private def parseLetFlags(v: Any): RLetFlags =
    val obj = v.asInstanceOf[Map[String, Any]]
    RLetFlags(
      is_rec = obj.getOrElse("is_rec", false).asInstanceOf[scala.Boolean],
      is_lazy = obj.getOrElse("is_lazy", false).asInstanceOf[scala.Boolean]
    )

  def parseSIR(v: Any): RSIR =
    val obj = v.asInstanceOf[Map[String, Any]]
    obj("type").asInstanceOf[String] match
      case "Var" =>
        RSIR.Var(
          name = obj("name").asInstanceOf[String],
          tp = parseType(obj("tp")),
          anns = parseAnnotationsDecl(obj("anns"))
        )
      case "ExternalVar" =>
        RSIR.ExternalVar(
          module_name = obj("module_name").asInstanceOf[String],
          name = obj("name").asInstanceOf[String],
          tp = parseType(obj("tp")),
          anns = parseAnnotationsDecl(obj("anns"))
        )
      case "Const" =>
        RSIR.Const(
          uplc_const = parseUplcConstant(obj("uplc_const")),
          tp = parseType(obj("tp")),
          anns = parseAnnotationsDecl(obj("anns"))
        )
      case "LamAbs" =>
        RSIR.LamAbs(
          param = parseSIR(obj("param")),
          term = parseSIR(obj("term")),
          type_params = obj("type_params").asInstanceOf[List[Any]].map(parseTypeVar),
          anns = parseAnnotationsDecl(obj("anns"))
        )
      case "Apply" =>
        RSIR.Apply(
          f = parseSIR(obj("f")),
          arg = parseSIR(obj("arg")),
          tp = parseType(obj("tp")),
          anns = parseAnnotationsDecl(obj("anns"))
        )
      case "Let" =>
        RSIR.Let(
          bindings = obj("bindings").asInstanceOf[List[Any]].map(parseBinding),
          body = parseSIR(obj("body")),
          flags = parseLetFlags(obj("flags")),
          anns = parseAnnotationsDecl(obj("anns"))
        )
      case "Constr" =>
        RSIR.Constr(
          name = obj("name").asInstanceOf[String],
          data = parseDataDecl(obj("data")),
          args = obj("args").asInstanceOf[List[Any]].map(parseSIR),
          tp = parseType(obj("tp")),
          anns = parseAnnotationsDecl(obj("anns"))
        )
      case "Match" =>
        RSIR.Match(
          scrutinee = parseSIR(obj("scrutinee")),
          cases = obj("cases").asInstanceOf[List[Any]].map(parseCase),
          tp = parseType(obj("tp")),
          anns = parseAnnotationsDecl(obj("anns"))
        )
      case "IfThenElse" =>
        RSIR.IfThenElse(
          cond = parseSIR(obj("cond")),
          t = parseSIR(obj("t")),
          f = parseSIR(obj("f")),
          tp = parseType(obj("tp")),
          anns = parseAnnotationsDecl(obj("anns"))
        )
      case "Builtin" =>
        RSIR.Builtin(
          builtin_fun = obj("builtin_fun").asInstanceOf[String],
          tp = parseType(obj("tp")),
          anns = parseAnnotationsDecl(obj("anns"))
        )
      case "Error" =>
        RSIR.Error(
          msg = parseSIR(obj("msg")),
          anns = parseAnnotationsDecl(obj("anns"))
        )
      case "Decl" =>
        RSIR.Decl(
          data = parseDataDecl(obj("data")),
          term = parseSIR(obj("term"))
        )
      case "Select" =>
        RSIR.Select(
          scrutinee = parseSIR(obj("scrutinee")),
          field = obj("field").asInstanceOf[String],
          tp = parseType(obj("tp")),
          anns = parseAnnotationsDecl(obj("anns"))
        )
      case other => throw new RuntimeException(s"Unknown SIR type: $other")

  // --- Data parsing for eval() arguments ---

  /** Parse a Rust Data enum value from a generic JSON map.
    * Matches the serde format from rustus-core/src/data.rs:
    *   Constr { tag, args }, Map { values }, List { values }, I { value }, B { value }
    */
  def parseRustusData(v: Any): scalus.uplc.builtin.Data =
    import scalus.cardano.onchain.plutus.prelude.{List => ScalusList}
    val obj = v.asInstanceOf[Map[String, Any]]
    obj("type").asInstanceOf[String] match
      case "Constr" =>
        val tag = obj("tag").asInstanceOf[Number].longValue
        val args = obj("args").asInstanceOf[List[Any]].map(parseRustusData)
        scalus.uplc.builtin.Data.Constr(tag, ScalusList.from(args))
      case "I" =>
        // num-bigint serde serializes BigInt as [sign, [digit, ...]]
        // where sign: 0=NoSign, 1=Plus, 2=Minus and digits are u32 values
        val value = obj("value") match
          case s: String => BigInt(s)
          case n: Number => BigInt(n.longValue)
          case signAndDigits: List[_] =>
            val parts = signAndDigits.asInstanceOf[List[Any]]
            val sign = parts(0).asInstanceOf[Number].intValue
            val digits = parts(1).asInstanceOf[List[Any]].map(_.asInstanceOf[Number].longValue)
            if digits.isEmpty then BigInt(0)
            else
              var result = BigInt(0)
              for (d, i) <- digits.zipWithIndex do
                result = result + (BigInt(d) << (32 * i))
              if sign == 2 then -result else result
          case other => throw new RuntimeException(s"Unexpected I value: $other")
        scalus.uplc.builtin.Data.I(value)
      case "B" =>
        val bytes = obj("value").asInstanceOf[List[Any]].map(_.asInstanceOf[Number].byteValue).toArray
        scalus.uplc.builtin.Data.B(scalus.builtin.ByteString.fromArray(bytes))
      case "Map" =>
        val pairs = obj("values").asInstanceOf[List[Any]].map { pair =>
          val arr = pair.asInstanceOf[List[Any]]
          (parseRustusData(arr(0)), parseRustusData(arr(1)))
        }
        scalus.uplc.builtin.Data.Map(ScalusList.from(pairs))
      case "List" =>
        val items = obj("values").asInstanceOf[List[Any]].map(parseRustusData)
        scalus.uplc.builtin.Data.List(ScalusList.from(items))
      case other => throw new RuntimeException(s"Unknown Data type: $other")

  // --- Codec for List[Any] (generic JSON array) ---
  given anyListCodec: JsonValueCodec[List[Any]] = new JsonValueCodec[List[Any]]:
    def nullValue: List[Any] = Nil
    def decodeValue(in: JsonReader, default: List[Any]): List[Any] =
      import scala.collection.mutable.ListBuffer
      val buf = ListBuffer[Any]()
      if in.isNextToken('[') then
        if !in.isNextToken(']') then
          in.rollbackToken()
          while {
            buf += readValue(in)
            in.isNextToken(',')
          } do ()
      buf.toList
    def encodeValue(x: List[Any], out: JsonWriter): scala.Unit =
      throw new UnsupportedOperationException("Encoding not needed")

  // --- EvalResult JSON encoding ---
  case class EvalResultJson(
      success: scala.Boolean,
      cpu: Long,
      mem: Long,
      logs: List[String],
      error: Option[String]
  )

  given evalResultCodec: JsonValueCodec[EvalResultJson] = JsonCodecMaker.make
