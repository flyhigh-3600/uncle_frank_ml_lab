//! Reference interpreter for Uncle Frank's Plan (`.ufp`).
//!
//! M0 scaffold. Execution over typed IR lands in UF-309; the text-terminal
//! trigger protocol lands in UF-310.

/// The compiler version stamped into artifacts and portal cache keys.
pub const UFP_VERSION: &str = env!("CARGO_PKG_VERSION");
