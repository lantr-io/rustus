/// Scalus-compatible Order type for comparison results.
///
/// Matches scalus name: `scalus.cardano.onchain.plutus.prelude.Order`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.prelude.Order")]
pub enum Order {
    Less,
    Equal,
    Greater,
}
