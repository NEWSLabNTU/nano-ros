//! Configuration for ThreadX QEMU RISC-V 64-bit virt nodes
//!
//! The presets are QEMU user-mode (slirp) networking's DEFAULT plan —
//! `10.0.2.0/24`, host (and router) at `10.0.2.2` — which is what every
//! launcher of this board provides (`nros_tests::qemu::start_riscv64_virt` is
//! plain `-netdev user`), what the board's `NROS_APP_CONFIG` emitters bring
//! NetX up on, and what the entry lane's locator default names
//! (`cmake/NanoRosEntryLocator.cmake`). Until issue 1557 they were the
//! threadx-linux TAP-bridge plan (`192.0.3.0/24`), a network nothing here
//! launches any more.
//!
//! phase-337 W4.b — the `{mac, ip, netmask, gateway, locator, domain_id}`
//! core, the `nros.toml` scanner and the three `no_std` parsers now come from
//! [`nros_board_common::BaseConfig`]. This board adds no fields of its own
//! (bare-metal NetX Duo + virtio-net: there is no host interface to name), so
//! `Config` is a newtype whose job is to carry the board's DEFAULTS and the
//! trait impls the family driver dispatches on.

use nros_board_common::{
    BaseConfig,
    base_config::{for_each_toml_field, parse_u32},
};

/// Network and node configuration for ThreadX QEMU RISC-V.
///
/// # Default (Talker)
///
/// - IP: 10.0.2.40/24, Gateway: 10.0.2.2 (QEMU slirp's host)
/// - Zenoh: `tcp/10.0.2.2:7447`
/// - MAC: 52:54:00:12:34:56 (QEMU default)
#[derive(Clone)]
pub struct Config {
    /// The shared network/node core. Boards compose it rather than extend it.
    pub base: BaseConfig,
}

impl Default for Config {
    fn default() -> Self {
        // Issue #214 — build-env DOMAIN bake. The CMake path boots via
        // `run_app_thread` with NO deploy overlay, so this domain drives the
        // Executor/Cyclone participant. The NetX wire identity (IP/MAC) and the
        // locator on that path come from the image itself (`image_config()` in
        // lib.rs, issue 1557), not from these defaults. `NROS_DOMAIN_ID` is set
        // per-build by `nros_threadx_rv64_rust_app` (corrosion env), matching
        // the C fixtures' `-DNROS_DOMAIN_ID` bake.
        //
        // `option_env!` deliberately stays in the BOARD crate rather than
        // moving to `BaseConfig`: it is expanded where it is written, and the
        // shared crate is built once for every board — reading the bake there
        // would leak one fixture's domain into all of them.
        let domain_id = option_env!("NROS_DOMAIN_ID")
            .and_then(parse_u32)
            .unwrap_or(0);
        Self {
            base: BaseConfig {
                mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56],
                ip: [10, 0, 2, 40],
                netmask: [255, 255, 255, 0],
                gateway: [10, 0, 2, 2],
                zenoh_locator: "tcp/10.0.2.2:7447",
                domain_id,
            },
        }
    }
}

impl Config {
    /// Preset for a listener/subscriber node.
    pub fn listener() -> Self {
        Self {
            base: BaseConfig {
                mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x57],
                ip: [10, 0, 2, 41],
                netmask: [255, 255, 255, 0],
                gateway: [10, 0, 2, 2],
                zenoh_locator: "tcp/10.0.2.2:7447",
                domain_id: 0,
            },
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

    /// Builder: set the transport locator the RMW backend connects
    /// through (e.g. `"tcp/10.0.2.2:7447"`,
    /// `"serial/UART_0#baudrate=115200"`).
    pub fn with_locator(mut self, locator: &'static str) -> Self {
        self.base.zenoh_locator = locator;
        self
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
    /// ip = "10.0.2.40/24"
    /// mac = "52:54:00:12:34:56"
    /// gateway = "10.0.2.2"
    /// locator = "tcp/10.0.2.2:7447"
    ///
    /// [node]
    /// domain_id = 0
    /// ```
    pub fn from_toml(toml: &'static str) -> Self {
        let mut config = Self::default();
        // This board has no board-specific keys, so every field the scanner
        // yields is either a `BaseConfig` one or ignored.
        for_each_toml_field(toml, |section, key, value| {
            config.base.apply_toml_field(section, key, value);
        });

        // Phase 177.38 — build-time ROS-domain override for per-fixture
        // isolation. `NROS_DOMAIN_ID` set at build time bakes a distinct domain
        // into this fixture without editing the toml (the example default stays
        // clean). Cyclone derives RTPS ports from the domain, so build-fixtures
        // gives each communicating role-set its own domain and concurrent
        // fixtures don't collide. Empty/unset keeps the config value.
        if let Some(d) = option_env!("NROS_DOMAIN_ID").and_then(parse_u32) {
            config.base.domain_id = d;
        }

        config
    }
}

// Phase 173.5 — nros.toml `[[transport]]` IP into the board `Config`
// (NanoRosOwned: this board owns the NetX Duo stack). No UART field ⇒
// baudrate keeps the trait's no-op default.
