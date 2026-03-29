//! PubKey Validator — analog of scalus PubKeyValidator.
//!
//! V1 style: 3 Data arguments, explicit fromData conversions.

#[path = "validator.rs"]
mod validator;

fn main() {
    match rustus::compile_module("pubkey_validator") {
        Ok(validator) => {
            println!("{}", validator.to_text().unwrap());
            println!("\nFlat: {} bytes", validator.to_flat().unwrap().len());
            let hash = validator.hash().unwrap();
            println!(
                "Script hash: {}",
                hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
            );
        }
        Err(e) => eprintln!("Error: {e}"),
    }
}
