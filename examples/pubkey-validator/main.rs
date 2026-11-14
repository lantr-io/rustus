//! PubKey Validator — analog of scalus PubKeyValidator.
//!
//! V1 style: 3 Data arguments, explicit fromData conversions.
//!
//! `cargo run --example pubkey-validator -- <out.sir.json>` also writes the SIR JSON
//! that is sent to Scalus, for `sbt "tests/run <out.sir.json>"` to compare against
//! the Scalus-compiled PubKeyValidator.

#[path = "validator.rs"]
mod validator;

const USAGE: &str = "Usage: pubkey-validator [<out.sir.json>]
Compiles pubkey_validator and prints its UPLC, flat size and script hash.
With a path, first writes the SIR JSON sent to Scalus there.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let sir_json = match args.as_slice() {
        [] => None,
        [path] if !path.starts_with('-') => Some(path.as_str()),
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };

    // Built once: the JSON written is the module that is compiled.
    let module = rustus::build_checked_module("pubkey_validator", Default::default())
        .unwrap_or_else(|e| fail(format!("building pubkey_validator: {e}")));
    if let Some(path) = sir_json {
        let json = serde_json::to_string_pretty(&module).expect("serializing the module");
        std::fs::write(path, &json).unwrap_or_else(|e| fail(format!("writing {path}: {e}")));
        println!("Wrote {path} ({} bytes)\n", json.len());
    }

    let validator = rustus::compile_built_module(&module)
        .unwrap_or_else(|e| fail(format!("compiling pubkey_validator: {e}")));
    println!("{}", validator.to_text().unwrap());
    println!("\nFlat: {} bytes", validator.to_flat().unwrap().len());
    let hash = validator.hash().unwrap();
    println!(
        "Script hash: {}",
        hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
}

fn fail(message: String) -> ! {
    eprintln!("Error: {message}");
    std::process::exit(1);
}
