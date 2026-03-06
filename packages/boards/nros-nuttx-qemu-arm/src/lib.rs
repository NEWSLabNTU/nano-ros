//! # nros-nuttx-qemu-arm
//!
//! Board crate for running nros on NuttX QEMU ARM virt (Cortex-A7 + virtio-net).
//!
//! Handles platform configuration. Users call [`run()`] with a closure that
//! receives [`&Config`](Config) and creates an `Executor` for full API access
//! (publishers, subscriptions, services, actions, timers).
//!
//! # Architecture
//!
//! Unlike bare-metal board crates (`nros-mps2-an385`), this crate has no custom
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
//! use nros_nuttx_qemu_arm::{Config, run};
//!
//! fn main() {
//!     run(Config::default(), |config| {
//!         let exec_config = ExecutorConfig::new(config.zenoh_locator)
//!             .domain_id(config.domain_id)
//!             .node_name("talker");
//!         let mut executor = Executor::<_, 0, 0>::open(&exec_config)?;
//!         let mut node = executor.create_node("talker")?;
//!         // ... create publishers, subscriptions, services, actions
//!         Ok(())
//!     })
//! }
//! ```

mod config;
mod node;

pub use config::Config;
pub use node::{init_hardware, run};
