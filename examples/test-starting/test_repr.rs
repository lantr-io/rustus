use rustus_core::data::{Data, ToData, FromData};
use rustus_prelude::ledger::v1::PubKeyHash;

fn main() {
    // PubKeyHash with one_element repr: should encode as just B(hash), not Constr(0, [B(hash)])
    let pkh = PubKeyHash { hash: vec![0xde, 0xad, 0xbe, 0xef] };

    let data = pkh.to_data();
    println!("PubKeyHash.to_data() = {:?}", data);

    // Should be B([de, ad, be, ef]), NOT Constr(0, [B([de, ad, be, ef])])
    match &data {
        Data::B { value } => println!("OK: raw ByteString, no Constr wrapper"),
        Data::Constr { .. } => println!("WRONG: has Constr wrapper (one_element not working)"),
        other => println!("WRONG: unexpected {:?}", other),
    }

    // Roundtrip
    let back = PubKeyHash::from_data(&data).unwrap();
    assert_eq!(pkh, back);
    println!("Roundtrip OK");
}
