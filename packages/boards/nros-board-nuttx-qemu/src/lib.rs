//! # nros-board-nuttx-qemu
//!
//! Board crate for running nros on the QEMU NuttX boards — **arm virt**
//! (Cortex-A7 + virtio-net) and **rv-virt** (rv32imac + virtio-net).
//!
//! ## One crate, two witnesses (phase-337 W3 / RFC-0064)
//!
//! These were two crates until phase-337 W3, and phase-322 W1.a measured 1059
//! byte-identical lines between them: both upper layers — the kernel *and* the
//! arch port — come from upstream NuttX, and the link list is *discovered* by
//! scanning `$NUTTX_DIR/staging` for `lib*.a` rather than hardcoded, so an
//! in-tree NuttX board crate is pure overlay with nothing arch-specific in it.
//!
//! What genuinely differs between the two boards is **data**, and it stays
//! data: `nuttx-config/<arch>/defconfig`, `<arch>-nuttx-toolchain.cmake`, the
//! `NUTTX_*` env in each `[[board]]` entry of `nros-board.toml`, and the two
//! FFI subcrates (`nros-nuttx-ffi` / `nros-nuttx-riscv-ffi`) whose
//! `.cargo/config.toml` carries the target triple. The one *semantic* delta
//! phase-322 found is [`SLIRP_DEFAULT_IP`], which matches each board's
//! defconfig `NETINIT` address and is therefore also an arch fact, not a fork.
//!
//! ## Architecture
//!
//! Unlike bare-metal board crates (`nros-board-mps2-an385`), this crate has no
//! custom hardware drivers or networking stack:
//!
//! - **Networking**: NuttX kernel provides BSD sockets (no smoltcp/lwIP)
//! - **Ethernet**: NuttX virtio-net driver (no custom LAN9118 driver)
//! - **Platform**: zenoh-pico reuses `unix/` platform (no `zpico-platform-*` crate)
//! - **Rust std**: NuttX targets support `std` — `println!`, `std::time` work natively
//!
//! # Example
//!
//! ```ignore
//! use nros_board_nuttx_qemu::NuttxQemu;
//! use nros_platform::BoardEntry;
//!
//! fn main() -> Result<(), nros::Error> {
//!     <NuttxQemu as BoardEntry>::run(|runtime| {
//!         // codegen-emitted plan
//!         Ok(())
//!     })
//! }
//! ```

mod config;
mod entry;
// Phase 212.N.3 — new platform-level trait impls (`nros_platform::Board*`)
// live in a sibling module so the legacy `nros_board_common::Board*` impls
// above stay untouched. Both trait families coexist during the 212.N
// transition; codegen-emitted Entry pkgs (212.N.4) consume the platform-level
// path via `<NuttxQemu as nros_platform::BoardEntry>::run`.
mod entry_212n;
mod node;

pub use config::Config;

/// Per-board marker for trait dispatch.
///
/// Phase 313 W-nuttx (#0243) — the legacy `nros_board_common::board_init`
/// impls (`BoardInit`/`BoardPrint`/`BoardExit` + the free `node::run`) are
/// RETIRED for this board; the live path is the `nros_platform::board`
/// trait set in [`mod entry_212n`] (`BoardInit`, `BoardPrint`, `BoardExit`,
/// `BoardEntry`).
pub struct NuttxQemu;

/// The pre-merge name of the arm witness's ZST — phase-337 W3.c keeps both
/// spellings, so an out-of-tree entry pkg that names one keeps compiling.
pub type QemuArmVirt = NuttxQemu;

/// The pre-merge name of the riscv witness's ZST. Same type: the board ZST only
/// selects trait impls, and those never differed between the two boards.
pub type QemuRvVirt = NuttxQemu;

pub use node::init_hardware;

// Issue #130 — the shared public eth0-config entry point + slirp defaults, so
// the C `nros-nuttx-ffi` entry can push the guest IP into `eth0` before
// `app_main()` exactly as the Rust `BoardEntry` path does (no drift, one impl).
#[cfg(target_os = "nuttx")]
pub use entry_212n::configure_entry_eth0;
pub use entry_212n::{SLIRP_DEFAULT_GATEWAY, SLIRP_DEFAULT_IP, SLIRP_DEFAULT_PREFIX};
