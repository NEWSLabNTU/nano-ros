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
    feature = "platform-threadx",
    feature = "platform-threadx-std",
))]
extern crate nros_platform as _;

#[cfg(all(
    not(feature = "std"),
    any(
        feature = "platform-freertos",
        feature = "platform-threadx",
        feature = "platform-esp-idf",
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

// Phase 134.fix — `.init_array` auto-register ctor.
//
// The staticlib pulls `nros-rmw-zenoh` with `default-features =
// false` (see Cargo.toml), which disables `nros-rmw-zenoh`'s new
// `linkme-register` default feature. The macro call inside
// `nros-rmw-zenoh::cffi_register::nros_rmw_register_backend!`
// expands to nothing, so the linkme entry that would normally fire
// at runtime auto-register doesn't get emitted. That avoids the
// duplicate `RMW_INIT_ENTRIES` distributed_slice collision when
// `libnros_rmw_zenoh.a` is linked next to `libnros_c.a` (both
// archives would otherwise bundle a crate-hash-distinct
// nros-rmw-cffi rlib instance with its own DUPCHECK static).
//
// To preserve auto-register without linkme, ship a tiny ELF
// `.init_array` slot holding a function pointer that calls
// `nros_rmw_zenoh_register()` before `main()` runs. The platform
// loader walks `.init_array` for every static (in load order)
// before transferring control to `main`. Each loader-supported
// target gets one row in the cfg block below; others fall back to
// the explicit `nros_rmw_zenoh_register()` call pattern.
#[cfg(all(
    any(
        feature = "platform-posix",
        feature = "platform-zephyr",
        feature = "platform-freertos",
        feature = "platform-nuttx",
        feature = "platform-threadx",
        feature = "platform-threadx-std",
        feature = "platform-bare-metal",
    ),
    any(
        target_os = "linux",
        target_os = "android",
        target_os = "fuchsia",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "macos",
    )
))]
mod auto_register_ctor {
    use core::ffi::c_void;

    // SAFETY: `nros_rmw_zenoh_register` is `#[unsafe(no_mangle)] extern "C"`,
    // returns an i32 status code, takes no arguments. Calling it before
    // `main` is safe — the function is documented as idempotent and
    // re-entrant against `Executor::open`.
    unsafe extern "C" fn nros_rmw_zenoh_auto_register() {
        unsafe extern "C" {
            fn nros_rmw_zenoh_register() -> i32;
        }
        unsafe {
            let _ = nros_rmw_zenoh_register();
        }
    }

    // ELF `.init_array` slot. The dynamic loader walks this section
    // before transferring control to `main`, invoking every fn
    // pointer found there. `#[used]` keeps the symbol against gc.
    // `link_section` lands it in the canonical loader section.
    #[used]
    #[cfg_attr(
        any(
            target_os = "linux",
            target_os = "android",
            target_os = "fuchsia",
            target_os = "freebsd",
            target_os = "openbsd",
        ),
        unsafe(link_section = ".init_array")
    )]
    #[cfg_attr(
        any(target_os = "macos", target_os = "ios"),
        unsafe(link_section = "__DATA,__mod_init_func")
    )]
    static AUTO_REGISTER_CTOR: unsafe extern "C" fn() = nros_rmw_zenoh_auto_register;

    // Touch `c_void` to silence the unused-import warning when the
    // module is compiled for a target where neither `link_section`
    // attribute fires (the all() outer cfg already excludes those,
    // but rust-analyzer evaluates the import eagerly).
    const _: () = {
        let _: *const c_void = core::ptr::null();
    };
}
