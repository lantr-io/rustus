/// Scalus-compatible BuiltinPair type.
///
/// Matches scalus name: `scalus.uplc.builtin.BuiltinPair`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.uplc.builtin.BuiltinPair")]
pub struct Pair<A, B> {
    pub fst: A,
    pub snd: B,
}

impl<A, B> Pair<A, B> {
    pub fn new(fst: A, snd: B) -> Self {
        Pair { fst, snd }
    }
}

impl<A, B> From<(A, B)> for Pair<A, B> {
    fn from((fst, snd): (A, B)) -> Self {
        Pair { fst, snd }
    }
}
