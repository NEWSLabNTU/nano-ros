//! Configuration for ThreadX Linux simulation nodes
//!
//! Same IP presets as the FreeRTOS board crate (`nros-board-mps2-an385-freertos`),
//! designed for the bridge topology used by ThreadX E2E tests.
//!
//! ThreadX Linux uses veth pairs (not TAP devices) because the NetX Duo Linux
//! network driver uses AF_PACKET/SOCK_RAW, which doesn't work correctly on TAP
//! devices with a bridge (traffic routes through the TAP fd instead of the bridge).
//! veth pairs are purely kernel-side and work correctly with bridges and AF_PACKET.
//!
//! phase-337 W4.c — the `{mac, ip, netmask, gateway, locator, domain_id}` core,
//! the `nros.toml` scanner and the three `no_std` parsers come from
//! [`nros_board_common::BaseConfig`]. `interface` stays HERE: it is a fact
//! about this board (a host NIC name), and pushing it into the shared type
//! would make every board carry a field it ignores.

use nros_board_common::BaseConfig;
use nros_board_common::base_config::{for_each_toml_field, parse_u32};

/// Network and node configuration for ThreadX Linux simulation.
///
/// # Default (Talker)
///
/// - IP: 192.0.3.10/24, Gateway: 192.0.3.1
/// - Zenoh: `tcp/192.0.3.1:7447`
/// - Interface: `veth-tx0`
#[derive(Clone)]
pub struct Config {
    /// The shared network/node core. Boards compose it rather than extend it.
    pub base: BaseConfig,
    /// Linux network interface name (veth for ThreadX Linux simulation).
    pub interface: &'static str,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base: BaseConfig {
                mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
                ip: [192, 0, 3, 10],
                netmask: [255, 255, 255, 0],
                gateway: [192, 0, 3, 1],
                zenoh_locator: "tcp/192.0.3.1:7447",
                domain_id: 0,
            },
            interface: "veth-tx0",
        }
    }
}

impl Config {
    /// Preset for a listener/subscriber node.
    pub fn listener() -> Self {
        Self {
            base: BaseConfig {
                mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
                ip: [192, 0, 3, 11],
                netmask: [255, 255, 255, 0],
                gateway: [192, 0, 3, 1],
                zenoh_locator: "tcp/192.0.3.1:7447",
                domain_id: 0,
            },
            interface: "veth-tx1",
        }
    }

    /// Alias for `Config::default()`.
    pub fn talker() -> Self {
        Self::default()
    }

    /// Builder: set MAC address.
    pub fn with_mac(mut self, mac: [u8; 6]) -> Self {
        self.base.mac = mac;
        self
    }

    /// Builder: set IP address.
    pub fn with_ip(mut self, ip: [u8; 4]) -> Self {
        self.base.ip = ip;
        self
    }

    /// Builder: set network mask.
    pub fn with_netmask(mut self, netmask: [u8; 4]) -> Self {
        self.base.netmask = netmask;
        self
    }

    /// Builder: set gateway.
    pub fn with_gateway(mut self, gateway: [u8; 4]) -> Self {
        self.base.gateway = gateway;
        self
    }

    /// Builder: set Linux network interface name.
    pub fn with_interface(mut self, interface: &'static str) -> Self {
        self.interface = interface;
        self
    }

    /// Builder: set the transport locator the RMW backend connects
    /// through (e.g. `"tcp/192.0.3.1:7447"`,
    /// `"serial/UART_0#baudrate=115200"`).
    pub fn with_locator(mut self, locator: &'static str) -> Self {
        self.base.zenoh_locator = locator;
        self
    }

    /// Deprecated alias for [`with_locator`](Self::with_locator).
    #[deprecated(
        since = "0.6.0",
        note = "renamed to `with_locator()` — the config API must not name a backend (issue 0330)"
    )]
    pub fn with_zenoh_locator(self, locator: &'static str) -> Self {
        self.with_locator(locator)
    }

    /// Builder: set ROS 2 domain ID.
    pub fn with_domain_id(mut self, domain_id: u32) -> Self {
        self.base.domain_id = domain_id;
        self
    }

    /// Parse configuration from a direct-mode `nros.toml` string.
    ///
    /// Missing fields keep this board's defaults. Designed for
    /// `include_str!("../nros.toml")` compile-time embedding.
    ///
    /// ```toml
    /// [[transport]]
    /// ip = "192.0.3.10/24"
    /// mac = "02:00:00:00:00:00"
    /// gateway = "192.0.3.1"
    /// interface = "veth-tx0"
    /// locator = "tcp/192.0.3.1:7447"
    ///
    /// [node]
    /// domain_id = 0
    /// ```
    pub fn from_toml(toml: &'static str) -> Self {
        let mut config = Self::default();
        for_each_toml_field(toml, |section, key, value| {
            if config.base.apply_toml_field(section, key, value) {
                return;
            }
            // The one field this board adds to the shared core.
            if (section, key) == ("transport", "interface") {
                config.interface = value;
            }
        });

        // Phase 177.38 — build-time ROS-domain override for per-fixture
        // isolation. `NROS_DOMAIN_ID` set at build time bakes a distinct domain
        // into this fixture without editing the toml (the example default stays
        // clean). Cyclone derives RTPS ports from the domain, so build-fixtures
        // gives each communicating role-set its own domain and concurrent
        // fixtures don't collide. Empty/unset keeps the config value.
        //
        // `option_env!` stays in the BOARD crate: it is expanded where it is
        // written, and the shared crate is built once for every board.
        if let Some(d) = option_env!("NROS_DOMAIN_ID").and_then(parse_u32) {
            config.base.domain_id = d;
        }

        config
    }
}

// Phase 173.5 — nros.toml `[[transport]]` IP into the board `Config`
// (NanoRosOwned: this board owns the NetX Duo stack). No UART field ⇒
// baudrate keeps the trait's no-op default.
