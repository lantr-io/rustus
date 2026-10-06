# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Rustus is a Rust frontend for the Scalus Cardano smart-contract compiler. Validators are written as ordinary Rust functions annotated with `#[rustus::compile]`; Rustus turns them into Scalus SIR, and Scalus (running in an embedded JVM) links, lowers to UPLC, and evaluates them. The same function also runs as plain Rust for off-chain testing.

## Commands

Requires a Rust toolchain (edition 2024), a JDK, and sbt.

```bash
# JAVA_HOME must be set on macOS: the `which java` fallback resolves to the
# /usr/bin/java stub, and every test then fails with "JVM not found".
export JAVA_HOME=$(/usr/libexec/java_home)

# Build the Scalus uber-JAR. Needed by anything that calls rustus::compile_module
# (all cargo tests, the validator examples). Rebuild after changing
# scala-loader/loader or the Scalus version — cargo does not track it.
cd scala-loader && sbt loader/assembly

cargo build
cargo test                                                    # all integration tests
cargo test --test htlc-validator-tests                        # one test binary
cargo test --test htlc-validator-tests reveal_wrong_preimage  # one test

cargo run --example htlc-validator   # compile and print UPLC, flat size, script hash
cargo run --example test_typing      # most scratch examples build SIR Rust-side only, no JVM

./run-e2e.sh                         # example `validator` -> my_validator.sir.json -> sbt loader/run

# Side-by-side: Scalus-compiled PubKeyValidator vs. a Rust-generated SIR JSON
cd scala-loader && sbt "tests/run <path-to.sir.json>"
```

`cargo test` runs the root package only; the member crates have no tests of their own. Tests and examples live at non-standard paths under `examples/`, so each new one needs a `[[test]]` / `[[example]]` entry in the root `Cargo.toml`.

The JAR is looked up in this order: `$RUSTUS_JAR`, `scala-loader/loader/target/scala-3.3.7/rustus-scalus.jar`, `~/.rustus/lib/rustus-scalus.jar`.

## Architecture

A proc macro cannot resolve types, so compilation is split into three stages, each in a different place:

**1. Macro expansion — `rustus-macros`.** `#[compile]` leaves the function intact and emits a hidden builder `__rustus_compile_<fn>` that constructs a `PreSIR` tree (`rustus-core/src/pre_sir.rs`: untyped, syntactic) plus a `TypeDict` filled with `<T as HasSIRType>::sir_type()` calls. `#[derive(ToData, FromData)]` generates the `Data` conversions, a `HasSIRType` impl (the type's `DataDecl`), and `OnchainPartialEq`. Both register themselves through `inventory::submit!` as `PreSirEntry { kind: TypeDecl | Function }`.

**2. Builder time, inside the user's binary — `rustus-core`.** `registry::build_module(name)` runs every registered entry: type decls first, then functions. `lower.rs` converts `PreSIR` to `SIR` (resolving calls to `ExternalVar`, choosing `Equals*`/`LessThan*` builtins from operand types, threading typeclass arguments), then `typing.rs` renumbers type variables and fills in every `SIRType::Unresolved`. Typing errors here are only printed to stderr; `rustus::compile_module` (root `src/lib.rs`) is what turns leftover `Unresolved` types into an `Err`.

**3. Scalus, over JNI — `rustus-jvm` + `scala-loader/loader`.** The `Module` is serialized with serde to JSON and handed to `RustusLoader.compile(String)`. `RustusJsonCodec` decodes it into `R*` mirror types, `RustusToScalus` converts those to real Scalus SIR, then Scalus's `SIRLinker` and `SirToUplcV3Lowering` produce the UPLC term. Rust holds the resulting `CompiledValidator` as a JNI global ref and calls `toFlat` / `toText` / `eval` / `scriptHash` on it (`Validator` in `rustus-jvm/src/lib.rs`). The JVM is created once per process.

The root `rustus` crate is a facade re-exporting the three crates' public API. `rustus-prelude` holds the on-chain standard library: Scalus-compatible `List`, `Option`, `SortedMap`, `AssocMap`, ledger types (`ledger::v1`, `ledger::v3`), and `builtins`.

## Things that will bite you

- **The registry is process-global.** Every `#[compile]` function and derived type linked into a binary ends up in one module, so two types with the same name in one binary collide. This is why each validator gets its own test and example binary, sharing code through `#[path = "validator.rs"] mod validator;`.
- **The module name selects the entry point.** The loader takes as main the binding whose name equals the module name, falling back to the last binding without a `module_name`. `compile_module("htlc_validator")` therefore has to match the function name.
- **The JSON shape is a three-way contract.** Changing the serde layout of `sir.rs`, `sir_type.rs`, `module.rs`, `constant.rs`, or `data.rs` requires matching edits in `RustusJsonCodec.scala` and `RustusToScalus.scala`, then a JAR rebuild.
- **Scalus names are load-bearing.** Prelude types and functions carry Scalus's fully-qualified names (`#[rustus(name = "scalus.cardano.onchain.plutus.prelude.List", repr = "list")]`, `#[rustus_module("...prelude.List$")]`) so that Scalus's intrinsics and representation handling apply to them. `repr = "one_element"` / `"map"` are emitted as `uplcRepr` annotations (`ProductCaseOneElement`, `PackedDataMap`) whose strings must match what Scalus's `SirTypeUplcGenerator.resolveUplcRepresentation` dispatches on — check these on every Scalus upgrade.
- **Version pins live in two places.** `scalusVersion` and `scalaVersion` are in `scala-loader/build.sbt`; the Scala version is also hard-coded in the JAR path in `rustus-jvm/src/jar.rs`.
- **Macro output names `rustus_core::…` directly**, so a crate using `#[rustus::compile]` or the derives must depend on `rustus-core` itself, not only on `rustus`.
- **`#[compile]` accepts a small Rust subset** — `let`, `match` on constructors or `_`, `if`, field access, calls, `+ - * == < <=`, unary `-`, `BigInt::from(<literal>)`, `FromData::from_data(&x).unwrap()`, `.to_data()`, `panic!`, `require!`; `&` and `*` are transparent. Anything else is a `compile_error!("#[compile] unsupported …")` from `rustus-macros/src/compile.rs`. Generic bounds `T: PartialEq` / `T: PartialOrd` become extra function arguments.
- **Adding a UPLC builtin touches three files:** the Rust implementation in `rustus-prelude/src/builtins.rs`, the `resolve_builtin` table in `rustus-core/src/lower.rs`, and `DefaultFun` in `rustus-core/src/default_fun.rs`. Calls must be written with the `builtins::` prefix to be recognized.

## Debugging

To inspect generated SIR without the JVM, call `rustus_core::registry::build_module(name)` and dump it with `serde_json::to_string_pretty` — the `examples/test-starting/` programs do this. `*.sir.json` files are gitignored. `sbt "loader/run <file.sir.json>"` prints the converted SIR, the linked SIR, and the UPLC for such a file.

`examples/test-starting/` is the scratch corpus that exercises the SIR emitters; `compile_tests.rs` there drives a subset through Scalus so backend regressions surface in `cargo test`.
