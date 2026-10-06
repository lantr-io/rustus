# Rustus

Write Cardano smart contracts in Rust.

Rustus is a Rust frontend for [Scalus](https://scalus.org). You write a validator as an ordinary Rust function, annotate it with `#[rustus::compile]`, and Rustus hands it to the Scalus compiler, which produces a Plutus V3 script. The function stays plain Rust too, so you can call it directly in unit tests, or evaluate the compiled script in the Plutus CEK machine without leaving `cargo test`.

Rustus is at an early stage (0.1.0): `#[compile]` accepts a subset of Rust, and the API will change.

## Example

A validator that succeeds only if the transaction is signed by the owner named in the datum:

```rust
use rustus_core::data::{Data, FromData};
use rustus_prelude::ledger::v1::{PubKeyHash, ScriptContext};
use rustus_prelude::list;

#[derive(Debug, Clone, PartialEq, rustus::ToData, rustus::FromData)]
pub struct OwnerDatum {
    pub owner: PubKeyHash,
}

#[rustus::compile]
pub fn pubkey_validator(datum: Data, _redeemer: Data, ctx: Data) {
    let owner_datum: OwnerDatum = FromData::from_data(&datum).unwrap();
    let script_ctx: ScriptContext = FromData::from_data(&ctx).unwrap();
    let signed: bool = list::contains(script_ctx.tx_info.signatories, owner_datum.owner);
    rustus_prelude::require!(signed, "Not signed by owner")
}
```

Compile it and use the result:

```rust
let validator = rustus::compile_module("pubkey_validator")?;

let flat = validator.to_flat()?;   // flat-encoded UPLC, ready for on-chain use
let hash = validator.hash()?;      // script hash
println!("{}", validator.to_text()?);

// Run it in the CEK machine with Data arguments
let result = validator.eval(&[datum, Data::unit(), ctx])?;
assert!(result.succeeded());
println!("cpu {} mem {} logs {:?}", result.cpu, result.mem, result.logs);
```

The full version, along with a hash-preimage validator and an HTLC, is in [`examples/`](examples/), each with tests.

## Getting started

You need Rust 1.85+, a JDK (11 or newer), and [sbt](https://www.scala-sbt.org/).

```bash
git clone https://github.com/lantr-io/rustus.git
cd rustus

# macOS: JAVA_HOME has to be set explicitly
export JAVA_HOME=$(/usr/libexec/java_home)

# Build the Scalus JAR that Rustus loads at runtime (once, and after changing scala-loader/)
(cd scala-loader && sbt loader/assembly)

cargo test
cargo run --example htlc-validator
```

Rustus finds the JAR in `scala-loader/loader/target/` when run from this repository. Elsewhere, set `RUSTUS_JAR` to its path or copy it to `~/.rustus/lib/rustus-scalus.jar`.

## How it works

1. The `#[compile]` macro translates the function body into an untyped intermediate tree and registers it, together with the types it uses.
2. When your program calls `compile_module`, Rustus resolves types and lowers that tree to Scalus SIR.
3. The SIR is passed as JSON to Scalus running in an embedded JVM, which links it, lowers it to UPLC, and can evaluate it.

## Repository layout

| Path | Contents |
|---|---|
| `src/` | `rustus` crate: the public API, re-exporting the crates below |
| `rustus-macros/` | `#[compile]`, `#[module]`, and the `ToData` / `FromData` derives |
| `rustus-core/` | SIR data model, lowering, typing |
| `rustus-prelude/` | On-chain standard library: `List`, `Option`, maps, ledger types, builtins |
| `rustus-jvm/` | JNI bridge to Scalus |
| `scala-loader/` | Scala side: reads the SIR JSON and drives Scalus; builds the JAR |
| `examples/` | Validators with tests, plus smaller development examples |
