//! Configuration for NuttX QEMU ARM virt nodes
//!
//! Provides sensible defaults for the QEMU virt machine with TAP networking.
//! Same IP scheme as bare-metal QEMU board crates (192.0.3.0/24 on br-qemu).

/// Network and node configuration for QEMU ARM virt.
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
#[derive(Clone, Debug)]
pub struct Config {
    /// IP address
    pub ip: [u8; 4],
    /// Network prefix length (default: 24)
    pub prefix: u8,
    /// Gateway IP
    pub gateway: [u8; 4],
    /// Zenoh locator
    pub zenoh_locator: &'static str,
    /// ROS 2 domain ID (default: 0)
    pub domain_id: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ip: [192, 0, 3, 10],
            prefix: 24,
            gateway: [192, 0, 3, 1],
            zenoh_locator: "tcp/192.0.3.1:7447",
            domain_id: 0,
        }
    }
}

impl Config {
    /// Configuration preset for a listener/subscriber node.
    pub fn listener() -> Self {
        Self {
            ip: [192, 0, 3, 11],
            ..Self::default()
        }
    }

    /// Alias for `Config::default()`.
    pub fn talker() -> Self {
        Self::default()
    }

    /// Configuration preset for a service/action server node.
    pub fn server() -> Self {
        Self {
            ip: [192, 0, 3, 12],
            ..Self::default()
        }
    }

    /// Configuration preset for a service/action client node.
    pub fn client() -> Self {
        Self {
            ip: [192, 0, 3, 13],
            ..Self::default()
        }
    }

    /// Builder: set IP address.
    pub fn with_ip(mut self, ip: [u8; 4]) -> Self {
        self.ip = ip;
        self
    }

    /// Builder: set network prefix length.
    pub fn with_prefix(mut self, prefix: u8) -> Self {
        self.prefix = prefix;
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
    /// gateway = "192.0.3.1"
    /// prefix = 24
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
            if line.starts_with('[') {
                if let Some(end) = line.find(']') {
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
                    ("network", "ip") => {
                        if let Some(ip) = parse_ipv4(value) {
                            config.ip = ip;
                        }
                    }
                    ("network", "gateway") => {
                        if let Some(gw) = parse_ipv4(value) {
                            config.gateway = gw;
                        }
                    }
                    ("network", "prefix") => {
                        if let Some(p) = parse_u32(value) {
                            config.prefix = p as u8;
                        }
                    }
                    ("zenoh", "locator") => {
                        config.zenoh_locator = value;
                    }
                    ("zenoh", "domain_id") => {
                        if let Some(d) = parse_u32(value) {
                            config.domain_id = d;
                        }
                    }
                    _ => {}
                }
            }
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
