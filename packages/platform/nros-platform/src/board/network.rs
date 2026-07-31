//! [`NetworkWait`] — Phase 212.N.1 mixin.
//!
//! Gate `BoardEntry::run` on carrier / DHCP / link-up before it
//! tries to open RMW sessions. Boards without an L3 stack (CAN-only,
//! serial-only, IVC-only) leave it unimplemented.

/// Carrier / DHCP / link-up gate for IP-aware transports.
pub trait NetworkWait: super::Board {
    /// Block until the board's IP stack is ready: carrier detected,
    /// DHCP lease acquired (or static IP applied), default route
    /// installed. Returns when the executor can open sockets.
    ///
    /// Returning `Err` ends boot via
    /// [`super::BoardExit::exit_failure`]. `Ok` proceeds into the
    /// `setup` callback.
    fn wait_link_up() -> Result<(), NetworkError>;
}

/// Network bringup failure mode (matches the coarse shape of
/// [`super::transport::TransportError`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkError {
    /// PHY linked but no DHCP lease before the board's deadline.
    DhcpTimeout,
    /// Static-IP configuration referenced a non-existent interface
    /// or duplicate address.
    ConfigInvalid,
    /// No default route or gateway unreachable.
    NoRoute,
    /// Board-specific failure not covered by the above.
    Other,
}
