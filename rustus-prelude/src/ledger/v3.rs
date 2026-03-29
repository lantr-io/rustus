//! Plutus V3 Ledger API types.
//!
//! Reuses V1 types where unchanged, defines V3-specific ones.
//! Matches scalus names under `scalus.cardano.onchain.plutus.v3`.

use rustus_core::num_bigint::BigInt;

use super::v1::{self, Credential, DatumHash, Hash, Interval, Lovelace, PolicyId, PubKeyHash, TxId, TxOutRef, Value};
use crate::list::List;
use crate::option::Option;

pub type Datum = rustus_core::data::Data;
pub type Redeemer = rustus_core::data::Data;

/// V3 ScriptPurpose — extended with governance actions.
/// scalus name: `scalus.cardano.onchain.plutus.v3.ScriptPurpose`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.ScriptPurpose")]
pub enum ScriptPurpose {
    Minting { policy_id: PolicyId },
    Spending { tx_out_ref: TxOutRef },
    Rewarding { credential: Credential },
    Certifying { index: BigInt, cert: Datum },
    Voting { voter: Datum },
    Proposing { index: BigInt, procedure: Datum },
}

/// V3 ScriptInfo
/// scalus name: `scalus.cardano.onchain.plutus.v3.ScriptInfo`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.ScriptInfo")]
pub enum ScriptInfo {
    MintingScript { policy_id: PolicyId },
    SpendingScript { tx_out_ref: TxOutRef, datum: Option<Datum> },
    RewardingScript { credential: Credential },
    CertifyingScript { index: BigInt, cert: Datum },
    VotingScript { voter: Datum },
    ProposingScript { index: BigInt, procedure: Datum },
}

/// OutputDatum — V2/V3 datum attachment.
/// scalus name: `scalus.cardano.onchain.plutus.v2.OutputDatum`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v2.OutputDatum")]
pub enum OutputDatum {
    NoOutputDatum,
    OutputDatumHash { hash: DatumHash },
    OutputDatum { datum: Datum },
}

/// TxOut V2/V3 — with OutputDatum and optional reference script.
/// scalus name: `scalus.cardano.onchain.plutus.v2.TxOut`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v2.TxOut")]
pub struct TxOut {
    pub address: v1::Address,
    pub value: Value,
    pub datum: OutputDatum,
    pub reference_script: Option<Hash>,
}

/// TxInInfo V3 — uses V2 TxOut.
/// scalus name: `scalus.cardano.onchain.plutus.v3.TxInInfo`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.TxInInfo")]
pub struct TxInInfo {
    pub out_ref: TxOutRef,
    pub resolved: TxOut,
}

/// TxInfo V3 — the full transaction info for PlutusV3 scripts.
/// scalus name: `scalus.cardano.onchain.plutus.v3.TxInfo`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.TxInfo")]
pub struct TxInfo {
    pub inputs: List<TxInInfo>,
    pub reference_inputs: List<TxInInfo>,
    pub outputs: List<TxOut>,
    pub fee: Lovelace,
    pub mint: Value,
    pub certificates: List<Datum>,
    pub withdrawals: Datum,
    pub valid_range: Interval,
    pub signatories: List<PubKeyHash>,
    pub redeemers: Datum,
    pub data: Datum,
    pub id: TxId,
    pub votes: Datum,
    pub proposal_procedures: List<Datum>,
    pub current_treasury_amount: Option<Lovelace>,
    pub treasury_donation: Option<Lovelace>,
}

/// ScriptContext V3.
/// scalus name: `scalus.cardano.onchain.plutus.v3.ScriptContext`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.ScriptContext")]
pub struct ScriptContext {
    pub tx_info: TxInfo,
    pub redeemer: Redeemer,
    pub script_info: ScriptInfo,
}
