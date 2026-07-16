//! Configuration for ThreadX QEMU RISC-V 64-bit virt nodes
//!
//! Same IP presets as the ThreadX Linux board crate, designed for the
//! TAP bridge topology used by QEMU E2E tests.

/// Network and node configuration for ThreadX QEMU RISC-V.
///
/// # Default (Talker)
///
/// - IP: 192.0.3.10/24, Gateway: 192.0.3.1
/// - Zenoh: `tcp/192.0.3.1:7447`
/// - MAC: 52:54:00:12:34:56 (QEMU default)
#[derive(Clone)]
pub struct Config {
    /// MAC address
    pub mac: [u8; 6],
    /// IP address
    pub ip: [u8; 4],
    /// Network mask
    pub netmask: [u8; 4],
    /// Gateway IP
    pub gateway: [u8; 4],
    /// Zenoh locator string
    pub zenoh_locator: &'static str,
    /// ROS 2 domain ID (default: 0)
    pub domain_id: u32,
}

impl Default for Config {
    fn default() -> Self {
        // Issue #214 — build-env DOMAIN bake. The CMake/CycloneDDS path boots
        // via `run_app_thread(Config::default(), ...)` with NO deploy overlay,
        // so this domain drives the Executor/Cyclone participant; the NetX
        // wire identity (IP/MAC) on that path comes from the cmake-generated
        // `NROS_APP_CONFIG` (`NROS_APP_NET_{IP,MAC}_LAST` cache vars applied
        // by startup.c BEFORE the kernel), not from this struct. `NROS_DOMAIN_ID`
        // is set per-build by `nros_threadx_rv64_rust_cyclone_app` (corrosion
        // env), matching the C fixtures' `-DNROS_DOMAIN_ID` bake. The zenoh
        // path is unaffected: its deploy overlay overrides after `default()`.
        let domain_id = option_env!("NROS_DOMAIN_ID")
            .and_then(parse_u32)
            .unwrap_or(0);
        Self {
            mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56],
            ip: [192, 0, 3, 10],
            netmask: [255, 255, 255, 0],
            gateway: [192, 0, 3, 1],
            zenoh_locator: "tcp/192.0.3.1:7447",
            domain_id,
        }
    }
}

impl Config {
    /// Preset for a listener/subscriber node.
    pub fn listener() -> Self {
        Self {
            mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x57],
            ip: [192, 0, 3, 11],
            netmask: [255, 255, 255, 0],
            gateway: [192, 0, 3, 1],
            zenoh_locator: "tcp/192.0.3.1:7447",
            domain_id: 0,
        }
    }

    /// Alias for `Config::default()`.
    pub fn talker() -> Self {
        Self::default()
    }

    /// Builder: set MAC address.
    pub fn with_mac(mut self, mac: [u8; 6]) -> Self {
        self.mac = mac;
        self
    }

    /// Builder: set IP address.
    pub fn with_ip(mut self, ip: [u8; 4]) -> Self {
        self.ip = ip;
        self
    }

    /// Builder: set network mask.
    pub fn with_netmask(mut self, netmask: [u8; 4]) -> Self {
        self.netmask = netmask;
        self
    }

    /// Builder: set gateway.
    pub fn with_gateway(mut self, gateway: [u8; 4]) -> Self {
        self.gateway = gateway;
        self
    }

    /// Builder: set zenoh locator.
    pub fn with_zenoh_locator(mut self, locator: &'static str) -> Self {
        self.zenoh_locator = locator;
        self
    }

    /// Builder: set ROS 2 domain ID.
    pub fn with_domain_id(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
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
    /// [network]
    /// ip = "192.0.3.10"
    /// mac = "52:54:00:12:34:56"
    /// gateway = "192.0.3.1"
    /// netmask = "255.255.255.0"
    ///
    /// [zenoh]
    /// locator = "tcp/192.0.3.1:7447"
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
                    // Phase 172.K — direct-mode nros.toml only; legacy
                    // `[network]`/`[zenoh]` arms dropped in K.6.
                    ("transport", "ip") => {
                        let (addr, pfx) = value.split_once('/').unwrap_or((value, ""));
                        if let Some(ip) = parse_ipv4(addr) {
                            config.ip = ip;
                        }
                        if let Some(p) = parse_u32(pfx) {
                            config.netmask = prefix_to_netmask(p as u8);
                        }
                    }
                    ("transport", "mac") => {
                        if let Some(mac) = parse_mac(value) {
                            config.mac = mac;
                        }
                    }
                    ("transport", "gateway") => {
                        if let Some(gw) = parse_ipv4(value) {
                            config.gateway = gw;
                        }
                    }
                    ("transport", "locator") => {
                        config.zenoh_locator = value;
                    }
                    ("node", "domain_id") => {
                        if let Some(d) = parse_u32(value) {
                            config.domain_id = d;
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
            config.domain_id = d;
        }

        config
    }
}

// ── Minimal no_std parsers ──────────────────────────────────────────────

/// Convert a CIDR prefix length (0..=32) to a dotted netmask (Phase 172.K).
fn prefix_to_netmask(prefix: u8) -> [u8; 4] {
    let p = prefix.min(32);
    let mask: u32 = if p == 0 { 0 } else { u32::MAX << (32 - p) };
    mask.to_be_bytes()
}

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

/// Parse a MAC address string ("02:00:00:00:00:00") into [u8; 6].
fn parse_mac(s: &str) -> Option<[u8; 6]> {
    let mut result = [0u8; 6];
    let mut byte_idx = 0;

    for part in s.split(':') {
        if byte_idx >= 6 || part.len() != 2 {
            return None;
        }
        result[byte_idx] = parse_hex_byte(part)?;
        byte_idx += 1;
    }

    if byte_idx == 6 { Some(result) } else { None }
}

/// Parse a two-character hex string ("0a") into a u8.
fn parse_hex_byte(s: &str) -> Option<u8> {
    let bytes = s.as_bytes();
    if bytes.len() != 2 {
        return None;
    }
    let hi = hex_digit(bytes[0])?;
    let lo = hex_digit(bytes[1])?;
    Some(hi * 16 + lo)
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
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

// Phase 173.5 — nros.toml `[[transport]]` IP into the board `Config`
// (NanoRosOwned: this board owns the NetX Duo stack). ThreadX stores a
// dotted netmask, so the prefix is expanded; no UART field ⇒ baudrate
// keeps the trait's no-op default.
impl nros_platform::BoardTransportConfig for Config {
    fn set_ipv4(&mut self, addr: [u8; 4], prefix: u8) {
        self.ip = addr;
        let bits = u32::from(prefix.min(32));
        let mask = if bits == 0 {
            0
        } else {
            u32::MAX << (32 - bits)
        };
        self.netmask = mask.to_be_bytes();
    }
}
