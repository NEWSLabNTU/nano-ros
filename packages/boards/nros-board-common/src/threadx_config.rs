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
}
