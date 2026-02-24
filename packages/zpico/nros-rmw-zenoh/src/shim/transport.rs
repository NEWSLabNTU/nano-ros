//! ZenohTransport and ZenohRmw implementations

use nros_rmw::{Rmw, RmwConfig, Transport, TransportConfig, TransportError};

use super::session::ZenohSession;

// ============================================================================
// ZenohTransport
// ============================================================================

/// Zenoh transport backend for embedded platforms
///
/// Uses nros-rmw-zenoh for a simplified API suitable for bare-metal systems.
pub struct ZenohTransport;

impl Transport for ZenohTransport {
    type Error = TransportError;
    type Session = ZenohSession;

    fn open(config: &TransportConfig) -> Result<Self::Session, Self::Error> {
        ZenohSession::new(config)
    }
}

// ============================================================================
// ZenohRmw
// ============================================================================

/// Zenoh-pico RMW backend for compile-time middleware selection.
///
/// Implements the [`Rmw`] factory trait, bridging from the
/// middleware-agnostic [`RmwConfig`] to zenoh-pico session initialization.
///
/// # Example
///
/// ```ignore
/// use nros_rmw::{Rmw, RmwConfig, SessionMode};
/// use nros_rmw_zenoh::ZenohRmw;
///
/// let config = RmwConfig {
///     locator: "tcp/192.168.1.1:7447",
///     mode: SessionMode::Client,
///     domain_id: 0,
///     node_name: "talker",
///     namespace: "",
/// };
/// let session = ZenohRmw::open(&config).unwrap();
/// ```
pub struct ZenohRmw;

impl Rmw for ZenohRmw {
    type Session = ZenohSession;
    type Error = TransportError;

    fn open(config: &RmwConfig) -> Result<Self::Session, Self::Error> {
        let transport_config = TransportConfig {
            locator: Some(config.locator),
            mode: config.mode,
            properties: &[],
        };
        ZenohSession::new(&transport_config)
    }
}
