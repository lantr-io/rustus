//! Plutus V3 Ledger API types.
//!
//! Reuses V1 types where unchanged, defines V3-specific ones.
//! Matches scalus names under `scalus.cardano.onchain.plutus.v3`.

use rustus_core::num_bigint::BigInt;

use super::v1::{self, Credential, DatumHash, Hash, Interval, Lovelace, PolicyId, PubKeyHash, TxId, TxOutRef, Value};
use crate::list::List;
use crate::option::Option;
use crate::sorted_map::SortedMap;

pub type Datum = rustus_core::data::Data;
pub type Redeemer = rustus_core::data::Data;
pub type ColdCommitteeCredential = Credential;
pub type HotCommitteeCredential = Credential;
pub type DRepCredential = Credential;

// ---------------------------------------------------------------------------
// Governance types
// ---------------------------------------------------------------------------

/// Vote — No, Yes, or Abstain.
/// scalus name: `scalus.cardano.onchain.plutus.v3.Vote`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.Vote")]
pub enum Vote {
    No,
    Yes,
    Abstain,
}

/// DRep — delegated representative.
/// scalus name: `scalus.cardano.onchain.plutus.v3.DRep`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.DRep")]
pub enum DRep {
    DRep { credential: DRepCredential },
    AlwaysAbstain,
    AlwaysNoConfidence,
}

/// Delegatee — staking delegation target.
/// scalus name: `scalus.cardano.onchain.plutus.v3.Delegatee`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.Delegatee")]
pub enum Delegatee {
    Stake { pub_key_hash: PubKeyHash },
    Vote { d_rep: DRep },
    StakeVote { pub_key_hash: PubKeyHash, d_rep: DRep },
}

/// TxCert — transaction certificate (11 variants).
/// scalus name: `scalus.cardano.onchain.plutus.v3.TxCert`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.TxCert")]
pub enum TxCert {
    RegStaking { credential: Credential, deposit: Option<Lovelace> },
    UnRegStaking { credential: Credential, refund: Option<Lovelace> },
    DelegStaking { credential: Credential, delegatee: Delegatee },
    RegDeleg { credential: Credential, delegatee: Delegatee, deposit: Lovelace },
    RegDRep { credential: DRepCredential, deposit: Lovelace },
    UpdateDRep { credential: DRepCredential },
    UnRegDRep { credential: DRepCredential, refund: Lovelace },
    PoolRegister { pool_id: PubKeyHash, pool_vfr: PubKeyHash },
    PoolRetire { pub_key_hash: PubKeyHash, epoch: BigInt },
    AuthHotCommittee { cold: ColdCommitteeCredential, hot: HotCommitteeCredential },
    ResignColdCommittee { cold: ColdCommitteeCredential },
}

/// Voter — who is voting in governance.
/// scalus name: `scalus.cardano.onchain.plutus.v3.Voter`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.Voter")]
pub enum Voter {
    CommitteeVoter { credential: HotCommitteeCredential },
    DRepVoter { credential: DRepCredential },
    StakePoolVoter { pub_key_hash: PubKeyHash },
}

/// GovernanceActionId — reference to a governance action.
/// scalus name: `scalus.cardano.onchain.plutus.v3.GovernanceActionId`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.GovernanceActionId")]
pub struct GovernanceActionId {
    pub tx_id: TxId,
    pub gov_action_ix: BigInt,
}

/// ProtocolVersion
/// scalus name: `scalus.cardano.onchain.plutus.v3.ProtocolVersion`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.ProtocolVersion")]
pub struct ProtocolVersion {
    pub pv_major: BigInt,
    pub pv_minor: BigInt,
}

/// GovernanceAction — 7 variants.
/// scalus name: `scalus.cardano.onchain.plutus.v3.GovernanceAction`
/// Note: fields that need SortedMap use Datum (raw Data) for now.
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.GovernanceAction")]
pub enum GovernanceAction {
    ParameterChange { id: Option<GovernanceActionId>, parameters: Datum, constitution_script: Option<Hash> },
    HardForkInitiation { id: Option<GovernanceActionId>, protocol_version: ProtocolVersion },
    TreasuryWithdrawals { withdrawals: Datum, constitution_script: Option<Hash> },
    NoConfidence { id: Option<GovernanceActionId> },
    UpdateCommittee { id: Option<GovernanceActionId>, removed_members: List<ColdCommitteeCredential>, added_members: Datum, new_quorum: Datum },
    NewConstitution { id: Option<GovernanceActionId>, constitution: Option<Hash> },
    InfoAction,
}

/// ProposalProcedure
/// scalus name: `scalus.cardano.onchain.plutus.v3.ProposalProcedure`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.ProposalProcedure")]
pub struct ProposalProcedure {
    pub deposit: Lovelace,
    pub return_address: Credential,
    pub governance_action: GovernanceAction,
}

// ---------------------------------------------------------------------------
// V3 ScriptPurpose / ScriptInfo — now with typed fields
// ---------------------------------------------------------------------------

/// V3 ScriptPurpose — extended with governance actions.
/// scalus name: `scalus.cardano.onchain.plutus.v3.ScriptPurpose`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.ScriptPurpose")]
pub enum ScriptPurpose {
    Minting { policy_id: PolicyId },
    Spending { tx_out_ref: TxOutRef },
    Rewarding { credential: Credential },
    Certifying { index: BigInt, cert: TxCert },
    Voting { voter: Voter },
    Proposing { index: BigInt, procedure: ProposalProcedure },
}

/// V3 ScriptInfo
/// scalus name: `scalus.cardano.onchain.plutus.v3.ScriptInfo`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.ScriptInfo")]
pub enum ScriptInfo {
    MintingScript { policy_id: PolicyId },
    SpendingScript { tx_out_ref: TxOutRef, datum: Option<Datum> },
    RewardingScript { credential: Credential },
    CertifyingScript { index: BigInt, cert: TxCert },
    VotingScript { voter: Voter },
    ProposingScript { index: BigInt, procedure: ProposalProcedure },
}

// ---------------------------------------------------------------------------
// V2/V3 shared types
// ---------------------------------------------------------------------------

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

/// TxInfo V3.
/// scalus name: `scalus.cardano.onchain.plutus.v3.TxInfo`
/// Fields needing SortedMap use Datum (raw Data) until SortedMap is implemented.
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.v3.TxInfo")]
pub struct TxInfo {
    pub inputs: List<TxInInfo>,
    pub reference_inputs: List<TxInInfo>,
    pub outputs: List<TxOut>,
    pub fee: Lovelace,
    pub mint: Value,
    pub certificates: List<TxCert>,
    pub withdrawals: SortedMap<Credential, Lovelace>,
    pub valid_range: Interval,
    pub signatories: List<PubKeyHash>,
    pub redeemers: SortedMap<ScriptPurpose, Redeemer>,
    pub data: SortedMap<DatumHash, Datum>,
    pub id: TxId,
    pub votes: SortedMap<Voter, SortedMap<GovernanceActionId, Vote>>,
    pub proposal_procedures: List<ProposalProcedure>,
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
