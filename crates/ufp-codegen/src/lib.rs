//! C + ggml code generation for `.ufp`.
//!
//! Placeholder in M0: the partitioner and emitter land in E6.

/// The compiler version stamped into artifacts and portal cache keys.
pub const UFP_VERSION: &str = env!("CARGO_PKG_VERSION");
