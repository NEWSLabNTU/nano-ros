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

// Phase 149.4.B — re-export the kernel-agnostic BoardInit trait so
// overlays can `use nros_board_nuttx::BoardInit` without naming
// nros-board-common directly. Once 149.4.B.2's overlay refactor
// lands, the per-board crate impls this trait and the generic
// `run::<B>` shim below consumes it.
pub use nros_board_common::BoardInit;

#[cfg(feature = "reference-qemu-arm")]
pub use nros_board_nuttx_qemu_arm::{Config, init_hardware, run};

/// Phase 149.4.B — generic NuttX entry point.
///
/// Drives every NuttX overlay's boot: invokes the board's
/// `BoardInit::init_hardware`, sleeps briefly for NuttX
/// networking to settle (the kernel runs `NETINIT_*` synchronously
/// before `main`, but virtio-net link-up isn't atomic), then
/// hands control to the user closure. Closure return code maps to
/// `std::process::exit(0)` / `(1)`.
///
/// Per-board overlay's `run` calls into this with the matching
/// `BoardInit` impl:
/// ```ignore
/// pub fn run<F, E>(cfg: Config, f: F) -> !
/// where
///     F: FnOnce(&Config) -> Result<(), E>,
///     E: std::fmt::Debug,
/// {
///     nros_board_nuttx::run_generic::<QemuArmVirt, _, _>(cfg, f)
/// }
/// ```
///
/// Available only when `std` is reachable (NuttX targets bring
/// their own `std`). Bare `cargo check` without a NuttX target +
/// without `reference-qemu-arm` skips the impl.
#[cfg(any(feature = "reference-qemu-arm", target_os = "nuttx"))]
pub fn run_generic<B, F, E>(cfg: B::Config, f: F) -> !
where
    B: BoardInit,
    F: FnOnce(&B::Config) -> std::result::Result<(), E>,
    E: std::fmt::Debug,
{
    B::init_hardware(&cfg);

    // NuttX virtio-net needs a brief warm-up after kernel
    // `NETINIT_*` before `connect()` succeeds.
    std::thread::sleep(std::time::Duration::from_secs(5));

    use std::io::Write as _;
    let _ = std::io::stdout().flush();

    match f(&cfg) {
        Ok(()) => {
            let _ = std::io::stdout().flush();
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Application error: {:?}", e);
            let _ = std::io::stdout().flush();
            std::process::exit(1);
        }
    }
}
