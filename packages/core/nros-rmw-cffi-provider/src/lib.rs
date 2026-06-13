//! RFC-0042 D3 / phase-241.D slice 4 — the single provider archive for the
//! `nros-rmw-cffi` C ABI.
//!
//! `nros-rmw-cffi` was made def-less: its registry static `REGISTRY` and the C
//! entry points (`nros_rmw_cffi_register{,_named}`, `_lookup`,
//! `_registered_names`, `_set_custom_transport`, `_walk_init_section`) are now
//! Rust-mangled (no `#[no_mangle]`), so every staticlib that bundles the cffi
//! rlib (`libnros_c.a`, `libnros_cpp.a`, each RMW staticlib) emits ZERO strong
//! duplicate symbols and references them undefined.
//!
//! This crate invokes `nros_rmw_cffi_export!{}` exactly once → emits the single
//! `#[no_mangle]` definition of each. Built as a standalone `staticlib`
//! (`libnros_rmw_cffi_provider.a`) linked once into the final image (mirrors
//! `nros-platform-posix`), it makes the cffi symbols single-definition by
//! construction — which is what lets the C/C++ link drop
//! `--allow-multiple-definition` (the blind ODR mask).
//!
//! It must NOT be hosted in a multi-archive crate (nros-c / nros-cpp are both
//! bundled into more than one staticlib, which would re-duplicate the defs).

#![cfg_attr(not(feature = "std"), no_std)]

// Keep `nros-platform` linked on the no_std RTOS targets — its
// `global-allocator` feature installs the heap adapter the cffi closure needs.
#[cfg(any(
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx",
))]
extern crate nros_platform as _;

#[cfg(all(
    not(feature = "std"),
    any(
        feature = "platform-freertos",
        feature = "platform-nuttx",
        feature = "platform-threadx",
        feature = "platform-bare-metal",
    )
))]
extern crate panic_halt as _;

// The one definition of REGISTRY + the cffi C ABI entry points.
nros_rmw_cffi::nros_rmw_cffi_export! {}
