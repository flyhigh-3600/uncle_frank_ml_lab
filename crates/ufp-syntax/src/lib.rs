//! Lexer, AST and parser for Uncle Frank's Plan (`.ufp`).
//!
//! M0 scaffold: the lexer (UF-101) and plan parser (UF-102) land here. Keeping
//! the crate present now fixes the workspace shape and lets the CLI grow against
//! a stable name.

/// The compiler version stamped into artifacts and portal cache keys.
pub const UFP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The syntax layer is not implemented yet.
pub fn ready() -> bool {
    false
}
