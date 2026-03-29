//! Preimage Validator — analog of scalus PreimageValidator.
//!
//! Validates that a preimage hashes to a given hash and that
//! the transaction is signed by a specific public key hash.

#[path = "validator.rs"]
mod validator;

fn main() {
    match rustus::compile_module("preimage_validator") {
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
