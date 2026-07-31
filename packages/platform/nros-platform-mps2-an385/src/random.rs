//! Xorshift32 PRNG — re-exports `nros_baremetal_common::random`.
//!
//! The full implementation lives in `nros-baremetal-common::random`
//! (shared with the other bare-metal platform crates). This module
//! is a thin re-export so callers can write
//! `nros_platform_mps2_an385::random::random_u32()` if they prefer
//! the per-platform path.

pub use nros_baremetal_common::random::{
    random_fill, random_u8, random_u16, random_u32, random_u64, seed,
};
