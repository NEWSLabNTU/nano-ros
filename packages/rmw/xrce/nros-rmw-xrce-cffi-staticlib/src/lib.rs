//! Phase 160.L — thin staticlib wrapper around `nros-rmw-xrce-cffi`.
//!
//! Same shape as `nros-rmw-zenoh-staticlib`:
//! flips the crate-type to `staticlib` so Cargo emits a standalone
//! `libnros_rmw_xrce_cffi_staticlib.a` archive containing the full
//! cffi dependency closure (Micro XRCE-DDS Client C sources, the
//! `.init_array` ctor entry, `nros_rmw_xrce_register()` C entry).
//!
//! Downstream link uses `--allow-multiple-definition` to reconcile
//! the compiler_builtins copies shared with `libnros_c.a`.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "panic-halt")]
use panic_halt as _;

// Force the linker to retain the cffi register entry. The actual
// symbol is `#[unsafe(no_mangle)] extern "C"` inside
// `nros-rmw-xrce-cffi`'s vtable.c; pulling its Rust-side `register()`
// wrapper into a `pub use` keeps the symbol alive against gc on
// hosted targets where the staticlib is consumed via
// `-Wl,--whole-archive`.
pub use nros_rmw_xrce_cffi::register;
