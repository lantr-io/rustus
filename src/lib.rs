pub use rustus_core::*;

// Re-export proc macros so users can write:
//   #[rustus::compile]        instead of #[rustus_macros::compile]
//   #[rustus::module("...")]  instead of #[rustus_macros::rustus_module("...")]
//   #[derive(rustus::ToData)] instead of #[derive(rustus_macros::ToData)]
pub use rustus_macros::compile;
pub use rustus_macros::rustus_module as module;
pub use rustus_macros::FromData;
pub use rustus_macros::ToData;
