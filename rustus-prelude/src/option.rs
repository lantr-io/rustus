/// Scalus-compatible Option type.
///
/// Note: scalus order is Some=0, None=1 (opposite of Haskell/standard convention).
///
/// Matches scalus names:
/// - DataDecl: "scalus.cardano.onchain.plutus.prelude.Option"
/// - Some: "scalus.cardano.onchain.plutus.prelude.Option$.Some"
/// - None: "scalus.cardano.onchain.plutus.prelude.Option$.None"
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.prelude.Option")]
pub enum Option<T> {
    Some { value: T },
    None,
}

impl<T> Option<T> {
    pub fn is_some(&self) -> bool {
        matches!(self, Option::Some { .. })
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Option::None)
    }

    pub fn unwrap(self) -> T {
        match self {
            Option::Some { value } => value,
            Option::None => panic!("called unwrap on None"),
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Option<U> {
        match self {
            Option::Some { value } => Option::Some { value: f(value) },
            Option::None => Option::None,
        }
    }
}

impl<T> From<std::option::Option<T>> for Option<T> {
    fn from(opt: std::option::Option<T>) -> Self {
        match opt {
            std::option::Option::Some(v) => Option::Some { value: v },
            std::option::Option::None => Option::None,
        }
    }
}
