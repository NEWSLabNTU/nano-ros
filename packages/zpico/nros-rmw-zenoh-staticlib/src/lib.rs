//! Phase 123.A.1.x.4.a — thin staticlib wrapper around
//! `nros-rmw-zenoh`.
//!
//! The wrapper exists solely to flip the crate-type from `rlib` to
//! `staticlib` so Cargo emits a standalone `libnros_rmw_zenoh_staticlib.a`
//! archive containing the full nros-rmw-zenoh dependency closure
//! (zenoh-pico C sources, zpico-sys bindings, compiler_builtins,
//! the cffi vtable trait impl, the `nros_rmw_zenoh_register()` C
//! entry).
//!
//! Linking: the archive is meant to sit next to `libnros_c.a` at
//! the CMake site (`NanoRos::NanoRos` pulls it in via
//! `find_dependency(NrosRmwZenohStaticlib)` once the install rules
//! land — A.1.x.4.b). Because both archives carry their own copy
//! of `compiler_builtins`, downstream links require
//! `--allow-multiple-definition` (GNU ld / lld).

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(any(feature = "platform-freertos", feature = "platform-threadx"))]
extern crate nros_platform as _;

#[cfg(all(
    not(feature = "std"),
    any(feature = "platform-freertos", feature = "platform-threadx")
))]
extern crate panic_halt as _;

// Force the linker to retain the cffi register entry. Without an
// explicit reference, `cargo:rustc-cdylib-link-arg=-Wl,-u` would be
// needed at the consumer's build; pulling the symbol into a `pub
// use` keeps `staticlib` extraction simple.
//
// The actual `nros_rmw_zenoh_register` symbol is `#[unsafe(no_mangle)]`
// and only emits under the same platform features as nros-rmw-zenoh
// itself; mirror the gate here so the wrapper compiles without a
// platform pinned.
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx",
    feature = "platform-bare-metal",
))]
pub use nros_rmw_zenoh::register;

// Phase 123.A.11.1 — the auto-register `.init_array` ctor lives
// in `nros-rmw-zenoh::cffi_register::AUTO_REGISTER_CTOR` (added
// by phase 104.A). The `pub use` above transitively keeps the
// register symbol live; the cdylib/staticlib link pulls in the
// `#[used]` static + the ctor body, so POSIX `.init_array`
// walking before `main()` registers the backend without
// `nros-c` ever calling `nros_rmw_zenoh_register` explicitly.
