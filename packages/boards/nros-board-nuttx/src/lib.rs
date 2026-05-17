//! # nros-board-nuttx
//!
//! **Generic NuttX board scaffolding for nano-ros.**
//!
//! Layer-2 entry-point in the board / BSP abstraction described in
//! `docs/design/board-bsp-integration-architecture.md`. Unlike the
//! `nros-board-{freertos, threadx}` siblings, this crate is THIN
//! by design — NuttX owns the kernel build through its own
//! `apps/external/nano-ros/` + `Make.defs` + `Kconfig` integration
//! (see `integrations/nuttx/` and the Phase 149.7 polish). The
//! Cargo side only needs to ship `Config` + `run` + board-init
//! hooks; there is no `build.rs` bundling the NuttX kernel
//! sources here.
//!
//! ## 149.4.A scaffolding
//!
//! Opt-in `reference-qemu-arm` feature re-exports `Config` + `run`
//! from `nros-board-nuttx-qemu-arm` so future overlays
//! (`nros-board-px4-fmu-v5-nuttx`, `nros-board-<vendor>-<board>-nuttx`)
//! depend on this crate name + can extend the `Config` shape +
//! patch board-specific init via `#[no_mangle]` hooks.
//!
//! 149.4.B (deferred) carves the per-board `Config` / `init_hardware`
//! variation into a `BoardInit` trait so the per-board crate
//! shrinks to a `pub struct MyBoard; impl BoardInit for MyBoard
//! { ... }`. Today the per-board crate hand-rolls `Config`.
//!
//! ## Public contract (post-149.4.B)
//!
//! - `Config` — TOML-loaded network + zenoh config.
//! - `run(Config, FnOnce(&Config) -> Result<(), E>)` — entry point.
//!   For NuttX this is a regular Rust `main` that initialises
//!   nros + drops into the user closure; the NuttX kernel is
//!   already up by the time `main` runs (NuttX init is the OS,
//!   not something this crate boots).
//! - `init_hardware()` — board-specific peripheral wakes
//!   (sensors, displays, vendor-specific GPIO that NuttX's `apps/`
//!   discovery doesn't auto-configure).
//!
//! ## SDK env-var contract
//!
//! NuttX owns the kernel build; the Cargo side reads:
//!
//! | Var | Purpose |
//! |---|---|
//! | `NUTTX_DIR` | Source root for header discovery (used by `nros-platform-cffi`'s NuttX C port). |
//!
//! Compared to FreeRTOS / ThreadX scaffolds, no kernel-source /
//! port-dir / config-dir env vars are read here. NuttX's own
//! `make menuconfig` + `defconfig` flow drives all of that.

#![cfg_attr(not(feature = "reference-qemu-arm"), no_std)]

#[cfg(feature = "reference-qemu-arm")]
pub use nros_board_nuttx_qemu_arm::{Config, init_hardware, run};
