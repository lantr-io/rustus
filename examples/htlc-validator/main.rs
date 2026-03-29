//! HTLC (Hash Time-Locked Contract) Validator.
//!
//! Allows a receiver to claim funds by revealing a hash preimage,
//! or the committer to reclaim after a timeout.

#[path = "validator.rs"]
mod validator;

fn main() {
    match rustus::compile_module("htlc_validator") {
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
