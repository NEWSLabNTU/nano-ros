//! Phase 123.A.1.x.4.a — thin staticlib wrapper around
//! `nros-rmw-dds`.
//!
//! Same shape as `nros-rmw-zenoh-staticlib`: flips the crate-type to
//! `staticlib` so Cargo emits `libnros_rmw_dds_staticlib.a` containing
//! the full nros-rmw-dds dependency closure (dust-dds, compiler_builtins,
//! the cffi vtable trait impl, `nros_rmw_dds_register()` C entry).
//!
//! Downstream link uses `--allow-multiple-definition` to reconcile
//! the compiler_builtins copies shared with `libnros_c.a`.

#![no_std]

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx",
    feature = "platform-bare-metal",
))]
pub use nros_rmw_dds::register;
