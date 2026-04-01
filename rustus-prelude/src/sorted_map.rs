use crate::list::List;
use crate::pair::Pair;

/// Scalus-compatible SortedMap type.
///
/// On-chain representation: Plutus Map (Data::Map).
/// Keys are maintained in sorted order (via Ord/PartialOrd).
/// Matches scalus name: `scalus.cardano.onchain.plutus.prelude.SortedMap`
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.prelude.SortedMap", repr = "map")]
pub struct SortedMap<K, V> {
    pub inner: List<Pair<K, V>>,
}

impl<K, V> SortedMap<K, V> {
    pub fn empty() -> Self {
        SortedMap { inner: List::Nil }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self.inner, List::Nil)
    }
}

impl<K: Clone + Ord, V: Clone> SortedMap<K, V> {
    pub fn from_vec(mut pairs: Vec<(K, V)>) -> Self {
        pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
        SortedMap {
            inner: List::from_vec(
                pairs.into_iter().map(|(k, v)| Pair::new(k, v)).collect(),
            ),
        }
    }

    pub fn singleton(key: K, value: V) -> Self {
        SortedMap {
            inner: List::Cons {
                head: Pair::new(key, value),
                tail: Box::new(List::Nil),
            },
        }
    }

    pub fn to_vec(&self) -> Vec<(K, V)> {
        self.inner
            .to_vec()
            .into_iter()
            .map(|p| (p.fst, p.snd))
            .collect()
    }

    pub fn get(&self, key: &K) -> Option<V>
    where
        K: PartialEq,
    {
        let mut cur = &self.inner;
        loop {
            match cur {
                List::Nil => return None,
                List::Cons { head, tail } => {
                    if &head.fst == key {
                        return Some(head.snd.clone());
                    }
                    if head.fst > *key {
                        return None;
                    }
                    cur = tail;
                }
            }
        }
    }

    pub fn insert(self, key: K, value: V) -> Self
    where
        K: PartialEq,
    {
        let mut pairs = self.to_vec();
        if let Some(pos) = pairs.iter().position(|(k, _)| k == &key) {
            pairs[pos] = (key, value);
        } else {
            pairs.push((key, value));
            pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
        }
        SortedMap {
            inner: List::from_vec(
                pairs.into_iter().map(|(k, v)| Pair::new(k, v)).collect(),
            ),
        }
    }
}
