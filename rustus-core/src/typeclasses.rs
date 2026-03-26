//! On-chain typeclass traits.
//!
//! These provide SIR expressions for typeclass operations.
//! The `#[compile]` macro maps Rust standard traits to these:
//!   PartialEq → OnchainPartialEq
//!   PartialOrd → OnchainPartialOrd (future)

use crate::constant::UplcConstant;
use crate::default_fun::DefaultFun;
use crate::module::AnnotationsDecl;
use crate::sir::SIR;
use crate::sir_type::SIRType;

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

// --- Built-in implementations ---

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
