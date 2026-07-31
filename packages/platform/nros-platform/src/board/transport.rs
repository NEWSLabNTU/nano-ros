//! [`TransportBringup`] — Phase 212.N.1 mixin.
//!
//! Per-board transport bringup. Brings the link layer (Ethernet,
//! WiFi, CAN, serial UART, USB CDC, IVC, …) up to the point where
//! the executor's RMW can open sessions. Called by
//! `BoardEntry::run` after `BoardInit::init_hardware` and before
//! `NetworkWait::wait_link_up` (if implemented).
//!
//! ## Composition
//!
//! Boards pick one or several transports at the type system level
//! by implementing `TransportBringup` once per concrete board. A
//! board with multiple transports composes via an internal helper
//! (e.g. a `MultiTransport` newtype) rather than via blanket impls,
//! since each transport's bringup is sequential and order-sensitive
//! (`init_link` before `link_up`, sockets opened only after link).
//!
//! ## Status
//!
//! Phase 212.N.1 — trait surface only. 212.N.2 family driver crates
//! provide concrete impls.

/// Per-board transport bringup contract.
pub trait TransportBringup: super::Board {
    /// Bring the link layer up. Returns when the link is at L2
    /// (Ethernet frames flow / WiFi associated / UART open at
    /// baud / CAN bus listening) but BEFORE any L3/IP state. The
    /// [`super::NetworkWait`] mixin handles DHCP / link-up gating.
    ///
    /// Returning `Err` ends boot via [`super::BoardExit::exit_failure`].
    fn init_transport() -> Result<(), TransportError>;
}

/// Transport bringup failure mode.
///
/// Kept minimal on purpose — boards surface the gritty detail (NIC
/// register state, WiFi association code, UART error register)
/// through their own logging. The error returned by
/// [`TransportBringup::init_transport`] is a coarse signal to
/// `BoardEntry::run` that boot can't proceed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransportError {
    /// PHY didn't reach the linked state in the board's deadline.
    LinkDown,
    /// Driver init failed (NIC reset hung, WiFi chip enumeration
    /// failed, UART couldn't claim its pins).
    DriverInit,
    /// Transport hardware absent or not enabled at the board level.
    NotPresent,
    /// Board-specific failure not covered by the above. Boards
    /// using this should also log a richer message via
    /// [`super::BoardPrint::println`] before returning.
    Other,
}
