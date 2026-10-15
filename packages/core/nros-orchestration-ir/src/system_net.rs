//! RFC-0103 D10 (phase-497 W1) — the network slice of a `system.toml`:
//! `[network.<name>]` (the project's networks) and `[image.<id>.net.<if>]`
//! (an image's address on one of them), parsed ONE way.
//!
//! D10 gives a network fact three owners: the board states its INTERFACES
//! (`[board.net]` in `nros-board.toml`), the project states its NETWORKS, and
//! the image states its ADDRESS on each. This module owns the project's and
//! the image's halves, typed, and lowers an image's address to the
//! `ip` / `gateway` / `netmask` triple the existing carriers already take
//! ([`Lowered`]), so W1 moves where the fact is AUTHORED without moving what a
//! board compiles.
//!
//! What W1 lowers and what it refuses:
//!
//! * a single interface with a static `address` or `host` on a named network
//!   is lowered;
//! * `network` is REQUIRED next to an address. The board's default network
//!   (`[board.net] network`) is a board fact, and the `nros::main!` reader has
//!   no board catalog, so a default taken here would resolve differently on
//!   the macro road than on the CLI road. W2's one resolver, which every
//!   carrier reads, is where that default lands;
//! * a leased address (`address = "dhcp"`), a stated `mac`, a network's
//!   `router`, a bridge/TAP `attach` and more than one interface are PARSED and
//!   validated, then refused naming the wave that renders them (W2, W3, W4): no
//!   carrier takes them yet, and a value accepted and then dropped is the
//!   silent failure D10 exists to remove.
//!
//! The flat single-interface spellings this replaces (`[image.<id>] ip` /
//! `gateway` / `netmask`) are refused by [`refuse_flat_image_net`], naming
//! the new home (D10g).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The built-in network every QEMU board defaults onto: QEMU's own user-mode
/// (SLIRP) network as `-nic user` configures it.
pub const QEMU_NETWORK: &str = "qemu";

/// The keys D10g retired from `[image.<id>]` / `[image_defaults]`.
pub const RETIRED_FLAT_KEYS: &[&str] = &["ip", "gateway", "netmask"];

/// `[network.<name>]` (D10a): what a network IS, and how an emulated image
/// attaches to it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkDef {
    /// The subnet, CIDR (`192.168.50.0/24`). Required for a project network.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subnet: Option<String>,
    /// The subnet's default route, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,
    /// The locator an image on this network uses by default (D10d, W2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router: Option<String>,
    /// The network hands out DHCP leases.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dhcp: bool,
    /// How an EMULATED image joins it (W3). Absent ⇒ physical boards only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attach: Option<Attach>,
}

/// `attach = "slirp"`, `{ bridge = "<host dev>" }` or `{ tap = "<host dev>" }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Attach {
    /// The only bare word: `"slirp"`.
    Mode(String),
    /// `{ bridge = "br0" }`.
    Bridge {
        /// Host bridge device.
        bridge: String,
    },
    /// `{ tap = "tap0" }`.
    Tap {
        /// Host TAP device.
        tap: String,
    },
}

/// `[image.<id>.net.<if>]` (D10b): an image's address on one interface.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageNetIf {
    /// The network this interface sits on: `qemu` or a `[network.<name>]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    /// A full IPv4 address, or `"dhcp"` for a lease.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// The last octet within the network's subnet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<u8>,
    /// The interface's MAC (`02:00:00:00:00:01`). Derived when absent (W2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
}

/// An image's address, lowered to what today's carriers take.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lowered {
    /// The interface the address is on.
    pub interface: Option<String>,
    /// Dotted IPv4.
    pub ip: Option<String>,
    /// Dotted IPv4.
    pub gateway: Option<String>,
    /// Dotted IPv4 mask.
    pub netmask: Option<String>,
}

/// The built-in network named `name`, if it is one.
#[must_use]
pub fn builtin(name: &str) -> Option<NetworkDef> {
    (name == QEMU_NETWORK).then(|| NetworkDef {
        subnet: Some("10.0.2.0/24".to_string()),
        gateway: Some("10.0.2.2".to_string()),
        router: None,
        dhcp: false,
        attach: Some(Attach::Mode("slirp".to_string())),
    })
}

/// Parse dotted IPv4.
pub fn parse_ipv4(s: &str) -> Result<[u8; 4], String> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return Err(format!("{s:?} is not a dotted IPv4 address"));
    }
    let mut out = [0u8; 4];
    for (i, p) in parts.iter().enumerate() {
        out[i] = p
            .parse::<u8>()
            .map_err(|_| format!("{s:?} is not a dotted IPv4 address"))?;
    }
    Ok(out)
}

fn dotted(o: [u8; 4]) -> String {
    format!("{}.{}.{}.{}", o[0], o[1], o[2], o[3])
}

/// A parsed `a.b.c.d/len`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Subnet {
    /// The network address (host bits cleared).
    pub base: [u8; 4],
    /// The prefix length.
    pub prefix: u8,
}

impl Subnet {
    /// Parse CIDR; host bits must be clear.
    pub fn parse(s: &str) -> Result<Self, String> {
        let (a, l) = s
            .split_once('/')
            .ok_or_else(|| format!("subnet {s:?} must be CIDR, e.g. \"192.168.50.0/24\""))?;
        let base = parse_ipv4(a).map_err(|e| format!("subnet {s:?}: {e}"))?;
        let prefix: u8 = l
            .parse()
            .ok()
            .filter(|p| (8..=30).contains(p))
            .ok_or_else(|| format!("subnet {s:?}: prefix must be 8..=30"))?;
        let sub = Self { base, prefix };
        if sub.mask_u32() & u32::from_be_bytes(base) != u32::from_be_bytes(base) {
            return Err(format!(
                "subnet {s:?}: host bits are set; write the network address"
            ));
        }
        Ok(sub)
    }
    fn mask_u32(self) -> u32 {
        u32::MAX << (32 - u32::from(self.prefix))
    }
    /// The netmask, dotted.
    #[must_use]
    pub fn netmask(self) -> [u8; 4] {
        self.mask_u32().to_be_bytes()
    }
    /// Whether `ip` is a usable host address in this subnet (not the network
    /// or broadcast address).
    #[must_use]
    pub fn contains_host(self, ip: [u8; 4]) -> bool {
        let m = self.mask_u32();
        let v = u32::from_be_bytes(ip);
        let b = u32::from_be_bytes(self.base);
        v & m == b && v != b && v != (b | !m)
    }
}

impl NetworkDef {
    /// Check a network's own facts. `name` is for messages.
    pub fn validate(&self, name: &str) -> Result<(), String> {
        let at = format!("[network.{name}]");
        let Some(subnet) = &self.subnet else {
            return Err(format!(
                "{at}: `subnet` is required (CIDR, e.g. \"192.168.50.0/24\")"
            ));
        };
        let sub = Subnet::parse(subnet).map_err(|e| format!("{at}: {e}"))?;
        if let Some(gw) = &self.gateway {
            let g = parse_ipv4(gw).map_err(|e| format!("{at} gateway: {e}"))?;
            if !sub.contains_host(g) {
                return Err(format!(
                    "{at}: gateway {gw} is not a host address in {subnet}"
                ));
            }
        }
        match &self.attach {
            None => {}
            Some(Attach::Mode(m)) if m == "slirp" => {}
            // Parsed, validated, and refused: the QEMU launchers still pick
            // their NIC per board until phase-497 W3, so a bridge or TAP named
            // here would be ignored at launch — the silent no-op D10 removes.
            Some(Attach::Bridge { .. } | Attach::Tap { .. }) => {
                return Err(format!(
                    "{at}: a bridge or TAP `attach` is valid, but the QEMU launchers do not \
                     read it yet — phase-497 W3 does; omit `attach` (a physical-only network) \
                     or use \"slirp\" for now"
                ));
            }
            Some(Attach::Mode(m)) => {
                return Err(format!(
                    "{at}: attach = {m:?} — the bare value is \"slirp\"; a bridge or TAP names \
                     its host device: `attach = {{ bridge = \"br0\" }}` / `{{ tap = \"tap0\" }}`"
                ));
            }
        }
        Ok(())
    }
}

/// `[network.*]` of a `system.toml` document, typed and validated. Redefining
/// the built-in `qemu` is refused: it is QEMU's own plan, not a project's.
pub fn parse_networks(doc: &toml::Table) -> Result<BTreeMap<String, NetworkDef>, String> {
    let Some(v) = doc.get("network") else {
        return Ok(BTreeMap::new());
    };
    let nets: BTreeMap<String, NetworkDef> = v
        .clone()
        .try_into()
        .map_err(|e| format!("[network.*]: {e}"))?;
    for (name, def) in &nets {
        if builtin(name).is_some() {
            return Err(format!(
                "[network.{name}]: `{name}` is built in (QEMU's own SLIRP network, \
                 10.0.2.0/24) and cannot be redefined; name your network something else"
            ));
        }
        def.validate(name)?;
    }
    Ok(nets)
}

/// `[image.<id>.net]` (or `[image_defaults.net]`) — `table` is the image
/// table — typed.
pub fn parse_image_net(
    image: Option<&toml::Table>,
    at: &str,
) -> Result<BTreeMap<String, ImageNetIf>, String> {
    let Some(v) = image.and_then(|t| t.get("net")) else {
        return Ok(BTreeMap::new());
    };
    v.clone()
        .try_into()
        .map_err(|e| format!("{at}.net: {e} (each interface is `[{at}.net.<if>]`)"))
}

/// D10g — refuse a flat `ip` / `gateway` / `netmask` on `table`, naming the
/// new home.
pub fn refuse_flat_image_net(table: Option<&toml::Table>, at: &str) -> Result<(), String> {
    let Some(t) = table else {
        return Ok(());
    };
    if let Some(k) = RETIRED_FLAT_KEYS.iter().find(|k| t.contains_key(**k)) {
        return Err(flat_key_refusal(at, k));
    }
    Ok(())
}

/// The D10g refusal text for a flat `key` on table `at`.
#[must_use]
pub fn flat_key_refusal(at: &str, key: &str) -> String {
    format!(
        "`{at}` states `{key}`, which is retired (RFC-0103 D10g, phase-497 W1): an image's \
         address is per interface and per network now. Write\n\n    [{at}.net.eth0]\n    \
         network = \"qemu\"          # or a [network.<name>] you define\n    host = 50                   \
         # or: address = \"10.0.2.50\"\n\nand drop `ip` / `gateway` / `netmask` — the gateway and \
         netmask are the network's (`[network.<name>] subnet` / `gateway`)."
    )
}

/// Overlay `image` interfaces on `defaults` (D5.1's image-over-defaults rule):
/// an interface the image states replaces the defaults' one whole.
#[must_use]
pub fn overlay(
    defaults: &BTreeMap<String, ImageNetIf>,
    image: &BTreeMap<String, ImageNetIf>,
) -> BTreeMap<String, ImageNetIf> {
    let mut out = defaults.clone();
    out.extend(image.iter().map(|(k, v)| (k.clone(), v.clone())));
    out
}

/// Lower an image's interfaces to the triple today's carriers take. `id` is
/// the image id, for messages.
pub fn lower(
    id: &str,
    networks: &BTreeMap<String, NetworkDef>,
    ifs: &BTreeMap<String, ImageNetIf>,
) -> Result<Lowered, String> {
    let at = format!("[image.{id}.net]");
    let mut it = ifs.iter();
    let Some((ifname, i)) = it.next() else {
        return Ok(Lowered::default());
    };
    if it.next().is_some() {
        return Err(format!(
            "{at}: {} interfaces ({}) — an image with more than one interface is phase-497 W4 \
             (multi-NIC); state one",
            ifs.len(),
            ifs.keys().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    let at = format!("[image.{id}.net.{ifname}]");
    if i.mac.is_some() {
        return Err(format!(
            "{at}: `mac` is parsed but no carrier takes a per-image MAC yet — phase-497 W2 \
             renders it (and derives one when it is absent); drop it for now"
        ));
    }
    let addressed = i.address.is_some() || i.host.is_some();
    let Some(net_name) = &i.network else {
        if addressed {
            return Err(format!(
                "{at}: an address needs its network — add `network = \"qemu\"` (QEMU's SLIRP \
                 network) or the name of a `[network.<name>]` you define"
            ));
        }
        return Ok(Lowered {
            interface: Some(ifname.clone()),
            ..Lowered::default()
        });
    };
    let net = match builtin(net_name) {
        Some(b) => b,
        None => networks.get(net_name).cloned().ok_or_else(|| {
            format!(
                "{at}: network = {net_name:?} names no `[network.{net_name}]` and is not the \
                 built-in \"{QEMU_NETWORK}\""
            )
        })?,
    };
    if net.router.is_some() {
        return Err(format!(
            "[network.{net_name}]: `router` is parsed but its default is rendered by \
             phase-497 W2 (RFC-0103 D10d); state the image's `locator` for now"
        ));
    }
    let subnet = Subnet::parse(net.subnet.as_deref().unwrap_or_default())
        .map_err(|e| format!("[network.{net_name}]: {e}"))?;
    let ip = match (&i.address, i.host) {
        (Some(_), Some(_)) => {
            return Err(format!("{at}: state `address` or `host`, not both"));
        }
        (Some(a), None) if a == "dhcp" => {
            if !net.dhcp {
                return Err(format!(
                    "{at}: address = \"dhcp\" on network {net_name:?}, which states no \
                     `dhcp = true`"
                ));
            }
            return Err(format!(
                "{at}: address = \"dhcp\" is valid here, but no carrier takes a leased \
                 address yet — phase-497 W2 renders it; use a static `host` / `address` for now"
            ));
        }
        (Some(a), None) => parse_ipv4(a).map_err(|e| format!("{at} address: {e}"))?,
        (None, Some(h)) => {
            let mut o = subnet.base;
            let host_bits = 32 - u32::from(subnet.prefix);
            if host_bits < 8 {
                return Err(format!(
                    "{at}: `host` is the last octet, and network {net_name:?} ({}) is smaller \
                     than a /24 — write the full `address`",
                    net.subnet.as_deref().unwrap_or_default()
                ));
            }
            o[3] = h;
            o
        }
        (None, None) => {
            return Ok(Lowered {
                interface: Some(ifname.clone()),
                ..Lowered::default()
            });
        }
    };
    if !subnet.contains_host(ip) {
        return Err(format!(
            "{at}: {} is not a host address in network {net_name:?} ({})",
            dotted(ip),
            net.subnet.as_deref().unwrap_or_default()
        ));
    }
    if let Some(gw) = &net.gateway
        && parse_ipv4(gw).ok() == Some(ip)
    {
        return Err(format!(
            "{at}: {} is network {net_name:?}'s gateway",
            dotted(ip)
        ));
    }
    Ok(Lowered {
        interface: Some(ifname.clone()),
        ip: Some(dotted(ip)),
        gateway: net.gateway.clone(),
        netmask: Some(dotted(subnet.netmask())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(s: &str) -> toml::Table {
        s.parse().expect("toml")
    }

    fn image_net(d: &toml::Table, id: &str) -> BTreeMap<String, ImageNetIf> {
        let img = d
            .get("image")
            .and_then(|i| i.get(id))
            .and_then(|i| i.as_table());
        parse_image_net(img, &format!("image.{id}")).expect("image net")
    }

    #[test]
    fn host_on_the_builtin_qemu_network_lowers_to_the_slirp_plan() {
        let d = doc("[image.a.net.eth0]\nnetwork = \"qemu\"\nhost = 50\n");
        let l = lower("a", &parse_networks(&d).unwrap(), &image_net(&d, "a")).unwrap();
        assert_eq!(l.interface.as_deref(), Some("eth0"));
        assert_eq!(l.ip.as_deref(), Some("10.0.2.50"));
        assert_eq!(l.gateway.as_deref(), Some("10.0.2.2"));
        assert_eq!(l.netmask.as_deref(), Some("255.255.255.0"));
    }

    #[test]
    fn an_explicit_physical_network_lowers_and_needs_no_attach() {
        let d = doc(
            "[network.lab]\nsubnet = \"192.168.50.0/24\"\ngateway = \"192.168.50.1\"\n\n\
             [image.a.net.eth0]\nnetwork = \"lab\"\naddress = \"192.168.50.20\"\n",
        );
        let nets = parse_networks(&d).unwrap();
        assert!(nets["lab"].attach.is_none());
        let l = lower("a", &nets, &image_net(&d, "a")).unwrap();
        assert_eq!(l.ip.as_deref(), Some("192.168.50.20"));
        assert_eq!(l.gateway.as_deref(), Some("192.168.50.1"));
    }

    #[test]
    fn an_image_that_states_nothing_keeps_the_board_default() {
        let d = doc("[image.a]\nboard = \"x\"\n");
        let l = lower("a", &parse_networks(&d).unwrap(), &image_net(&d, "a")).unwrap();
        assert_eq!(l, Lowered::default());
    }

    #[test]
    fn refusals_name_what_to_write() {
        let cases = [
            ("[image.a.net.eth0]\nhost = 50\n", "needs its network"),
            (
                "[image.a.net.eth0]\nnetwork = \"lab\"\nhost = 5\n",
                "names no `[network.lab]`",
            ),
            (
                "[image.a.net.eth0]\nnetwork = \"qemu\"\nhost = 2\n",
                "gateway",
            ),
            (
                "[image.a.net.eth0]\nnetwork = \"qemu\"\naddress = \"10.0.3.5\"\n",
                "not a host address",
            ),
            (
                "[image.a.net.eth0]\nnetwork = \"qemu\"\nhost = 5\naddress = \"10.0.2.5\"\n",
                "not both",
            ),
            (
                "[image.a.net.eth0]\nnetwork = \"qemu\"\naddress = \"dhcp\"\n",
                "states no `dhcp = true`",
            ),
            (
                "[image.a.net.eth0]\nnetwork = \"qemu\"\nhost = 5\nmac = \"02:00:00:00:00:01\"\n",
                "W2",
            ),
            (
                "[image.a.net.eth0]\nnetwork = \"qemu\"\nhost = 5\n[image.a.net.eth1]\nnetwork = \"qemu\"\nhost = 6\n",
                "W4",
            ),
        ];
        for (src, want) in cases {
            let d = doc(src);
            let e = lower("a", &parse_networks(&d).unwrap(), &image_net(&d, "a")).expect_err(src);
            assert!(e.contains(want), "{src}\n-> {e}");
        }
    }

    #[test]
    fn a_leased_address_is_validated_then_deferred_to_w2() {
        let d = doc(
            "[network.lab]\nsubnet = \"192.168.50.0/24\"\ndhcp = true\n\n\
             [image.a.net.eth0]\nnetwork = \"lab\"\naddress = \"dhcp\"\n",
        );
        let e = lower("a", &parse_networks(&d).unwrap(), &image_net(&d, "a")).unwrap_err();
        assert!(e.contains("W2") && e.contains("valid here"), "{e}");
    }

    #[test]
    fn network_tables_are_validated() {
        for (src, want) in [
            ("[network.qemu]\nsubnet = \"10.0.2.0/24\"\n", "built in"),
            (
                "[network.lab]\ngateway = \"192.168.1.1\"\n",
                "`subnet` is required",
            ),
            ("[network.lab]\nsubnet = \"192.168.1.5/24\"\n", "host bits"),
            (
                "[network.lab]\nsubnet = \"192.168.1.0/24\"\ngateway = \"10.0.0.1\"\n",
                "not a host address",
            ),
            (
                "[network.lab]\nsubnet = \"192.168.1.0/24\"\nattach = \"tap\"\n",
                "bare value",
            ),
            (
                "[network.lab]\nsubnet = \"192.168.1.0/24\"\nmode = \"bridge\"\n",
                "unknown field",
            ),
        ] {
            let e = parse_networks(&doc(src)).expect_err(src);
            assert!(e.contains(want), "{src}\n-> {e}");
        }
        let e = parse_networks(&doc(
            "[network.br]\nsubnet = \"192.168.1.0/24\"\nattach = { bridge = \"br0\" }\n",
        ))
        .unwrap_err();
        assert!(e.contains("W3"), "{e}");
        let ok = parse_networks(&doc(
            "[network.lan]\nsubnet = \"192.168.1.0/24\"\nattach = \"slirp\"\n",
        ))
        .unwrap();
        assert_eq!(ok["lan"].attach, Some(Attach::Mode("slirp".into())));
    }

    #[test]
    fn the_flat_spellings_are_refused_naming_the_new_home() {
        let d = doc("[image.a]\nboard = \"x\"\nip = \"10.0.2.50\"\n");
        let img = d
            .get("image")
            .and_then(|i| i.get("a"))
            .and_then(|i| i.as_table());
        let e = refuse_flat_image_net(img, "image.a").unwrap_err();
        assert!(
            e.contains("[image.a.net.eth0]") && e.contains("D10g"),
            "{e}"
        );
    }

    #[test]
    fn image_interfaces_replace_the_defaults_per_interface() {
        let d = doc(
            "[image_defaults.net.eth0]\nnetwork = \"qemu\"\nhost = 10\n\n\
             [image.a.net.eth0]\nnetwork = \"qemu\"\nhost = 11\n",
        );
        let base = parse_image_net(
            d.get("image_defaults").and_then(|t| t.as_table()),
            "image_defaults",
        )
        .unwrap();
        let merged = overlay(&base, &image_net(&d, "a"));
        assert_eq!(merged["eth0"].host, Some(11));
    }
}
