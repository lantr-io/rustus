/// On-chain `require`: if condition is false, fail with error message.
///
/// In Rust: panics if condition is false.
/// In SIR: compiles to `if cond then () else Error(msg)`.
///
/// Note: Use as expression (last line without semicolon) in #[compile] functions.
#[macro_export]
macro_rules! require {
    ($cond:expr, $msg:literal) => {
        if $cond { () } else { panic!($msg) }
    };
}
