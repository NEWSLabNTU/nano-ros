//! Phase 152.2.B.4 — `ThreadxConfig` trait.
//!
//! Generic `nros_board_threadx::run<B>` (lifted alongside this
//! trait) needs uniform access to the network + RMW config bits
//! while letting each per-board `Config` carry overlay-specific
//! extensions. The trait is the narrow waist; `BoardInit::Config`
//! is bound to it via the generic crate's `run<B>` `where` clause.
//!
//! Today's two reference overlays:
//!
//! - `nros-board-threadx-linux` — has `interface: &'static str`
//!   (veth name for the NSOS BSD shim's host network).
//! - `nros-board-threadx-qemu-riscv64` — no `interface` (bare
//!   metal, no host network).
//!
//! `interface()` returns `Option<&str>` so bare-metal boards
//! drop the field cleanly. The C-side FFI signature unified on
//! the 5-arg form (`ip, netmask, gateway, mac, interface_name`)
//! and ignores `interface_name = NULL` on bare-metal overlays.

/// The router a threadx-linux image dials when nothing states one: the image's
/// `system.toml` `locator`, the cmake road's `NROS_ENTRY_LOCATOR`, or a run-time
/// `$NROS_LOCATOR`, in that order of what a build reaches first.
///
/// Issue 1793: this board stated its fallback three times and they named two
/// routers. The Rust `Config::default()` said `tcp/192.0.3.1:7447` (the
/// `qemu-br` bridge gateway) while both C emitters said `tcp/127.0.0.1:7555`.
/// The board's `[board.net]` is `stack = "host"` — NetX goes through the host's
/// sockets (nsos-netx), not a bridge — so loopback is the only address that is
/// reachable without root-provisioned interfaces, and 7447 is the port a stock
/// `rmw_zenohd` listens on. No in-tree image relies on the value: every
/// threadx-linux example and workspace row states its own locator.
///
/// ONE value, read by `Config::default()` and by the crate's build-script C
/// emitter; `cmake/board/nano-ros-board-threadx-linux.cmake` holds a literal
/// copy, which the test at the bottom of this file keeps equal.
pub const THREADX_LINUX_DEFAULT_LOCATOR: &str = "tcp/127.0.0.1:7447";

/// The same fallback for rv-virt-threadx, whose network is QEMU slirp:
/// `10.0.2.2` is the host as the guest sees it. Issue 1793: the Rust default
/// dialled port 7447 and both C emitters 7553. 7447 is the stock router's port,
/// and no in-tree image relies on either — every Rust image states its
/// locator, every C/C++ zenoh row passes `NROS_ENTRY_LOCATOR`, and the one row
/// that passes neither (`errno-isolation`) opens no session. Read by
/// `Config::default()` and the build-script C emitter;
/// `cmake/board/nano-ros-board-rv-virt-threadx.cmake` holds a literal copy the
/// test below keeps equal.
pub const THREADX_QEMU_RISCV64_DEFAULT_LOCATOR: &str = "tcp/10.0.2.2:7447";

/// Read-only accessors over a per-overlay ThreadX `Config`. One
/// impl per overlay — same shape as `BoardInit::Config`, kept
/// in a separate trait so a future generic
/// `nros_board_threadx::run<B: BoardInit + BoardPrint + BoardExit>`
/// can add `B::Config: ThreadxConfig` without touching
/// `BoardInit`'s shape (which is shared across kernel families).
pub trait ThreadxConfig {
    fn mac(&self) -> &[u8; 6];
    fn ip(&self) -> &[u8; 4];
    fn netmask(&self) -> &[u8; 4];
    fn gateway(&self) -> &[u8; 4];

    /// Host network interface name (e.g. `"veth-tx0"`). Bare-
    /// metal overlays return `None`; the generic `run<B>` then
    /// passes `NULL` to `nros_threadx_set_config`'s `interface_name`
    /// parameter.
    fn interface(&self) -> Option<&str> {
        None
    }

    /// Phase 212.N.7 step-3.5 — locator + domain_id accessors so the
    /// family-driver `run_entry` body can pass them to
    /// `ExecutorConfig::new(...)` when opening the embedded
    /// `Executor`. Defaults match the historical
    /// `zenoh_locator = "tcp/127.0.0.1:7447"` / `domain_id = 0`
    /// the per-board overlays carry. Override on a per-overlay
    /// `Config` when the values come from a different source.
    ///
    /// Named `locator` (not `zenoh_locator`) to match
    /// `nros_platform::BoardConfig::locator`, the RMW vtable's own
    /// `locator` parameter and `ExecutorConfig::new` — this trait is
    /// RMW-agnostic and must not name a concrete backend (issue 0330).
    fn locator(&self) -> &'static str {
        "tcp/127.0.0.1:7447"
    }

    fn domain_id(&self) -> u32 {
        0
    }

    /// A bounded run's spin budget in milliseconds: the entry spins this long
    /// after setup, prints `nros: application complete` and exits 0. `None`
    /// (the default) spins for the life of the firmware.
    ///
    /// This is `$NROS_ENTRY_SPIN_MS`, the knob the linux board and the C/C++
    /// entry funnels already honour so a bounded observer (the runtime
    /// registration test, issue 1310) can run an entry and see it finish. Only
    /// a board that runs as a host process has an environment to read it from,
    /// so only such an overlay overrides this.
    fn entry_spin_ms(&self) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    // The crate is `no_std` without `build-helpers`; a test still has std.
    extern crate std;
    use std::{format, vec::Vec};

    /// Every `.locator` line of `module` must name `locator`. Spacing before
    /// `=` is ignored: the two modules align their fields differently.
    fn assert_cmake_locator(module: &str, locator: &str) {
        let path = format!(
            "{}/../../../cmake/board/{module}",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(&path).expect("read the board's cmake module");
        let want = format!(r#"\"{locator}\","#);
        let found: Vec<&str> = text.lines().filter(|l| l.contains(".locator")).collect();
        assert!(
            !found.is_empty() && found.iter().all(|l| l.contains(&want)),
            "{path}: its NROS_APP_CONFIG locator must be `{locator}`, found: {found:?}"
        );
    }

    /// Issue 1793 — the cmake road's `NROS_APP_CONFIG` emitters cannot read a
    /// Rust const, so each holds a literal; this keeps them the same router.
    #[test]
    fn cmake_threadx_fallback_locators_match_the_rust_ones() {
        assert_cmake_locator(
            "nano-ros-board-threadx-linux.cmake",
            super::THREADX_LINUX_DEFAULT_LOCATOR,
        );
        assert_cmake_locator(
            "nano-ros-board-rv-virt-threadx.cmake",
            super::THREADX_QEMU_RISCV64_DEFAULT_LOCATOR,
        );
    }
}
