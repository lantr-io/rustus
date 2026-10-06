/// Scalus-compatible Order type for comparison results.
///
/// Matches scalus name: `scalus.cardano.onchain.plutus.prelude.Order`, including its
/// `@UplcRepr(UplcConstr)`: on-chain an Order is a native `constr`, not Data.
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.prelude.Order", repr = "uplc_constr")]
pub enum Order {
    Less,
    Equal,
    Greater,
}
