//! On-chain typeclass traits and registry.
//!
//! A "functional typeclass" is a single-method typeclass that maps to an extra
//! function argument in SIR. The `#[compile]` macro desugars trait bounds into
//! these extra arguments.
//!
//! Standard Rust trait mapping:
//!   PartialEq → OnchainPartialEq (method: (A, A) -> Boolean)
//!   PartialOrd → OnchainPartialOrd (method: (A, A) -> Order)
//!
//! Users can define custom typeclasses with `#[rustus::functional_typeclass]`.

use crate::default_fun::DefaultFun;
use crate::module::AnnotationsDecl;
use crate::sir::SIR;
use crate::sir_type::SIRType;

// ---------------------------------------------------------------------------
// Typeclass metadata — describes how a trait bound maps to SIR
// ---------------------------------------------------------------------------

/// Registration entry for a functional typeclass.
#[derive(Debug, Clone)]
pub struct FunctionalTypeclassInfo {
    /// The Rust trait name (e.g., "PartialEq", "OnChainEq")
    pub rust_trait_name: &'static str,
    /// The scalus typeclass name (e.g., "scalus.cardano.onchain.plutus.prelude.Eq")
    pub scalus_name: &'static str,
    /// The trait method name to call for the SIR expression (e.g., "sir_eq")
    pub method_name: &'static str,
}

// ---------------------------------------------------------------------------
// Typeclass registry — inventory-based for extensibility
// ---------------------------------------------------------------------------

/// An inventory-collected typeclass registration entry.
pub struct TypeclassEntry {
    pub info: FunctionalTypeclassInfo,
}

inventory::collect!(TypeclassEntry);

/// Returns all registered typeclass mappings (built-in + user-defined).
pub fn typeclass_registry() -> Vec<FunctionalTypeclassInfo> {
    let mut result = vec![
        // Built-in: standard Rust traits mapped to on-chain typeclasses
        FunctionalTypeclassInfo {
            rust_trait_name: "PartialEq",
            scalus_name: "scalus.cardano.onchain.plutus.prelude.Eq",
            method_name: "sir_eq",
        },
        FunctionalTypeclassInfo {
            rust_trait_name: "PartialOrd",
            scalus_name: "scalus.cardano.onchain.plutus.prelude.Ord",
            method_name: "sir_ord",
        },
    ];
    // Collect user-defined typeclasses via inventory
    for entry in inventory::iter::<TypeclassEntry> {
        result.push(entry.info.clone());
    }
    result
}

// ---------------------------------------------------------------------------
// On-chain typeclass traits — provide SIR for concrete type implementations
// ---------------------------------------------------------------------------

/// On-chain equality typeclass.
/// Maps to scalus `Eq[A]`: `(A, A) → Boolean`.
///
/// Types that derive `ToData` automatically get this.
/// The `#[compile]` macro maps `PartialEq` bounds to this trait.
pub trait OnchainPartialEq {
    /// Returns the SIR expression for the equality function.
    /// The expression should have type `Fun(T, Fun(T, Boolean))`.
    fn sir_eq() -> SIR;
}

/// On-chain ordering typeclass.
/// Maps to scalus `Ord[A]`: `(A, A) → Order`.
///
/// The `#[compile]` macro maps `PartialOrd` bounds to this trait.
pub trait OnchainPartialOrd {
    /// Returns the SIR expression for the comparison function.
    /// The expression should have type `Fun(T, Fun(T, Order))`.
    fn sir_ord() -> SIR;
}

// ---------------------------------------------------------------------------
// Built-in OnchainPartialEq implementations
// ---------------------------------------------------------------------------

impl OnchainPartialEq for num_bigint::BigInt {
    fn sir_eq() -> SIR {
        SIR::Builtin {
            builtin_fun: DefaultFun::EqualsInteger,
            tp: SIRType::Fun {
                from: Box::new(SIRType::Integer),
                to: Box::new(SIRType::Fun {
                    from: Box::new(SIRType::Integer),
                    to: Box::new(SIRType::Boolean),
                }),
            },
            anns: AnnotationsDecl::empty(),
        }
    }
}

impl OnchainPartialEq for Vec<u8> {
    fn sir_eq() -> SIR {
        SIR::Builtin {
            builtin_fun: DefaultFun::EqualsByteString,
            tp: SIRType::Fun {
                from: Box::new(SIRType::ByteString),
                to: Box::new(SIRType::Fun {
                    from: Box::new(SIRType::ByteString),
                    to: Box::new(SIRType::Boolean),
                }),
            },
            anns: AnnotationsDecl::empty(),
        }
    }
}

impl OnchainPartialEq for crate::data::Data {
    fn sir_eq() -> SIR {
        SIR::Builtin {
            builtin_fun: DefaultFun::EqualsData,
            tp: SIRType::Fun {
                from: Box::new(SIRType::Data),
                to: Box::new(SIRType::Fun {
                    from: Box::new(SIRType::Data),
                    to: Box::new(SIRType::Boolean),
                }),
            },
            anns: AnnotationsDecl::empty(),
        }
    }
}

impl OnchainPartialEq for bool {
    fn sir_eq() -> SIR {
        // Booleans are Constr(0/1, []) — compare with equalsData
        SIR::Builtin {
            builtin_fun: DefaultFun::EqualsData,
            tp: SIRType::Fun {
                from: Box::new(SIRType::Boolean),
                to: Box::new(SIRType::Fun {
                    from: Box::new(SIRType::Boolean),
                    to: Box::new(SIRType::Boolean),
                }),
            },
            anns: AnnotationsDecl::empty(),
        }
    }
}

impl OnchainPartialEq for String {
    fn sir_eq() -> SIR {
        SIR::Builtin {
            builtin_fun: DefaultFun::EqualsString,
            tp: SIRType::Fun {
                from: Box::new(SIRType::String),
                to: Box::new(SIRType::Fun {
                    from: Box::new(SIRType::String),
                    to: Box::new(SIRType::Boolean),
                }),
            },
            anns: AnnotationsDecl::empty(),
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in OnchainPartialOrd implementations
// ---------------------------------------------------------------------------

impl OnchainPartialOrd for num_bigint::BigInt {
    fn sir_ord() -> SIR {
        make_binary_builtin(DefaultFun::LessThanInteger, SIRType::Integer)
    }
}

impl OnchainPartialOrd for Vec<u8> {
    fn sir_ord() -> SIR {
        make_binary_builtin(DefaultFun::LessThanByteString, SIRType::ByteString)
    }
}

impl OnchainPartialOrd for crate::data::Data {
    fn sir_ord() -> SIR {
        // Data ordering: placeholder using equalsData.
        // Proper implementation requires serialiseData + byte comparison.
        make_binary_builtin(DefaultFun::EqualsData, SIRType::Data)
    }
}

/// Build a SIR Builtin with type `(T, T) -> Boolean` for a given builtin function.
/// Used by both OnchainPartialEq and OnchainPartialOrd implementations.
fn make_binary_builtin(fun: DefaultFun, operand_tp: SIRType) -> SIR {
    SIR::Builtin {
        builtin_fun: fun,
        tp: SIRType::Fun {
            from: Box::new(operand_tp.clone()),
            to: Box::new(SIRType::Fun {
                from: Box::new(operand_tp),
                to: Box::new(SIRType::Boolean),
            }),
        },
        anns: AnnotationsDecl::empty(),
    }
}
