//! Minimal libc stubs — pulled in via `nros_baremetal_common`'s
//! `libc-stubs` feature.
//!
//! The actual `#[unsafe(no_mangle)]` symbols (`strlen`, `memcpy`, etc.)
//! are emitted from `nros-baremetal-common::libc_stubs` when that
//! crate is built with the `libc-stubs` feature enabled. This
//! platform crate enables that feature in its `Cargo.toml`, so the
//! symbols are available at link time without any code in this file.
