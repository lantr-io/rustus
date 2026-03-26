package rustus.loader

import rustus.loader.RustusJsonCodec.*
import scalus.compiler.sir.*
import scalus.compiler.sir.SIR.{Case, Pattern}
import scalus.uplc.{Constant, DefaultFun}

/** Transform rustus intermediate types (R*) to scalus native SIR types.
  *
  * Two-pass approach:
  *   1. Build symbol table: parse data_decls into scalus DataDecl objects
  *   2. Walk SIR tree, resolving decl_name references to actual DataDecl objects
  */
object RustusToScalus:

  case class TransformResult(
      module: Module,
      mainBinding: Option[Binding]
  )

  def transform(rmodule: RModule): TransformResult =
    // Build symbol table using mutable map to handle forward references.
    // Pass 1: create DataDecl stubs (no constructor params resolved yet)
    val stubTable = scala.collection.mutable.Map[String, DataDecl]()
    for (name, rdecl) <- rmodule.data_decls do
      stubTable(name) = DataDecl(
        name = rdecl.name,
        constructors = rdecl.constructors.map(rc =>
          ConstrDecl(rc.name, Nil, rc.type_params.map(convertTypeVar), Nil, emptyAnns)
        ),
        typeParams = rdecl.type_params.map(convertTypeVar),
        annotations = emptyAnns
      )

    // Pass 2: rebuild with resolved constructor params, using stubs for cross-refs
    val symbolTable: Map[String, DataDecl] =
      rmodule.data_decls.map { (name, rdecl) =>
        name -> DataDecl(
          name = rdecl.name,
          constructors = rdecl.constructors.map(rc => convertConstrDecl(rc, stubTable.toMap)),
          typeParams = rdecl.type_params.map(convertTypeVar),
          annotations = emptyAnns
        )
      }

    // Pass 3: convert bindings with resolved types
    // Strip Decl wrappers from binding values — DataDecls are already in the symbol table,
    // and the linker expects binding values to be AnnotatedSIR (not Decl).
    val bindings = rmodule.defs.map { rb =>
      Binding(
        name = rb.name,
        tp = convertSIRType(rb.tp, symbolTable),
        value = stripDecls(convertSIR(rb.value, symbolTable))
      )
    }

    val module = Module(
      version = rmodule.version,
      name = rmodule.name,
      linked = false,
      requireBackend = None,
      defs = bindings
    )

    // Main binding = last non-module binding (the user's validator)
    val mainBinding = bindings.findLast(b =>
      rmodule.defs.find(_.name == b.name).flatMap(_.module_name).isEmpty
    )
    TransformResult(module, mainBinding)

  private def convertConstrDecl(
      rc: RConstrDecl,
      symbolTable: => Map[String, DataDecl]
  ): ConstrDecl =
    ConstrDecl(
      name = rc.name,
      params = rc.params.map(tb =>
        TypeBinding(tb.name, convertSIRType(tb.tp, symbolTable))
      ),
      typeParams = rc.type_params.map(convertTypeVar),
      parentTypeArgs = rc.parent_type_args.map(t => convertSIRType(t, symbolTable)),
      annotations = emptyAnns
    )

  // --- SIRType conversion ---

  def convertSIRType(
      rt: RSIRType,
      symbolTable: => Map[String, DataDecl]
  ): SIRType =
    rt match
      case RSIRType.Integer    => SIRType.Integer
      case RSIRType.Boolean    => SIRType.Boolean
      case RSIRType.ByteString => SIRType.ByteString
      case RSIRType.StringType => SIRType.String
      case RSIRType.Unit       => SIRType.Unit
      case RSIRType.Data       => SIRType.Data()
      case RSIRType.Fun(from, to) =>
        SIRType.Fun(convertSIRType(from, symbolTable), convertSIRType(to, symbolTable))
      case RSIRType.SumCaseClass(declName, typeArgs) =>
        val decl = symbolTable.getOrElse(
          declName,
          throw new RuntimeException(s"Unknown data decl: $declName")
        )
        SIRType.SumCaseClass(decl, typeArgs.map(t => convertSIRType(t, symbolTable)))
      case RSIRType.CaseClass(constrName, declName, typeArgs) =>
        val decl = symbolTable.getOrElse(
          declName,
          throw new RuntimeException(s"Unknown data decl: $declName")
        )
        val constr = decl.constructors.find(_.name == constrName).getOrElse(
          throw new RuntimeException(s"Unknown constructor: $constrName in $declName")
        )
        val convertedTypeArgs = typeArgs.map(t => convertSIRType(t, symbolTable))
        // Single constructor with same name as DataDecl → product type (no parent)
        val isCaseClass = decl.constructors.length == 1 && constr.name == decl.name
        val parent = if isCaseClass then None else Some(SIRType.SumCaseClass(decl, convertedTypeArgs))
        SIRType.CaseClass(constr, convertedTypeArgs, parent)
      case RSIRType.TypeVar(name, optId, isBuiltin) =>
        SIRType.TypeVar(name, optId, isBuiltin)

  // --- SIR conversion ---

  def convertSIR(
      rsir: RSIR,
      symbolTable: Map[String, DataDecl]
  ): SIR =
    rsir match
      case RSIR.Var(name, tp, _) =>
        SIR.Var(name, convertSIRType(tp, symbolTable), emptyAnns)

      case RSIR.ExternalVar(moduleName, name, tp, _) =>
        SIR.ExternalVar(moduleName, name, convertSIRType(tp, symbolTable), emptyAnns)

      case RSIR.Const(uplcConst, tp, _) =>
        SIR.Const(convertConstant(uplcConst), convertSIRType(tp, symbolTable), emptyAnns)

      case RSIR.LamAbs(param, term, typeParams, _) =>
        val paramVar = convertSIR(param, symbolTable) match
          case v: SIR.Var => v
          case other => throw new RuntimeException(s"LamAbs param must be Var, got: $other")
        SIR.LamAbs(
          param = paramVar,
          term = convertSIR(term, symbolTable),
          typeParams = typeParams.map(convertTypeVar),
          anns = emptyAnns
        )

      case RSIR.Apply(f, arg, tp, _) =>
        SIR.Apply(
          f = asAnnotated(convertSIR(f, symbolTable)),
          arg = asAnnotated(convertSIR(arg, symbolTable)),
          tp = convertSIRType(tp, symbolTable),
          anns = emptyAnns
        )

      case RSIR.Let(bindings, body, flags, _) =>
        val letFlags =
          if flags.is_rec && flags.is_lazy then SIR.LetFlags.Recursivity | SIR.LetFlags.Lazy
          else if flags.is_rec then SIR.LetFlags.Recursivity
          else if flags.is_lazy then SIR.LetFlags.Lazy
          else SIR.LetFlags.None
        SIR.Let(
          bindings = bindings.map(rb =>
            Binding(rb.name, convertSIRType(rb.tp, symbolTable), convertSIR(rb.value, symbolTable))
          ),
          body = convertSIR(body, symbolTable),
          flags = letFlags,
          anns = emptyAnns
        )

      case RSIR.Constr(name, data, args, tp, _) =>
        val decl = convertDataDeclInline(data, symbolTable)
        SIR.Constr(
          name = name,
          data = decl,
          args = args.map(a => convertSIR(a, symbolTable)),
          tp = convertSIRType(tp, symbolTable),
          anns = emptyAnns
        )

      case RSIR.Match(scrutinee, cases, tp, _) =>
        SIR.Match(
          scrutinee = asAnnotated(convertSIR(scrutinee, symbolTable)),
          cases = cases.map(c => convertCase(c, symbolTable)),
          tp = convertSIRType(tp, symbolTable),
          anns = emptyAnns
        )

      case RSIR.IfThenElse(cond, t, f, tp, _) =>
        SIR.IfThenElse(
          cond = asAnnotated(convertSIR(cond, symbolTable)),
          t = asAnnotated(convertSIR(t, symbolTable)),
          f = asAnnotated(convertSIR(f, symbolTable)),
          tp = convertSIRType(tp, symbolTable),
          anns = emptyAnns
        )

      case RSIR.Builtin(builtinFun, tp, _) =>
        val bf = DefaultFun.valueOf(builtinFun)
        SIR.Builtin(bf, convertSIRType(tp, symbolTable), emptyAnns)

      case RSIR.Error(msg, _) =>
        SIR.Error(asAnnotated(convertSIR(msg, symbolTable)), emptyAnns)

      case RSIR.Decl(data, term) =>
        val decl = convertDataDeclInline(data, symbolTable)
        SIR.Decl(decl, convertSIR(term, symbolTable))

      case RSIR.Select(scrutinee, field, tp, _) =>
        SIR.Select(
          scrutinee = convertSIR(scrutinee, symbolTable),
          field = field,
          tp = convertSIRType(tp, symbolTable),
          anns = emptyAnns
        )

  // --- Helper conversions ---

  private def convertCase(rc: RCase, symbolTable: Map[String, DataDecl]): Case =
    Case(
      pattern = convertPattern(rc.pattern, symbolTable),
      body = convertSIR(rc.body, symbolTable),
      anns = emptyAnns
    )

  private def convertPattern(
      rp: RPattern,
      symbolTable: Map[String, DataDecl]
  ): Pattern =
    rp match
      case RPattern.Wildcard => Pattern.Wildcard
      case RPattern.Constr(constrName, declName, bindings, typeParamsBindings) =>
        // Try exact match, then suffix match (Rust short names vs scalus full names)
        val decl = symbolTable.get(declName).orElse(
          symbolTable.values.find(d => d.name.endsWith(s".$declName") || d.name.endsWith(s"$declName"))
        ).getOrElse(
          throw new RuntimeException(s"Unknown data decl in pattern: $declName (available: ${symbolTable.keys.mkString(", ")})")
        )
        // Find constructor: exact match, then suffix match
        val constr = decl.constructors.find(_.name == constrName).orElse {
          val suffix = constrName.split("::").last
          decl.constructors.find(c => c.name.endsWith(s"$$.$suffix"))
        }.getOrElse(
          throw new RuntimeException(s"Unknown constructor in pattern: $constrName in ${decl.name}")
        )
        // Pad bindings to match constructor param count (Rust `..` omits trailing fields)
        val fullBindings = if bindings.length < constr.params.length then
          bindings ++ List.fill(constr.params.length - bindings.length)("_")
        else bindings
        Pattern.Constr(
          constr = constr,
          bindings = fullBindings,
          typeParamsBindings = typeParamsBindings.map(t => convertSIRType(t, symbolTable))
        )

  private def convertConstant(rc: RUplcConstant): Constant =
    rc match
      case RUplcConstant.Integer(v)    => Constant.Integer(BigInt(v))
      case RUplcConstant.Bool(v)       => Constant.Bool(v)
      case RUplcConstant.StringConst(v) => Constant.String(v)
      case RUplcConstant.ByteString(v) =>
        Constant.ByteString(scalus.builtin.ByteString.fromArray(v.map(_.toByte).toArray))
      case RUplcConstant.UnitConst     => Constant.Unit

  private def convertTypeVar(rv: RTypeVar): SIRType.TypeVar =
    SIRType.TypeVar(rv.name, rv.opt_id, rv.is_builtin)

  private def convertDataDeclInline(
      rd: RDataDecl,
      symbolTable: Map[String, DataDecl]
  ): DataDecl =
    // If this DataDecl is already in the symbol table, use that version
    symbolTable.getOrElse(
      rd.name,
      DataDecl(
        name = rd.name,
        constructors = rd.constructors.map(rc => convertConstrDecl(rc, symbolTable)),
        typeParams = rd.type_params.map(convertTypeVar),
        annotations = emptyAnns
      )
    )

  /** Strip Decl wrappers — unwrap to the inner term */
  @scala.annotation.tailrec
  private def stripDecls(sir: SIR): SIR =
    sir match
      case SIR.Decl(_, term) => stripDecls(term)
      case other => other

  /** Wrap a SIR in AnnotatedSIR if it isn't already */
  private def asAnnotated(sir: SIR): AnnotatedSIR =
    sir match
      case a: AnnotatedSIR => a
      case SIR.Decl(_, term) =>
        asAnnotated(term)

  private val emptyAnns = AnnotationsDecl(SIRPosition.empty)
