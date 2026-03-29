pub mod constant;
pub mod data;
pub mod default_fun;
pub mod lower;
pub mod module;
pub mod pre_sir;
pub mod registry;
pub mod sir;
pub mod sir_type;
pub mod typeclasses;
pub mod typing;

// Re-export inventory so generated code from rustus-macros can use it
// without users adding inventory as a direct dependency.
pub use inventory;
pub use num_bigint;

pub mod prelude {
    pub use crate::data::{Data, FromData, ToData};
    pub use crate::module::{AnnotationsDecl, Binding, Module, SourcePos};
    pub use crate::registry::{build_module, EntryKind, PreSirEntry, ResolutionContext};
    pub use crate::sir::SIR;
    pub use crate::sir_type::{DataDecl, HasSIRType, SIRType};
}
