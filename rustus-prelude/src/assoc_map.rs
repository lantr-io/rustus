use crate::list::List;
use crate::pair::Pair;

/// Scalus-compatible AssocMap type.
///
/// On-chain representation: Plutus Map (Data::Map).
/// Matches scalus name: `scalus.cardano.onchain.plutus.prelude.AssocMap`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.prelude.AssocMap", repr = "map")]
pub struct AssocMap<K, V> {
    pub inner: List<Pair<K, V>>,
}

impl<K, V> AssocMap<K, V> {
    pub fn empty() -> Self {
        AssocMap { inner: List::Nil }
    }

    pub fn singleton(key: K, value: V) -> Self {
        AssocMap {
            inner: List::Cons {
                head: Pair::new(key, value),
                tail: Box::new(List::Nil),
            },
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self.inner, List::Nil)
    }
}

impl<K: Clone, V: Clone> AssocMap<K, V> {
    pub fn from_vec(pairs: Vec<(K, V)>) -> Self {
        AssocMap {
            inner: List::from_vec(
                pairs.into_iter().map(|(k, v)| Pair::new(k, v)).collect(),
            ),
        }
    }

    pub fn to_vec(&self) -> Vec<(K, V)> {
        self.inner
            .to_vec()
            .into_iter()
            .map(|p| (p.fst, p.snd))
            .collect()
    }
}

impl<K: PartialEq + Clone, V: Clone> AssocMap<K, V> {
    pub fn get(&self, key: &K) -> Option<V> {
        self.to_vec()
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.to_vec().iter().any(|(k, _)| k == key)
    }
}
