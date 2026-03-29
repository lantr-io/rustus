/// Scalus-compatible generic List type.
///
/// Matches scalus names:
/// - DataDecl: "scalus.cardano.onchain.plutus.prelude.List"
/// - Nil:  "scalus.cardano.onchain.plutus.prelude.List$.Nil"
/// - Cons: "scalus.cardano.onchain.plutus.prelude.List$.Cons"  (fields: head, tail)
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.prelude.List", repr = "list")]
pub enum List<T> {
    Nil,
    Cons { head: T, tail: Box<List<T>> },
}

impl<T: Clone> List<T> {
    pub fn from_vec(items: Vec<T>) -> Self {
        items
            .into_iter()
            .rev()
            .fold(List::Nil, |acc, item| List::Cons {
                head: item,
                tail: Box::new(acc),
            })
    }

    pub fn to_vec(&self) -> Vec<T> {
        let mut result = vec![];
        let mut current = self;
        loop {
            match current {
                List::Nil => break,
                List::Cons { head, tail } => {
                    result.push(head.clone());
                    current = tail;
                }
            }
        }
        result
    }
}

impl<T: rustus_core::data::ToData + Clone> List<T> {
    /// Convert all elements to Data.
    pub fn map_to_data(&self) -> List<rustus_core::data::Data> {
        match self {
            List::Nil => List::Nil,
            List::Cons { head, tail } => List::Cons {
                head: head.to_data(),
                tail: Box::new(tail.map_to_data()),
            },
        }
    }
}

impl<T: PartialEq + Clone> List<T> {
    /// Rust-side contains (works with any PartialEq type for testing).
    pub fn contains_elem(&self, elem: &T) -> bool {
        match self {
            List::Nil => false,
            List::Cons { head, tail } => {
                if head == elem {
                    true
                } else {
                    tail.contains_elem(elem)
                }
            }
        }
    }
}

#[rustus_macros::rustus_module("scalus.cardano.onchain.plutus.prelude.List$")]
mod sir {
    use super::List;
    use rustus_core::data::Data;

    #[rustus_macros::compile]
    pub fn is_empty(list: List<Data>) -> bool {
        match list {
            List::Nil => true,
            _ => false,
        }
    }

    #[rustus_macros::compile]
    pub fn head(list: List<Data>) -> Data {
        match list {
            List::Cons { head, .. } => head,
            List::Nil => panic!("head: empty list"),
        }
    }

    #[rustus_macros::compile]
    pub fn tail(list: List<Data>) -> List<Data> {
        match list {
            List::Cons { tail, .. } => *tail,
            List::Nil => panic!("tail: empty list"),
        }
    }

    /// On-chain contains: uses typeclass equality.
    /// SIR signature: (List<T>, T, (T, T) → Boolean) → Boolean
    #[rustus_macros::compile]
    pub fn contains<T: PartialEq>(list: List<T>, elem: T) -> bool {
        match list {
            List::Nil => false,
            List::Cons { head, tail } => {
                if head == elem {
                    true
                } else {
                    contains(*tail, elem)
                }
            }
        }
    }
}

pub use sir::{contains, head, is_empty, tail};
