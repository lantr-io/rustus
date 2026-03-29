//! Plutus V1 Ledger API types.
//!
//! Matches scalus names under `scalus.cardano.onchain.plutus.v1`.

use rustus_core::num_bigint::BigInt;

use crate::list::List;

// Type aliases matching scalus
pub type Hash = Vec<u8>;
pub type ValidatorHash = Hash;
pub type PolicyId = Vec<u8>;
pub type TokenName = Vec<u8>;
pub type Datum = rustus_core::data::Data;
pub type DatumHash = Hash;
pub type Redeemer = rustus_core::data::Data;
pub type ScriptHash = Hash;
pub type PosixTime = BigInt;
pub type Lovelace = BigInt;
pub type Closure = bool;
pub type PosixTimeRange = Interval;

/// IntervalBoundType — NegInf, Finite(time), PosInf.
/// scalus name: `scalus.cardano.onchain.plutus.v1.IntervalBoundType`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v1.IntervalBoundType")]
pub enum IntervalBoundType {
    NegInf,
    Finite { time: PosixTime },
    PosInf,
}

/// IntervalBound — a bound (lower or upper) of an Interval.
/// scalus name: `scalus.cardano.onchain.plutus.v1.IntervalBound`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v1.IntervalBound")]
pub struct IntervalBound {
    pub bound_type: IntervalBoundType,
    pub is_inclusive: Closure,
}

impl IntervalBound {
    pub fn neg_inf() -> Self {
        IntervalBound {
            bound_type: IntervalBoundType::NegInf,
            is_inclusive: true,
        }
    }

    pub fn pos_inf() -> Self {
        IntervalBound {
            bound_type: IntervalBoundType::PosInf,
            is_inclusive: true,
        }
    }

    pub fn finite_inclusive(time: PosixTime) -> Self {
        IntervalBound {
            bound_type: IntervalBoundType::Finite { time },
            is_inclusive: true,
        }
    }

    pub fn finite_exclusive(time: PosixTime) -> Self {
        IntervalBound {
            bound_type: IntervalBoundType::Finite { time },
            is_inclusive: false,
        }
    }
}

/// Interval — a time interval with lower and upper bounds.
/// scalus name: `scalus.cardano.onchain.plutus.v1.Interval`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v1.Interval")]
pub struct Interval {
    pub from: IntervalBound,
    pub to: IntervalBound,
}

impl Interval {
    /// (-∞, +∞) — always valid
    pub fn always() -> Self {
        Interval {
            from: IntervalBound::neg_inf(),
            to: IntervalBound::pos_inf(),
        }
    }

    /// Empty interval — never valid
    pub fn never() -> Self {
        Interval {
            from: IntervalBound::pos_inf(),
            to: IntervalBound::neg_inf(),
        }
    }

    /// [time, +∞)
    pub fn after(time: PosixTime) -> Self {
        Interval {
            from: IntervalBound::finite_inclusive(time),
            to: IntervalBound::pos_inf(),
        }
    }

    /// (-∞, time]
    pub fn before(time: PosixTime) -> Self {
        Interval {
            from: IntervalBound::neg_inf(),
            to: IntervalBound::finite_inclusive(time),
        }
    }
}

/// Value: a map from PolicyId to (map from TokenName to quantity).
///
/// On-chain represented as just the inner map (no Constr wrapper).
/// scalus name: `scalus.cardano.onchain.plutus.v1.Value`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v1.Value", repr = "one_element")]
pub struct Value {
    pub inner: rustus_core::data::Data,
}

/// TxId: transaction hash.
/// scalus name: `scalus.cardano.onchain.plutus.v1.TxId`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v1.TxId")]
pub struct TxId {
    pub hash: Hash,
}

/// TxOutRef: reference to a transaction output.
/// scalus name: `scalus.cardano.onchain.plutus.v1.TxOutRef`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v1.TxOutRef")]
pub struct TxOutRef {
    pub id: TxId,
    pub idx: BigInt,
}

/// PubKeyHash — on-chain represented as just the ByteString (no Constr wrapper).
/// scalus name: `scalus.cardano.onchain.plutus.v1.PubKeyHash`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v1.PubKeyHash", repr = "one_element")]
pub struct PubKeyHash {
    pub hash: Hash,
}

/// Credential: either a public key or a script.
/// scalus name: `scalus.cardano.onchain.plutus.v1.Credential`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v1.Credential")]
pub enum Credential {
    PubKeyCredential { hash: PubKeyHash },
    ScriptCredential { hash: ValidatorHash },
}

/// StakingCredential
/// scalus name: `scalus.cardano.onchain.plutus.v1.StakingCredential`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v1.StakingCredential")]
pub enum StakingCredential {
    StakingHash { credential: Credential },
    StakingPtr { slot: BigInt, tx_ix: BigInt, cert_ix: BigInt },
}

/// Address
/// scalus name: `scalus.cardano.onchain.plutus.v1.Address`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v1.Address")]
pub struct Address {
    pub credential: Credential,
    pub staking_credential: crate::option::Option<StakingCredential>,
}

/// TxOut: a transaction output.
/// scalus name: `scalus.cardano.onchain.plutus.v1.TxOut`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v1.TxOut")]
pub struct TxOut {
    pub address: Address,
    pub value: Value,
    pub datum_hash: crate::option::Option<DatumHash>,
}

/// TxInInfo: a transaction input with its resolved output.
/// scalus name: `scalus.cardano.onchain.plutus.v1.TxInInfo`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v1.TxInInfo")]
pub struct TxInInfo {
    pub out_ref: TxOutRef,
    pub resolved: TxOut,
}

/// ScriptPurpose
/// scalus name: `scalus.cardano.onchain.plutus.v1.ScriptPurpose`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v1.ScriptPurpose")]
pub enum ScriptPurpose {
    Minting { policy_id: PolicyId },
    Spending { tx_out_ref: TxOutRef },
    Rewarding { staking_credential: StakingCredential },
    Certifying { dcert: Datum },
}

/// TxInfo: V1 transaction info.
/// scalus name: `scalus.cardano.onchain.plutus.v1.TxInfo`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v1.TxInfo")]
pub struct TxInfo {
    pub inputs: List<TxInInfo>,
    pub outputs: List<TxOut>,
    pub fee: Value,
    pub mint: Value,
    pub dcert: List<Datum>,
    pub withdrawals: List<Datum>,
    pub valid_range: PosixTimeRange,
    pub signatories: List<PubKeyHash>,
    pub data: List<Datum>,
    pub id: TxId,
}

/// ScriptContext: V1 script context.
/// scalus name: `scalus.cardano.onchain.plutus.v1.ScriptContext`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v1.ScriptContext")]
pub struct ScriptContext {
    pub tx_info: TxInfo,
    pub purpose: ScriptPurpose,
}
