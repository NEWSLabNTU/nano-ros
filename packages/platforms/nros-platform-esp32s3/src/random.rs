//! Xorshift32 PRNG — re-exports `nros_baremetal_common::random`.

pub use nros_baremetal_common::random::{
    random_fill, random_u8, random_u16, random_u32, random_u64, seed,
};
