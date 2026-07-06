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

/// Phase 152.4.B — `BoardInit` impl for the QEMU ARM virt board.
///
/// Provides `Config` + `init_hardware` per the kernel-agnostic
/// contract `nros_board_common::BoardInit`. The
/// `nros_board_nuttx::run_generic<B>` shim consumes this so future
/// NuttX overlays plug into the same boot path.
pub struct QemuArmVirt;

impl nros_board_common::BoardInit for QemuArmVirt {
    type Config = Config;

    fn init_hardware(cfg: &Config) {
        // Mirrors `node::init_hardware`: re-seed /dev/urandom +
        // override defconfig-baked IP via ioctl.
        node::init_hardware(cfg);
    }
}

/// Phase 173.1 — complete the `Board` super-trait. NuttX targets ship
/// `std`, so printing + exit route through the hosted stdlib (same
/// primitives `node::run` already uses).
impl nros_board_common::BoardPrint for QemuArmVirt {
    fn println(args: core::fmt::Arguments<'_>) {
        println!("{args}");
    }
}

impl nros_board_common::BoardExit for QemuArmVirt {
    fn exit_success() -> ! {
        std::process::exit(0)
    }

    fn exit_failure() -> ! {
        std::process::exit(1)
    }
}

pub use node::{init_hardware, run};

// Issue #130 — the shared public eth0-config entry point + slirp defaults, so
// the C `nros-nuttx-ffi` entry can push the guest IP into `eth0` before
// `app_main()` exactly as the Rust `BoardEntry` path does (no drift, one impl).
#[cfg(target_os = "nuttx")]
pub use entry_212n::configure_entry_eth0;
pub use entry_212n::{SLIRP_DEFAULT_GATEWAY, SLIRP_DEFAULT_IP, SLIRP_DEFAULT_PREFIX};
