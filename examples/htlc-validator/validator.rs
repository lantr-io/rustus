use rustus_core::bytestring::ByteString;
use rustus_core::data::{Data, FromData};
use rustus_prelude::builtins;
use rustus_prelude::ledger::v1::PubKeyHash;
use rustus_prelude::ledger::v1::PosixTime;
use rustus_prelude::ledger::v3;
use rustus_prelude::list;
use rustus_prelude::option::Option as PlutusOption;

/// HTLC datum: committer, receiver, hash image, and timeout.
#[derive(Debug, Clone, PartialEq, rustus::ToData, rustus::FromData)]
pub struct Config {
    pub committer: PubKeyHash,
    pub receiver: PubKeyHash,
    pub image: ByteString,
    pub timeout: PosixTime,
}

/// HTLC redeemer: either timeout (reclaim) or reveal preimage (claim).
#[derive(Debug, Clone, PartialEq, rustus::ToData, rustus::FromData)]
pub enum Action {
    Timeout,
    Reveal { preimage: ByteString },
}

/// V3 HTLC validator — single Data argument (ScriptContext).
///
/// - Timeout: committer reclaims, must be signed by committer
/// - Reveal: receiver claims by preimage, must be signed by receiver
#[rustus::compile]
pub fn htlc_validator(sc_data: Data) {
    let ctx: v3::ScriptContext = FromData::from_data(&sc_data).unwrap();
    let action: Action = FromData::from_data(&ctx.redeemer).unwrap();

    match ctx.script_info {
        v3::ScriptInfo::SpendingScript { datum, .. } => {
            let datum_data: Data = match datum {
                PlutusOption::Some { value } => value,
                PlutusOption::None => panic!("Expected datum"),
            };
            let config: Config = FromData::from_data(&datum_data).unwrap();

            match action {
                Action::Timeout => {
                    let signed: bool = list::contains(
                        ctx.tx_info.signatories,
                        config.committer,
                    );
                    rustus_prelude::require!(signed, "Must be signed by committer");
                }
                Action::Reveal { preimage } => {
                    let signed: bool = list::contains(
                        ctx.tx_info.signatories,
                        config.receiver,
                    );
                    rustus_prelude::require!(signed, "Must be signed by receiver");
                    let hash: ByteString = builtins::sha3_256(&preimage);
                    rustus_prelude::require!(hash == config.image, "Invalid preimage");
                }
            }
        }
        _ => {
            panic!("Must be a spending script")
        }
    }
}
