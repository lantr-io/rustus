//! Data encodings of ledger types, checked against the shapes Plutus and Scalus use.
//! A wrong shape still compiles and passes every validator test, but a validator then
//! decodes real script contexts wrongly, so the shapes are pinned here. Rust-side only, no JVM.

use rustus_core::bytestring::ByteString;
use rustus_core::data::{Data, FromData, ToData};
use rustus_core::num_bigint::BigInt;
use rustus_prelude::ledger::v1::{Credential, PubKeyHash};
use rustus_prelude::ledger::v3::{Constitution, GovernanceAction};
use rustus_prelude::list::List;
use rustus_prelude::option::Option as POption;
use rustus_prelude::pair::Pair;
use rustus_prelude::sorted_map::SortedMap;

fn nothing() -> Data {
    Data::Constr { tag: 1, args: vec![] }
}

fn just(value: Data) -> Data {
    Data::Constr { tag: 0, args: vec![value] }
}

fn bytes(b: &[u8]) -> Data {
    Data::B { value: b.to_vec() }
}

fn int(i: i64) -> Data {
    Data::I { value: BigInt::from(i) }
}

fn single<K: Clone, V: Clone>(key: K, value: V) -> SortedMap<K, V> {
    SortedMap { inner: List::from_vec(vec![Pair::new(key, value)]) }
}

/// Encodes `action`, checks the result against `expected`, and decodes it back.
fn assert_encoding(action: GovernanceAction, expected: Data) {
    let data = action.to_data();
    assert_eq!(data, expected);
    assert_eq!(GovernanceAction::from_data(&data).unwrap(), action);
}

/// Plutus `Constitution` is a one-field constructor, so `NewConstitution` carries
/// `Constr 0 [Maybe ScriptHash]`, not the `Maybe` itself (Scalus 1.5.0 follows this).
#[test]
fn new_constitution_wraps_the_script_in_a_constitution() {
    let script = ByteString::from(vec![0xef]);
    assert_encoding(
        GovernanceAction::NewConstitution {
            id: POption::None,
            constitution: Constitution { constitution_script: POption::Some { value: script } },
        },
        Data::Constr { tag: 5, args: vec![nothing(), just(just(bytes(&[0xef])))] },
    );
    assert_encoding(
        GovernanceAction::NewConstitution {
            id: POption::None,
            constitution: Constitution { constitution_script: POption::None },
        },
        Data::Constr { tag: 5, args: vec![nothing(), just(nothing())] },
    );
}

/// `withdrawals` is a `Map Credential Lovelace`.
#[test]
fn treasury_withdrawals_is_a_credential_map() {
    let credential = Credential::ScriptCredential { hash: ByteString::from(vec![0xab]) };
    assert_encoding(
        GovernanceAction::TreasuryWithdrawals {
            withdrawals: single(credential, BigInt::from(5)),
            constitution_script: POption::None,
        },
        Data::Constr {
            tag: 2,
            args: vec![
                Data::Map {
                    values: vec![(Data::Constr { tag: 1, args: vec![bytes(&[0xab])] }, int(5))],
                },
                nothing(),
            ],
        },
    );
}

/// `added_members` is a `Map ColdCommitteeCredential Integer`; `new_quorum` stays raw Data.
#[test]
fn update_committee_adds_members_from_a_credential_map() {
    let member = Credential::PubKeyCredential {
        hash: PubKeyHash { hash: ByteString::from(vec![0xcd]) },
    };
    let quorum = Data::Constr { tag: 0, args: vec![int(2), int(3)] };
    assert_encoding(
        GovernanceAction::UpdateCommittee {
            id: POption::None,
            removed_members: List::Nil,
            added_members: single(member, BigInt::from(7)),
            new_quorum: quorum.clone(),
        },
        Data::Constr {
            tag: 4,
            args: vec![
                nothing(),
                Data::List { values: vec![] },
                Data::Map {
                    values: vec![(Data::Constr { tag: 0, args: vec![bytes(&[0xcd])] }, int(7))],
                },
                quorum,
            ],
        },
    );
}
