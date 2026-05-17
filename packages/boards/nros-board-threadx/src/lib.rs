//! # nros-board-threadx
//!
//! **Generic ThreadX + NetX-Duo scaffolding crate for nano-ros.**
//!
//! Layer-2 entry-point in the board / BSP abstraction described in
//! `docs/design/board-bsp-integration-architecture.md`. Overlay
//! crates (`nros-board-<vendor>-<chip>-threadx`, e.g. Renesas
//! Synergy / STM32 X-CUBE-AZRTOS / NXP MCUXpresso ThreadX) depend
//! on this crate + patch vendor HAL deltas via `#[no_mangle]`
//! hooks. See `book/src/porting/vendor-overlay.md` for the cookbook.
//!
//! ## 149.2.A scaffolding
//!
//! The crate exists today as a façade. Two opt-in features
//! re-export `Config` + `run` from the existing per-board ThreadX
//! crates so future overlays have a stable name to depend on while
//! 149.2.B carves the kernel + NetX-Duo build glue out of the
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
//! ## Public contract (post-149.2.B)
//!
//! Once the build-glue carve-out lands:
//!
//! - `Config` — TOML-loaded network + zenoh config; overlays extend.
//! - `run(Config, FnOnce(&Config) -> Result<(), E>)` — entry point.
//!   Calls `tx_kernel_enter()` after stashing the user closure;
//!   the ThreadX app thread invokes the closure once the kernel +
//!   network are up.
//! - `init_hardware()` — board-specific init (default no-op;
//!   overlay re-exports a vendor version).
//! - `#[no_mangle]` hooks the overlay implements:
//!   - `nros_board_init_clocks()` — clock tree + pin mux.
//!   - `nros_board_init_eth()` — NetX-Duo `NX_IP_DRIVER` binding.
//!   - `nros_board_init_extra_drivers()` — sensors / displays.
//!
//! ## SDK env-var contract
//!
//! The generic `build.rs` will read (after 149.2.B):
//!
//! | Var | Default | Purpose |
//! |---|---|---|
//! | `THREADX_DIR` | none (required) | ThreadX kernel source root. |
//! | `THREADX_CONFIG_DIR` | overlay's `config/` | `tx_user.h` directory. |
//! | `NETX_DIR` | none (required) | NetX-Duo source root. |
//! | `NETX_CONFIG_DIR` | overlay's `config/` | `nx_user.h` directory. |
//! | `THREADX_CFLAGS` | none | Extra compiler flags (overlay sets per-arch). |
//! | `BOARD_LINKER_SCRIPT_DIR` | none | Overlay's linker-script dir, added to link search path. |

// The two reference boards have incompatible `std` requirements
// (`-linux` is `std`, `-qemu-riscv64` is `no_std`), so we cannot
// flip `no_std` unconditionally — the attribute lives behind the
// feature gates.

#![cfg_attr(
    not(any(feature = "reference-linux", feature = "reference-qemu-riscv64")),
    no_std
)]

#[cfg(feature = "reference-linux")]
pub use nros_board_threadx_linux::{Config, init_hardware, run};

#[cfg(all(
    feature = "reference-qemu-riscv64",
    not(feature = "reference-linux")
))]
pub use nros_board_threadx_qemu_riscv64::{Config, init_hardware, run};
