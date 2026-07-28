//! # nros-board-nuttx-qemu-arm
//!
//! Board crate for running nros on NuttX QEMU ARM virt (Cortex-A7 + virtio-net).
//!
//! Handles platform configuration. Users call [`run()`] with a closure that
//! receives [`&Config`](Config) and creates an `Executor` for full API access
//! (publishers, subscriptions, services, actions, timers).
//!
//! # Architecture
//!
//! Unlike bare-metal board crates (`nros-board-mps2-an385`), this crate has no custom
//! hardware drivers or networking stack:
//!
//! - **Networking**: NuttX kernel provides BSD sockets (no smoltcp/lwIP)
//! - **Ethernet**: NuttX virtio-net driver (no custom LAN9118 driver)
//! - **Platform**: zenoh-pico reuses `unix/` platform (no `zpico-platform-*` crate)
//! - **Rust std**: NuttX targets support `std` — `println!`, `std::time` work natively
//!
//! # Example
//!
//! ```ignore
//! use nros::prelude::*;
//! use nros_board_nuttx_qemu_arm::{Config, run};
//!
//! fn main() {
//!     run(Config::default(), |config| {
//!         let exec_config = ExecutorConfig::new(config.zenoh_locator)
//!             .domain_id(config.domain_id)
//!             .node_name("talker");
//!         let mut executor = Executor::open(&exec_config)?;
//!         let mut node = executor.create_node("talker")?;
//!         // ... create publishers, subscriptions, services, actions
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
// path via `<QemuArmVirt as nros_platform::BoardEntry>::run`.
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
pub struct QemuArmVirt;

pub use node::init_hardware;

// Issue #130 — the shared public eth0-config entry point + slirp defaults, so
// the C `nros-nuttx-ffi` entry can push the guest IP into `eth0` before
// `app_main()` exactly as the Rust `BoardEntry` path does (no drift, one impl).
#[cfg(target_os = "nuttx")]
pub use entry_212n::configure_entry_eth0;
pub use entry_212n::{SLIRP_DEFAULT_GATEWAY, SLIRP_DEFAULT_IP, SLIRP_DEFAULT_PREFIX};
