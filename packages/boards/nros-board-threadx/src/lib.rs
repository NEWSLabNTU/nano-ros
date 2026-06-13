//! # nros-board-threadx
//!
//! **Generic ThreadX + NetX-Duo scaffolding crate for nano-ros.**
//!
//! Layer-2 entry-point in the board / BSP abstraction described in
//! `docs/design/0012-board-bsp-integration-architecture.md`. Overlay
//! crates (`nros-board-<vendor>-<chip>-threadx`, e.g. Renesas
//! Synergy / STM32 X-CUBE-AZRTOS / NXP MCUXpresso ThreadX) depend
//! on this crate + patch vendor HAL deltas via `#[no_mangle]`
//! hooks. See `book/src/porting/vendor-overlay.md` for the cookbook.
//!
//! ## 152.2.A scaffolding
//!
//! The crate exists today as a façade. Two opt-in features
//! re-export `Config` + `run` from the existing per-board ThreadX
//! crates so future overlays have a stable name to depend on while
//! 152.2.B carves the kernel + NetX-Duo build glue out of the
//! per-board `build.rs` files into this crate's own `build.rs`.
//!
//! | Feature | Re-exports from | Use case |
//! |---|---|---|
//! | `reference-linux` | `nros-board-threadx-linux` | Linux ThreadX-sim port + NSOS (host-kernel BSD sockets shim). Useful for CI + cross-test runners. |
//! | `reference-qemu-riscv64` | `nros-board-threadx-qemu-riscv64` | Bare-metal RISC-V64 QEMU port; real ThreadX kernel + NetX-Duo TCP/IP over virtio-net. |
//!
//! Pick one (not both — the two reference boards have different
//! `Config` shapes; the scaffolding feature-gate keeps them
//! mutually exclusive at link time).
//!
//! ## Public contract (post-152.2.B)
//!
//! Once the build-glue carve-out lands:
//!
//! - `Config` — TOML-loaded network + zenoh config; overlays extend.
//! - [`run`] — legacy
//!   `(Config, FnOnce(&Config) -> Result<(), E>) -> !` entry point
//!   over the `nros-board-common` traits. Calls `tx_kernel_enter()`
//!   after stashing the user closure; the ThreadX app thread invokes
//!   the closure once the kernel + network are up.
//! - [`run_entry`] — Phase 212.N.2 additive entry point over the new
//!   [`nros_platform::board`] trait set
//!   (`BoardInit` parameterless + `BoardPrint` + `BoardExit`
//!   + `RuntimeCtx`). Shape:
//!   `(Config, FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>) ->
//!   Result<(), E>`. Per-board `impl BoardEntry` impls (landed in
//!   212.N.3) delegate here.
//! - `init_hardware()` — board-specific init (default no-op;
//!   overlay re-exports a vendor version).
//! - `#[no_mangle]` hooks the overlay implements:
//!   - `nros_board_init_clocks()` — clock tree + pin mux.
//!   - `nros_board_init_eth()` — NetX-Duo `NX_IP_DRIVER` binding.
//!   - `nros_board_init_extra_drivers()` — sensors / displays.
//!
//! ## Phase 212.N status
//!
//! The legacy [`run`] (taking the `nros-board-common` traits + a
//! `&Config` closure) and the new [`run_entry`] (taking the
//! `nros_platform::board` traits + a `&mut RuntimeCtx` closure)
//! coexist during the 212.N migration; per-board crates pick whichever
//! entry point their `impl BoardEntry` / legacy `run` wrapper needs.
//! Phase 212.N.7 retires the legacy shape and collapses to
//! [`run_entry`] alone.
//!
//! ## SDK env-var contract
//!
//! The generic `build.rs` will read (after 152.2.B):
//!
//! | Var | Default | Purpose |
//! |---|---|---|
//! | `THREADX_DIR` | none (required) | ThreadX kernel source root. |
//! | `THREADX_CONFIG_DIR` | overlay's `config/` | `tx_user.h` directory. |
//! | `NETX_DIR` | none (required) | NetX-Duo source root. |
//! | `NETX_CONFIG_DIR` | overlay's `config/` | `nx_user.h` directory. |
//! | `THREADX_CFLAGS` | none | Extra compiler flags (overlay sets per-arch). |
//! | `BOARD_LINKER_SCRIPT_DIR` | none | Overlay's linker-script dir, added to link search path. |

// Phase 152.2.B.4 — both ThreadX overlays now `no_std`
// (152.4.B-prep refactor on `-linux`), so the crate flips
// unconditionally. The generic `run<B>` lives here; per-board
// overlays implement `BoardInit + BoardPrint + BoardExit +
// ThreadxConfig` and provide a thin non-generic `run` wrapper.

#![no_std]

// Phase 214.H.1 — single source of truth for the storage sizes both
// `entry.rs` and `node.rs` (the two ThreadX dispatch surfaces) share.
// Previously each file carried its own
//   const CTX_STORAGE_SIZE: usize = 8192;
//   const IFACE_BUF_SIZE: usize = 64;
// pair, with `assert!`-style call sites referring to the literal
// names. Bump them here once; both consumers pick up the change.
mod sizes {
    /// Storage for the per-board `AppContext` blob (board init writes
    /// transport state into this; the run-loop reads it). 8 KB covers
    /// the canonical zenoh-pico session + lwIP iface footprint with
    /// headroom; bump if `AppContext` outgrows it (the run loops
    /// `assert!(size <= CTX_STORAGE_SIZE)` will trip first).
    pub(crate) const CTX_STORAGE_SIZE: usize = 8192;

    /// Storage for the canonical network-interface name buffer the
    /// ThreadX glue passes through to the bsd / nsos shim. 64 bytes
    /// matches `IFNAMSIZ`-style limits on every supported host
    /// (Linux `IFNAMSIZ = 16`, BSD = 16; we round up for slack).
    pub(crate) const IFACE_BUF_SIZE: usize = 64;
}

mod entry;
mod node;

pub use entry::{run_app_thread, run_entry};
pub use node::run;
pub use nros_board_common::{BoardExit, BoardInit, BoardPrint, ThreadxConfig};

// Legacy 152.2.A façade — keep the per-board `Config` +
// `init_hardware` + `run` re-export accessible behind the
// reference-* features so existing downstream that picked the
// generic crate name during the .A → .B transition keeps
// working. New consumers should depend on the per-board crate
// directly (or import `run` from here + pick the marker type
// via turbofish).
#[cfg(feature = "reference-linux")]
pub use nros_board_threadx_linux::{Config as ConfigLinux, init_hardware as init_hardware_linux};

#[cfg(feature = "reference-qemu-riscv64")]
pub use nros_board_threadx_qemu_riscv64::{
    Config as ConfigQemuRiscv64, init_hardware as init_hardware_qemu_riscv64,
};
