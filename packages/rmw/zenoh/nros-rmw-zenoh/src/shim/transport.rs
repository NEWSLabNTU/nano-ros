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
/// let session = ZenohRmw::default().open(&config).unwrap();
/// ```
#[derive(Default)]
pub struct ZenohRmw;

impl Rmw for ZenohRmw {
    type Session = ZenohSession;
    type Error = TransportError;

    fn open(self, config: &RmwConfig) -> Result<Self::Session, Self::Error> {
        ZenohSession::new(&transport_config(config))
    }
}

/// The session config [`ZenohRmw::open`] hands the shim — every field of the
/// RMW config, `properties` included.
///
/// Issue 2051 — this read `properties: &[]`. Every zenoh session behind the C
/// seam opens here (`nros-rmw-cffi`'s Rust adapter marshals the caller's
/// properties into `RmwConfig` and calls this), so a bringup's lowered RMW
/// file, a C/C++ entry's `nros_cpp_set_session_properties` and phase-206 W3's
/// whole property carrier were all dropped one call short of zenoh-pico, with
/// no error: the session opened against the baked locator, which reads exactly
/// like a configuration that took effect.
fn transport_config<'a>(config: &RmwConfig<'a>) -> TransportConfig<'a> {
    TransportConfig {
        locator: Some(config.locator),
        mode: config.mode,
        properties: config.properties,
        node_name: config.node_name,
        namespace: config.namespace,
        domain_id: config.domain_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rmw_configs_session_properties_reach_the_shim() {
        let props = [("connect", "tcp/10.0.0.9:7499"), ("mode", "client")];
        let config = RmwConfig {
            locator: "tcp/127.0.0.1:7447",
            mode: nros_rmw::SessionMode::Client,
            domain_id: 3,
            node_name: "talker",
            namespace: "/ns",
            properties: &props,
        };
        let t = transport_config(&config);
        assert_eq!(t.properties, &props[..]);
        assert_eq!(t.locator, Some("tcp/127.0.0.1:7447"));
        assert_eq!(
            (t.domain_id, t.node_name, t.namespace),
            (3, "talker", "/ns")
        );
    }
}
