//! The network/node configuration core every board crate re-declares.
//!
//! phase-322 W1.g counted **12 hand-rolled `Config` structs** across
//! `packages/boards/`, at least nine carrying an identical
//! `{mac, ip, netmask, gateway, locator, domain_id}` core, and the
//! `DeployOverlay` → `Config` merge written out at least four separate times.
//! Each copy drifted on its own: `nros-board-rtic-mps2-an385` and
//! `nros-board-mps2-an385` disagree on the default IP (`10.0.2.10` vs
//! `192.0.3.10`), which is invisible until a node fails to reach the router at
//! runtime.
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
    /// The netmask as a CIDR prefix length.
    ///
    /// Counts leading ones and stops at the first zero, so a discontiguous
    /// mask reports the length of its leading run rather than a popcount —
    /// `255.0.255.0` is a misconfiguration, and `/8` says so more usefully
    /// than `/16` would.
    pub const fn prefix(&self) -> u8 {
        let bits = u32::from_be_bytes(self.netmask);
        let mut n = 0u8;
        while n < 32 {
            if bits & (0x8000_0000 >> n) == 0 {
                break;
            }
            n += 1;
        }
        n
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
