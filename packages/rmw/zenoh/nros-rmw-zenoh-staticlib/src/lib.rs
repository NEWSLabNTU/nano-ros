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

#[cfg(any(
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx",
    feature = "platform-threadx-std",
))]
extern crate nros_platform as _;

#[cfg(all(
    not(feature = "std"),
    any(
        feature = "platform-freertos",
        feature = "platform-nuttx",
        feature = "platform-threadx",
    )
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
    feature = "platform-threadx-std",
    feature = "platform-bare-metal",
))]
pub use nros_rmw_zenoh::register;

// Phase 249 P4b.3 — the hosted `.init_array` auto-register ctor now
// lives in the `nros-rmw-zenoh` backend crate itself, emitted by the
// `nros_rmw_register_backend!` macro on every hosted target
// (`not(target_os = "none")`) regardless of feature selection. The
// wrapper's own redundant ctor was removed: it existed only as the
// `linkme`-off fallback, and `linkme` is gone (RFC-0042 §D3.3). The
// `pub use register` above keeps the C entry alive for the embedded
// explicit-register path.
