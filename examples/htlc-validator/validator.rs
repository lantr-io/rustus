use rustus_core::bytestring::ByteString;
use rustus_core::data::{Data, FromData};
use rustus_core::num_bigint::BigInt;
use rustus_prelude::builtins;
use rustus_prelude::ledger::v1::{IntervalBoundType, PubKeyHash, PosixTime};
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
/// - Timeout: committer reclaims after timeout, signed by committer
/// - Reveal: receiver claims before timeout by preimage, signed by receiver
#[rustus::compile]
pub fn htlc_validator(sc_data: Data) {
    let ctx: v3::ScriptContext = FromData::from_data(&sc_data).unwrap();
    let action: Action = FromData::from_data(&ctx.redeemer).unwrap();

    match ctx.script_info {
        v3::ScriptInfo::SpendingScript { tx_out_ref: _, datum } => {
            let datum_data: Data = match datum {
                PlutusOption::Some { value } => value,
                PlutusOption::None => panic!("Expected datum"),
            };
            let config: Config = FromData::from_data(&datum_data).unwrap();

            match action {
                Action::Timeout => {
                    // Must be after timeout
                    let valid_from: PosixTime = match ctx.tx_info.valid_range.from.bound_type {
                        IntervalBoundType::Finite { time } => time,
                        _ => BigInt::from(0),
                    };
                    rustus_prelude::require!(config.timeout <= valid_from, "Must be after timeout");
                    // Must be signed by committer
                    let signed: bool = list::contains(
                        ctx.tx_info.signatories,
                        config.committer,
                    );
                    rustus_prelude::require!(signed, "Must be signed by committer");
                }
                Action::Reveal { preimage } => {
                    // Must be before timeout
                    let valid_to: PosixTime = match ctx.tx_info.valid_range.to.bound_type {
                        IntervalBoundType::Finite { time } => time,
                        _ => panic!("ValidTo must be set"),
                    };
                    rustus_prelude::require!(valid_to <= config.timeout, "Must be before timeout");
                    // Must be signed by receiver
                    let signed: bool = list::contains(
                        ctx.tx_info.signatories,
                        config.receiver,
                    );
                    rustus_prelude::require!(signed, "Must be signed by receiver");
                    // Preimage must match
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
