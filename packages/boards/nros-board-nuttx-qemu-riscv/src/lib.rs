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
//! use nros_board_nuttx_qemu_riscv::{Config, run};
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
// path via `<QemuRvVirt as nros_platform::BoardEntry>::run`.
mod entry_212n;
mod node;

pub use config::Config;

/// Per-board marker for trait dispatch.
///
/// Phase 313 W-nuttx (#0243) — the legacy `nros_board_common::board_init` impls
/// (`BoardInit`/`BoardPrint`/`BoardExit` + the free `node::run`) are RETIRED for
/// this board; the live path is the `nros_platform::board` trait set in
/// [`mod entry_212n`].
pub struct QemuRvVirt;

pub use node::init_hardware;
