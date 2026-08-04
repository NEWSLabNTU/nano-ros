//! The network/node configuration core every board crate re-declares.
//!
//! phase-322 W1.g counted **12 hand-rolled `Config` structs** across
//! `packages/boards/`, at least nine carrying an identical
//! `{mac, ip, netmask, gateway, locator, domain_id}` core, and the
//! `DeployOverlay` → `Config` merge written out at least four separate times.
//! Each copy drifted on its own: `nros-board-rtic-mps2-an385` and
//! `nros-board-mps2-an385` disagreed on the default IP (`10.0.2.10` vs
//! `192.0.3.10`), which is invisible until a node fails to reach the router at
//! runtime. (phase-337 W6.a folded those two crates into one and turned the
//! second default into the NAMED `Config::qemu_slirp()` preset — the two values
//! are two QEMU launch modes, so the defect was that one of them lived in a
//! sibling crate as a second `Default`-shaped function.)
//!
//! This type is **additive** (phase-337 W1.b): nothing adopts it yet. Each
//! board wave migrates its own `Config` onto it as that wave's first step, so
//! the blast radius is one board rather than twelve. A board keeps its
//! board-specific fields — `uart_base`/`baudrate` on MPS2, `interface` on
//! ThreadX Linux — and composes:
//!
//! ```ignore
//! pub struct Config {
//!     pub base: BaseConfig,
//!     pub interface: &'static str,
//! }
//! ```
//!
//! # Netmask, not prefix
//!
//! The 12 structs are split on how they spell the same fact: half store
//! `prefix: u8` (NuttX, MPS2), half `netmask: [u8; 4]` (ThreadX, FreeRTOS).
//! `BaseConfig` stores the **netmask**, because that is what
//! [`nros_platform::DeployOverlay`] carries — a config that stored the
//! prefix would have to convert on every overlay merge, which is precisely the
//! per-board code this type exists to delete. `prefix()` and
//! [`BaseConfig::with_prefix`] cover the boards that think in CIDR.

/// The configuration core shared by every networked board.
///
/// Board-specific settings do NOT belong here — compose instead of extending,
/// or this becomes the union of twelve boards and every board carries ten
/// fields it ignores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseConfig {
    /// MAC address. Boards with no Ethernet MAC (NuttX under QEMU takes its
    /// from the host TAP device) leave it at the default and ignore it.
    pub mac: [u8; 6],
    /// Static IPv4 address.
    pub ip: [u8; 4],
    /// Subnet mask. See the module note on netmask-vs-prefix.
    pub netmask: [u8; 4],
    /// Default route.
    pub gateway: [u8; 4],
    /// RMW endpoint the firmware dials, e.g. `"tcp/192.0.3.1:7447"`.
    pub zenoh_locator: &'static str,
    /// ROS 2 domain ID.
    pub domain_id: u32,
}

impl Default for BaseConfig {
    /// The QEMU-bridge scheme (`192.0.3.0/24` on `br-qemu`) the emulated
    /// boards share. A board on a different network overrides in its own
    /// `Default` — that is a board fact, not a drifted copy.
    fn default() -> Self {
        Self {
            // Locally administered, unicast.
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
            ip: [192, 0, 3, 10],
            netmask: [255, 255, 255, 0],
            gateway: [192, 0, 3, 1],
            zenoh_locator: "tcp/192.0.3.1:7447",
            domain_id: 0,
        }
    }
}

impl BaseConfig {
    /// The netmask as a CIDR prefix length. See [`prefix_from_netmask`].
    pub const fn prefix(&self) -> u8 {
        prefix_from_netmask(self.netmask)
    }

    /// Set the netmask from a CIDR prefix length. `prefix > 32` saturates to
    /// `/32`: a host route is the safe reading of a nonsense value, since it
    /// fails to reach the network loudly instead of silently widening it.
    pub const fn with_prefix(mut self, prefix: u8) -> Self {
        self.netmask = netmask_from_prefix(prefix);
        self
    }

    /// Apply a deploy overlay, field by field: `Some` replaces, `None` keeps
    /// the board default.
    ///
    /// This merge existed in at least four board crates, each spelled slightly
    /// differently. `transport` and the node name are NOT applied here — they
    /// are consumed by `BoardEntry`, not by the network config.
    #[cfg(feature = "deploy-overlay")]
    pub fn apply_overlay(&mut self, overlay: &nros_platform::DeployOverlay) {
        if let Some(locator) = overlay.locator {
            self.zenoh_locator = locator;
        }
        if let Some(ip) = overlay.ip {
            self.ip = ip;
        }
        if let Some(gateway) = overlay.gateway {
            self.gateway = gateway;
        }
        if let Some(netmask) = overlay.netmask {
            self.netmask = netmask;
        }
        if let Some(domain_id) = overlay.domain_id {
            self.domain_id = domain_id;
        }
    }
}

/// A dotted-quad netmask as a CIDR prefix length.
///
/// Counts leading ones and stops at the first zero, so a discontiguous mask
/// reports the length of its leading run rather than a popcount —
/// `255.0.255.0` is a misconfiguration, and `/8` says so more usefully than
/// `/16` would.
///
/// Free function as well as [`BaseConfig::prefix`] because the boards that
/// think in CIDR need the conversion where they have a bare `[u8; 4]` and no
/// `BaseConfig` — `nros-board-mps2-an385`'s deploy-overlay merge, for one.
/// Both MPS2 crates had grown their own popcount `mask_to_prefix` (phase-337
/// W6); this is the one spelling they now share.
pub const fn prefix_from_netmask(netmask: [u8; 4]) -> u8 {
    let bits = u32::from_be_bytes(netmask);
    let mut n = 0u8;
    while n < 32 {
        if bits & (0x8000_0000 >> n) == 0 {
            break;
        }
        n += 1;
    }
    n
}

// ── nros.toml direct-mode parsing ───────────────────────────────────────
//
// phase-337 W4.b/W4.c — every networked board's `Config::from_toml` was the
// same ~90-line line-scanner plus the same three `no_std` parsers, copied. The
// two ThreadX boards' copies were byte-identical apart from ONE arm
// (`("transport", "interface")`, which only threadx-linux has). Splitting the
// scan (here) from the field dispatch (also here, for the shared fields) lets
// a board add its own arms without re-deriving the scanner.

/// Call `f(section, key, value)` for every `key = value` line of a
/// direct-mode `nros.toml`, with `section` the enclosing `[table]` or
/// `[[array-of-tables]]` name (`""` before the first header).
///
/// Quotes are stripped from `value`; comments and blank lines are skipped.
/// Deliberately not a TOML parser: this runs in `no_std` firmware against a
/// string the build baked in with `include_str!`, and the accepted subset is
/// the one nano-ros emits.
pub fn for_each_toml_field(
    toml: &'static str,
    mut f: impl FnMut(&'static str, &'static str, &'static str),
) {
    let mut section: &'static str = "";
    for line in toml.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // `[[transport]]` array-of-tables + plain `[node]` sections.
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
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim();
        let value = line[eq + 1..].trim();
        let value = if (value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\''))
        {
            &value[1..value.len() - 1]
        } else {
            value
        };
        f(section, key, value);
    }
}

/// Parse an IPv4 address (`"192.0.3.10"`).
pub fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
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

/// Parse a MAC address (`"02:00:00:00:00:00"`).
pub fn parse_mac(s: &str) -> Option<[u8; 6]> {
    let mut result = [0u8; 6];
    let mut byte_idx = 0;

    for part in s.split(':') {
        if byte_idx >= 6 || part.len() != 2 {
            return None;
        }
        let bytes = part.as_bytes();
        let hi = hex_digit(bytes[0])?;
        let lo = hex_digit(bytes[1])?;
        result[byte_idx] = hi * 16 + lo;
        byte_idx += 1;
    }

    if byte_idx == 6 { Some(result) } else { None }
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Parse a decimal integer. `None` on overflow or any non-digit.
pub fn parse_u32(s: &str) -> Option<u32> {
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

impl BaseConfig {
    /// Apply one direct-mode `nros.toml` field, returning whether it was
    /// consumed.
    ///
    /// `false` means "not one of mine" — the board then tries its own arms
    /// (`interface`, `uart_base`, …). Unrecognised fields are silently ignored
    /// by the caller, matching the pre-phase-337 behaviour: a firmware image
    /// has nowhere to report a warning to.
    pub fn apply_toml_field(&mut self, section: &str, key: &str, value: &'static str) -> bool {
        match (section, key) {
            ("transport", "ip") => {
                let (addr, pfx) = match value.split_once('/') {
                    Some(p) => p,
                    None => (value, ""),
                };
                if let Some(ip) = parse_ipv4(addr) {
                    self.ip = ip;
                }
                if let Some(p) = parse_u32(pfx) {
                    self.netmask = netmask_from_prefix(p as u8);
                }
                true
            }
            ("transport", "mac") => {
                if let Some(mac) = parse_mac(value) {
                    self.mac = mac;
                }
                true
            }
            ("transport", "gateway") => {
                if let Some(gw) = parse_ipv4(value) {
                    self.gateway = gw;
                }
                true
            }
            ("transport", "locator") => {
                self.zenoh_locator = value;
                true
            }
            ("node", "domain_id") => {
                if let Some(d) = parse_u32(value) {
                    self.domain_id = d;
                }
                true
            }
            _ => false,
        }
    }
}

/// A CIDR prefix length as a dotted-quad netmask. Saturates at `/32`.
pub const fn netmask_from_prefix(prefix: u8) -> [u8; 4] {
    let p = if prefix > 32 { 32 } else { prefix };
    // `u32 << 32` is UB-adjacent in Rust (it panics in debug, wraps in
    // release), so /0 is spelled out rather than shifted.
    let bits: u32 = if p == 0 { 0 } else { u32::MAX << (32 - p) };
    bits.to_be_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_round_trips_through_netmask() {
        for p in 0..=32u8 {
            let cfg = BaseConfig::default().with_prefix(p);
            assert_eq!(cfg.prefix(), p, "prefix {p} did not round-trip");
        }
    }

    #[test]
    fn common_prefixes_have_the_expected_dotted_quad() {
        assert_eq!(netmask_from_prefix(24), [255, 255, 255, 0]);
        assert_eq!(netmask_from_prefix(16), [255, 255, 0, 0]);
        assert_eq!(netmask_from_prefix(8), [255, 0, 0, 0]);
        assert_eq!(netmask_from_prefix(0), [0, 0, 0, 0]);
        assert_eq!(netmask_from_prefix(32), [255, 255, 255, 255]);
    }

    #[test]
    fn an_out_of_range_prefix_saturates_to_a_host_route() {
        // Not a silent wrap to /0, which would widen the network instead of
        // narrowing it.
        assert_eq!(netmask_from_prefix(33), [255, 255, 255, 255]);
        assert_eq!(netmask_from_prefix(255), [255, 255, 255, 255]);
    }

    #[test]
    fn a_discontiguous_mask_reports_its_leading_run() {
        let cfg = BaseConfig {
            netmask: [255, 0, 255, 0],
            ..BaseConfig::default()
        };
        assert_eq!(cfg.prefix(), 8);
    }

    // phase-337 W4 — the scanner + field dispatch these tests cover replaced
    // one hand-copied ~90-line parser per board. They assert the behaviour the
    // ThreadX copies had, so "behaviour-neutral" is checked rather than
    // asserted.

    const SAMPLE: &str = "\
# a comment
[[transport]]
ip = \"10.0.2.40/16\"
mac = \"52:54:00:12:34:57\"
gateway = \"10.0.2.2\"
locator = \"tcp/10.0.2.2:7553\"
interface = \"veth-tx1\"

[node]
domain_id = 7
";

    #[test]
    fn a_direct_mode_toml_lands_on_the_shared_core() {
        let mut cfg = BaseConfig::default();
        for_each_toml_field(SAMPLE, |s, k, v| {
            cfg.apply_toml_field(s, k, v);
        });
        assert_eq!(cfg.ip, [10, 0, 2, 40]);
        assert_eq!(cfg.prefix(), 16, "the /nn suffix sets the netmask");
        assert_eq!(cfg.mac, [0x52, 0x54, 0x00, 0x12, 0x34, 0x57]);
        assert_eq!(cfg.gateway, [10, 0, 2, 2]);
        assert_eq!(cfg.zenoh_locator, "tcp/10.0.2.2:7553");
        assert_eq!(cfg.domain_id, 7);
    }

    #[test]
    fn a_board_specific_key_is_left_for_the_board() {
        // `interface` is threadx-linux's, not the shared core's: the dispatch
        // must decline it so the board's own arm can claim it.
        let mut cfg = BaseConfig::default();
        let mut seen_interface = None;
        for_each_toml_field(SAMPLE, |s, k, v| {
            if !cfg.apply_toml_field(s, k, v) && (s, k) == ("transport", "interface") {
                seen_interface = Some(v);
            }
        });
        assert_eq!(seen_interface, Some("veth-tx1"));
    }

    #[test]
    fn a_field_outside_a_known_section_is_ignored_not_misapplied() {
        // The scanner yields section-qualified keys, so `[other] ip = …` must
        // not overwrite the transport IP.
        let mut cfg = BaseConfig::default();
        let before = cfg.ip;
        for_each_toml_field("[other]\nip = \"1.2.3.4\"\n", |s, k, v| {
            assert!(!cfg.apply_toml_field(s, k, v));
        });
        assert_eq!(cfg.ip, before);
    }

    #[test]
    fn a_malformed_value_keeps_the_board_default() {
        // Firmware has nowhere to report a parse error to, so the pre-existing
        // behaviour is "ignore and keep the default" — not "zero the field".
        let mut cfg = BaseConfig::default();
        let before = cfg.ip;
        assert!(cfg.apply_toml_field("transport", "ip", "not-an-address"));
        assert_eq!(cfg.ip, before);
        assert_eq!(parse_ipv4("1.2.3"), None);
        assert_eq!(parse_ipv4("1.2.3.256"), None);
        assert_eq!(parse_mac("52:54:00:12:34"), None);
        assert_eq!(parse_u32(""), None);
        assert_eq!(parse_u32("4294967296"), None);
    }

    #[test]
    fn the_default_is_the_qemu_bridge_scheme() {
        let cfg = BaseConfig::default();
        assert_eq!(cfg.ip, [192, 0, 3, 10]);
        assert_eq!(cfg.gateway, [192, 0, 3, 1]);
        assert_eq!(cfg.prefix(), 24);
        assert_eq!(cfg.domain_id, 0);
        // Locally administered bit set, multicast bit clear.
        assert_eq!(cfg.mac[0] & 0x03, 0x02);
    }
}
