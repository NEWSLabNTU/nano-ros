//! Network and node configuration for the NuttX QEMU boards.
//!
//! phase-337 W3.a — composed on [`nros_board_common::BaseConfig`] rather than
//! re-declaring the `{ip, netmask, gateway, locator, domain_id}` core. NuttX
//! owns no MAC (the virtio-net device takes its address from QEMU), so the
//! board reads every other field off the shared type and ignores `mac`.
//!
//! The board thinks in CIDR (`prefix`), because `SIOCSIFNETMASK` is fed from a
//! prefix in [`crate::node`]; `BaseConfig` stores the netmask because that is
//! what `DeployOverlay` carries. `prefix()` / `with_prefix()` bridge the two,
//! which is exactly the conversion the shared type exists to own.

use nros_board_common::BaseConfig;

/// Network and node configuration for the QEMU NuttX boards.
///
/// # Default Configuration (Talker)
///
/// - IP: 192.0.3.10/24
/// - Gateway: 192.0.3.1
/// - Zenoh: tcp/192.0.3.1:7447
///
/// # Listener Configuration
///
/// - IP: 192.0.3.11/24
/// - Gateway: 192.0.3.1
/// - Zenoh: tcp/192.0.3.1:7447
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// The shared network/node core (RFC-0064 / phase-337 W1.b).
    pub base: BaseConfig,
}

impl Default for Config {
    /// The QEMU-bridge scheme every emulated board shares — i.e. exactly
    /// [`BaseConfig::default`]. Spelled through the shared default rather than
    /// re-listed, so a board that genuinely differs is visible as an override.
    fn default() -> Self {
        Self {
            base: BaseConfig::default(),
        }
    }
}

impl Config {
    /// IP address.
    pub const fn ip(&self) -> [u8; 4] {
        self.base.ip
    }

    /// Network prefix length.
    pub const fn prefix(&self) -> u8 {
        self.base.prefix()
    }

    /// Gateway IP.
    pub const fn gateway(&self) -> [u8; 4] {
        self.base.gateway
    }

    /// Transport locator the RMW backend dials.
    pub const fn locator(&self) -> &'static str {
        self.base.zenoh_locator
    }

    /// ROS 2 domain ID.
    pub const fn domain_id(&self) -> u32 {
        self.base.domain_id
    }

    /// Configuration preset for a listener/subscriber node.
    pub fn listener() -> Self {
        Self::default().with_ip([192, 0, 3, 11])
    }

    /// Alias for `Config::default()`.
    pub fn talker() -> Self {
        Self::default()
    }

    /// Configuration preset for a service/action server node.
    pub fn server() -> Self {
        Self::default().with_ip([192, 0, 3, 12])
    }

    /// Configuration preset for a service/action client node.
    pub fn client() -> Self {
        Self::default().with_ip([192, 0, 3, 13])
    }

    /// Builder: set IP address.
    pub const fn with_ip(mut self, ip: [u8; 4]) -> Self {
        self.base.ip = ip;
        self
    }

    /// Builder: set network prefix length.
    pub const fn with_prefix(mut self, prefix: u8) -> Self {
        self.base = self.base.with_prefix(prefix);
        self
    }

    /// Builder: set gateway.
    pub const fn with_gateway(mut self, gateway: [u8; 4]) -> Self {
        self.base.gateway = gateway;
        self
    }

    /// Builder: set the transport locator the RMW backend connects
    /// through (e.g. `"tcp/192.0.3.1:7447"`,
    /// `"serial/UART_0#baudrate=115200"`).
    pub const fn with_locator(mut self, locator: &'static str) -> Self {
        self.base.zenoh_locator = locator;
        self
    }

    /// Builder: set ROS 2 domain ID.
    pub const fn with_domain_id(mut self, domain_id: u32) -> Self {
        self.base.domain_id = domain_id;
        self
    }

    /// Parse configuration from a TOML string.
    ///
    /// Missing fields use board-specific defaults. This is designed to work
    /// with `include_str!("../config.toml")` for compile-time embedding.
    ///
    /// # Supported fields
    ///
    /// ```toml
    /// [transport]
    /// ip = "192.0.3.10/24"
    /// gateway = "192.0.3.1"
    /// locator = "tcp/192.0.3.1:7447"
    ///
    /// [node]
    /// domain_id = 0
    /// ```
    pub fn from_toml(toml: &'static str) -> Self {
        let mut config = Self::default();
        let mut section = "";

        for line in toml.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Phase 172.K — `[[transport]]` array-of-tables + dotted sections.
            if line.starts_with('[') {
                if line.starts_with("[[") {
                    if let Some(end) = line.find("]]") {
                        section = line[2..end].trim();
                    }
                } else if let Some(end) = line.find(']') {
                    section = line[1..end].trim();
                }
                continue;
            }
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim();
                let value = line[eq_pos + 1..].trim();
                let value = if (value.starts_with('"') && value.ends_with('"'))
                    || (value.starts_with('\'') && value.ends_with('\''))
                {
                    &value[1..value.len() - 1]
                } else {
                    value
                };

                match (section, key) {
                    // Phase 172.K — direct-mode nros.toml only (NuttX owns no
                    // MAC); legacy `[network]`/`[zenoh]` arms dropped in K.6.
                    ("transport", "ip") => {
                        let (addr, pfx) = value.split_once('/').unwrap_or((value, ""));
                        if let Some(ip) = parse_ipv4(addr) {
                            config.base.ip = ip;
                        }
                        if let Some(p) = parse_u32(pfx) {
                            config = config.with_prefix(p as u8);
                        }
                    }
                    ("transport", "gateway") => {
                        if let Some(gw) = parse_ipv4(value) {
                            config.base.gateway = gw;
                        }
                    }
                    ("transport", "locator") => {
                        config.base.zenoh_locator = value;
                    }
                    ("node", "domain_id") => {
                        if let Some(d) = parse_u32(value) {
                            config.base.domain_id = d;
                        }
                    }
                    _ => {}
                }
            }
        }

        // Phase 177.38 — build-time ROS-domain override for per-fixture
        // isolation. `NROS_DOMAIN_ID` set at build time bakes a distinct domain
        // into this fixture without editing config.toml (the example default
        // stays clean). Cyclone derives RTPS ports from the domain, so
        // build-fixtures gives each communicating role-set its own domain and
        // concurrent fixtures don't collide. Empty/unset keeps the config value.
        if let Some(d) = option_env!("NROS_DOMAIN_ID").and_then(parse_u32) {
            config.base.domain_id = d;
        }

        config
    }
}

// ── Minimal no_std parsers ──────────────────────────────────────────────

/// Parse an IPv4 address string ("192.0.3.10") into [u8; 4].
fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let mut result = [0u8; 4];
    let mut octet_idx = 0;
    let mut current: u16 = 0;
    let mut has_digit = false;

    for b in s.as_bytes() {
        match b {
            b'0'..=b'9' => {
                current = current * 10 + (*b - b'0') as u16;
                if current > 255 {
                    return None;
                }
                has_digit = true;
            }
            b'.' => {
                if !has_digit || octet_idx >= 3 {
                    return None;
                }
                result[octet_idx] = current as u8;
                octet_idx += 1;
                current = 0;
                has_digit = false;
            }
            _ => return None,
        }
    }

    if has_digit && octet_idx == 3 {
        result[3] = current as u8;
        Some(result)
    } else {
        None
    }
}

/// Parse a decimal integer string.
fn parse_u32(s: &str) -> Option<u32> {
    let mut result: u32 = 0;
    let mut has_digit = false;
    for b in s.as_bytes() {
        match b {
            b'0'..=b'9' => {
                result = result.checked_mul(10)?.checked_add((*b - b'0') as u32)?;
                has_digit = true;
            }
            _ => return None,
        }
    }
    if has_digit { Some(result) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_the_shared_base_config() {
        let c = Config::default();
        assert_eq!(c.ip(), [192, 0, 3, 10]);
        assert_eq!(c.gateway(), [192, 0, 3, 1]);
        assert_eq!(c.prefix(), 24);
        assert_eq!(c.domain_id(), 0);
    }

    /// The prefix↔netmask bridge is the whole reason this board composes on
    /// `BaseConfig` rather than keeping its own `prefix: u8` field: the board
    /// feeds `SIOCSIFNETMASK` from a prefix while `DeployOverlay` carries a
    /// netmask, and exactly one conversion should exist.
    #[test]
    fn prefix_round_trips_through_the_shared_netmask() {
        for p in [0u8, 8, 16, 24, 30, 32] {
            assert_eq!(Config::default().with_prefix(p).prefix(), p);
        }
    }

    #[test]
    fn role_presets_differ_only_in_the_host_octet() {
        for (c, last) in [
            (Config::talker(), 10u8),
            (Config::listener(), 11),
            (Config::server(), 12),
            (Config::client(), 13),
        ] {
            assert_eq!(c.ip(), [192, 0, 3, last]);
            assert_eq!(c.gateway(), Config::default().gateway());
        }
    }

    #[test]
    fn from_toml_reads_the_direct_mode_transport_section() {
        let c = Config::from_toml(
            "[transport]\nip = \"10.0.2.30/24\"\ngateway = \"10.0.2.2\"\n\
             locator = \"tcp/10.0.2.2:7447\"\n[node]\ndomain_id = 7\n",
        );
        assert_eq!(c.ip(), [10, 0, 2, 30]);
        assert_eq!(c.prefix(), 24);
        assert_eq!(c.gateway(), [10, 0, 2, 2]);
        assert_eq!(c.locator(), "tcp/10.0.2.2:7447");
        // NROS_DOMAIN_ID is unset in a plain `cargo test`, so the TOML wins.
        if option_env!("NROS_DOMAIN_ID").is_none() {
            assert_eq!(c.domain_id(), 7);
        }
    }
}
