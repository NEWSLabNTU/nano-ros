//! Configuration for QEMU FreeRTOS nodes
//!
//! Same IP presets as the bare-metal board crate (`nros-mps2-an385`),
//! designed for the TAP bridge topology used by `just test-freertos`.

/// Network and node configuration for QEMU MPS2-AN385 + FreeRTOS.
///
/// # Default (Talker)
///
/// - IP: 192.0.3.10/24, Gateway: 192.0.3.1
/// - Zenoh: `tcp/192.0.3.1:7447`
#[derive(Clone)]
pub struct Config {
    /// MAC address (default: locally administered 02:00:00:00:00:00)
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

    // ── Scheduling ─────────────────────────────────────────────────────
    // Normalized 0–31 scale (higher = more important). Board crate maps
    // to FreeRTOS 0–(configMAX_PRIORITIES-1) via `to_freertos_priority()`.

    /// Application task priority (normalized 0–31, default 12).
    pub app_priority: u8,
    /// Application task stack size in bytes (default 65536).
    pub app_stack_bytes: u32,
    /// Zenoh-pico read task priority (normalized 0–31, default 16).
    pub zenoh_read_priority: u8,
    /// Zenoh-pico read task stack size in bytes (default 5120).
    pub zenoh_read_stack_bytes: u32,
    /// Zenoh-pico lease task priority (normalized 0–31, default 16).
    pub zenoh_lease_priority: u8,
    /// Zenoh-pico lease task stack size in bytes (default 5120).
    pub zenoh_lease_stack_bytes: u32,
    /// Network poll task priority (normalized 0–31, default 16).
    pub poll_priority: u8,
    /// Network poll interval in milliseconds (default 5).
    pub poll_interval_ms: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
            ip: [192, 0, 3, 10],
            netmask: [255, 255, 255, 0],
            gateway: [192, 0, 3, 1],
            zenoh_locator: "tcp/192.0.3.1:7447",
            domain_id: 0,
            // Scheduling defaults match the old hardcoded constants:
            // APP_TASK_PRIORITY=3 → normalized 12 (12*7/31 ≈ 2.7 → 3)
            // POLL_TASK_PRIORITY=4 → normalized 16 (16*7/31 ≈ 3.6 → 4)
            // zenoh read/lease default to 4 in zenoh-pico (configMAX_PRIORITIES/2)
            app_priority: 12,
            app_stack_bytes: 65536,
            zenoh_read_priority: 16,
            zenoh_read_stack_bytes: 5120,
            zenoh_lease_priority: 16,
            zenoh_lease_stack_bytes: 5120,
            poll_priority: 16,
            poll_interval_ms: 5,
        }
    }
}

impl Config {
    /// Preset for a listener/subscriber node.
    pub fn listener() -> Self {
        Self {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
            ip: [192, 0, 3, 11],
            ..Self::default()
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

    /// Builder: set application task priority (normalized 0–31).
    pub fn with_app_priority(mut self, p: u8) -> Self {
        self.app_priority = p;
        self
    }

    /// Builder: set application task stack size in bytes.
    pub fn with_app_stack_bytes(mut self, s: u32) -> Self {
        self.app_stack_bytes = s;
        self
    }

    /// Builder: set network poll interval in milliseconds.
    pub fn with_poll_interval_ms(mut self, ms: u32) -> Self {
        self.poll_interval_ms = ms;
        self
    }

    /// Map a normalized 0–31 priority to FreeRTOS priority.
    ///
    /// FreeRTOS uses 0 (idle) to `configMAX_PRIORITIES - 1` (highest).
    /// The MPS2-AN385 FreeRTOSConfig.h sets `configMAX_PRIORITIES = 8`,
    /// so the output range is 0–7.
    ///
    /// Mapping: `freertos_pri = normalized * 7 / 31` (linear).
    pub fn to_freertos_priority(normalized: u8) -> u32 {
        let n = if normalized > 31 { 31 } else { normalized };
        (n as u32 * 7) / 31
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
    /// mac = "02:00:00:00:00:00"
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
                    ("network", "mac") => {
                        if let Some(mac) = parse_mac(value) {
                            config.mac = mac;
                        }
                    }
                    ("network", "gateway") => {
                        if let Some(gw) = parse_ipv4(value) {
                            config.gateway = gw;
                        }
                    }
                    ("network", "netmask") => {
                        if let Some(nm) = parse_ipv4(value) {
                            config.netmask = nm;
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
                    ("scheduling", "app_priority") => {
                        if let Some(v) = parse_u32(value) {
                            config.app_priority = v.min(31) as u8;
                        }
                    }
                    ("scheduling", "app_stack_bytes") => {
                        if let Some(v) = parse_u32(value) {
                            config.app_stack_bytes = v;
                        }
                    }
                    ("scheduling", "zenoh_read_priority") => {
                        if let Some(v) = parse_u32(value) {
                            config.zenoh_read_priority = v.min(31) as u8;
                        }
                    }
                    ("scheduling", "zenoh_read_stack_bytes") => {
                        if let Some(v) = parse_u32(value) {
                            config.zenoh_read_stack_bytes = v;
                        }
                    }
                    ("scheduling", "zenoh_lease_priority") => {
                        if let Some(v) = parse_u32(value) {
                            config.zenoh_lease_priority = v.min(31) as u8;
                        }
                    }
                    ("scheduling", "zenoh_lease_stack_bytes") => {
                        if let Some(v) = parse_u32(value) {
                            config.zenoh_lease_stack_bytes = v;
                        }
                    }
                    ("scheduling", "poll_priority") => {
                        if let Some(v) = parse_u32(value) {
                            config.poll_priority = v.min(31) as u8;
                        }
                    }
                    ("scheduling", "poll_interval_ms") => {
                        if let Some(v) = parse_u32(value) {
                            config.poll_interval_ms = v;
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
