//! RFC-0103 D10 (phase-497) — the network slice of a `system.toml`:
//! `[network.<name>]` (the project's networks) and `[image.<id>.net.<if>]`
//! (an image's address on one of them), parsed ONE way.
//!
//! D10 gives a network fact three owners: the board states its INTERFACES
//! (`[board.net]` in `nros-board.toml`), the project states its NETWORKS, and
//! the image states its ADDRESS on each. This module owns the project's and
//! the image's halves, typed, and checks what can be checked without the
//! board ([`check_image_interfaces`]).
//!
//! The board's half — which interfaces it has, the default network an
//! unnamed interface lands on, the MAC an image does not state, and the
//! duplicate-address refusal across images — needs the board catalog, so it is
//! the CLI's resolver (`nros_cli_core::orchestration::image_net`, W2). Its
//! answer reaches every build as one text (`nros_board_common::image_net`);
//! `nros::main!` bakes that text and does not resolve anything itself.
//!
//! The flat single-interface spellings this replaced (`[image.<id>] ip` /
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
    /// How an EMULATED image joins it (rendered by the QEMU launchers,
    /// phase-497 W3). Absent ⇒ physical boards only.
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

/// An interface's address as the image stated it, checked against its
/// network: static, or a lease. `None` from [`stated_address`] means the image
/// stated neither, and the resolver applies the network's default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatedAddress {
    /// `address = "a.b.c.d"` or `host = N`.
    Static([u8; 4]),
    /// `address = "dhcp"`, on a network with `dhcp = true`.
    Dhcp,
}

/// QEMU's own first guest address on its SLIRP network (`10.0.2.15`), which
/// an image on [`QEMU_NETWORK`] that states no address takes — what a plain
/// `-nic user` guest would pick for itself.
pub const QEMU_FIRST_GUEST: [u8; 4] = [10, 0, 2, 15];

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

/// A bridge or TAP `attach` names a host network device, which the QEMU
/// launcher splices into a `-nic` argument: empty, whitespace or a comma
/// would not survive that.
fn check_host_device(at: &str, d: &str) -> Result<(), String> {
    if d.is_empty() || d.contains(char::is_whitespace) || d.contains(',') {
        return Err(format!(
            "{at}: attach names host device {d:?}, which is not a device name"
        ));
    }
    Ok(())
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
            // phase-497 W3 — the QEMU launchers render these (`-nic bridge` /
            // `-nic tap`, `nros_board_common::image_net::qemu_nic`).
            Some(Attach::Bridge { bridge: d } | Attach::Tap { tap: d }) => {
                check_host_device(&at, d)?;
            }
            Some(Attach::Mode(m)) => {
                return Err(format!(
                    "{at}: attach = {m:?} — the bare value is \"slirp\"; a bridge or TAP names \
                     its host device: `attach = {{ bridge = \"br0\" }}` / `{{ tap = \"tap0\" }}`"
                ));
            }
        }
        // D10d — the locator an image on this network dials by default.
        if let Some(r) = &self.router
            && (r.is_empty() || r.contains(char::is_whitespace) || !r.contains('/'))
        {
            return Err(format!(
                "{at}: router = {r:?} is not a locator — write it the way an image's `locator` \
                 is written, e.g. \"tcp/192.168.50.1:7447\""
            ));
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

/// RFC-0109 D4 (phase-514 W2) — the board config files an image names.
///
/// `os_config` lists an image's layer-1 files. The `.toml` ones are the board
/// package's own network file (RFC-0109 D4); the rest (Zephyr `.conf`,
/// devicetree `.overlay`, a NuttX `defconfig` fragment) belong to the RTOS and
/// are not read here.
#[must_use]
pub fn board_net_files(os_config: &[String]) -> Vec<&str> {
    os_config
        .iter()
        .map(String::as_str)
        .filter(|f| f.ends_with(".toml"))
        .collect()
}

/// `os_config` from an image's raw table (the leaf road).
pub fn parse_os_config(image: Option<&toml::Table>, at: &str) -> Result<Vec<String>, String> {
    let Some(v) = image.and_then(|t| t.get("os_config")) else {
        return Ok(Vec::new());
    };
    v.clone()
        .try_into()
        .map_err(|e| format!("{at}.os_config: {e} (a list of file paths)"))
}

/// Read one board network file: a table per interface, each with the fields
/// of [`ImageNetIf`] (`network`, `host` or `address`, `mac`).
pub fn read_board_net_file(path: &std::path::Path) -> Result<BTreeMap<String, ImageNetIf>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("{}: {e} (named by `os_config`)", path.display()))?;
    toml::from_str(&text).map_err(|e| {
        format!(
            "{}: {e} (a board network file is one `[<interface>]` table per interface, \
             with `network`, `host` or `address`, and `mac` — RFC-0109 D4)",
            path.display()
        )
    })
}

/// An image's interfaces: its inline `net` plus every board network file its
/// `os_config` names, resolved against `dir` (the `system.toml`'s directory).
/// An interface stated in two places is refused, naming both: one owner per
/// fact (RFC-0103), and the file is that owner (RFC-0109 D4).
pub fn fold_board_net_files(
    inline: &BTreeMap<String, ImageNetIf>,
    dir: &std::path::Path,
    os_config: &[String],
    at: &str,
) -> Result<BTreeMap<String, ImageNetIf>, String> {
    let mut out = inline.clone();
    let mut owner: BTreeMap<String, String> = inline
        .keys()
        .map(|k| (k.clone(), format!("[{at}.net.{k}]")))
        .collect();
    for file in board_net_files(os_config) {
        let path = dir.join(file);
        for (name, decl) in read_board_net_file(&path)? {
            if let Some(prev) = owner.get(&name) {
                return Err(format!(
                    "`{at}`: interface `{name}` is stated twice — in {prev} and in {} — so \
                     the image has two owners for one address (RFC-0109 D4: the board \
                     network file named by `os_config` is its one home)",
                    path.display()
                ));
            }
            owner.insert(name.clone(), path.display().to_string());
            out.insert(name, decl);
        }
    }
    Ok(out)
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

/// The network named `name`: the built-in [`QEMU_NETWORK`] or one of the
/// project's. `at` is the table that named it, for the message.
pub fn network_def(
    name: &str,
    networks: &BTreeMap<String, NetworkDef>,
    at: &str,
) -> Result<NetworkDef, String> {
    match builtin(name) {
        Some(b) => Ok(b),
        None => networks.get(name).cloned().ok_or_else(|| {
            format!(
                "{at}: network = {name:?} names no `[network.{name}]` and is not the built-in \
                 \"{QEMU_NETWORK}\""
            )
        }),
    }
}

impl NetworkDef {
    /// The subnet, parsed. Validated networks always have one.
    pub fn subnet(&self, name: &str) -> Result<Subnet, String> {
        Subnet::parse(self.subnet.as_deref().unwrap_or_default())
            .map_err(|e| format!("[network.{name}]: {e}"))
    }

    /// The gateway, parsed.
    pub fn gateway_octets(&self, name: &str) -> Result<Option<[u8; 4]>, String> {
        self.gateway
            .as_deref()
            .map(|g| parse_ipv4(g).map_err(|e| format!("[network.{name}] gateway: {e}")))
            .transpose()
    }

    /// Whether images on this network each get a PRIVATE instance of it, so two
    /// of them never meet and cannot collide: QEMU's SLIRP, which every
    /// `-nic user` creates per VM. A bridge, a TAP or a physical network is
    /// shared, and the resolver refuses a duplicate address or MAC on it.
    #[must_use]
    pub fn is_per_image(&self) -> bool {
        matches!(&self.attach, Some(Attach::Mode(m)) if m == "slirp")
    }
}

/// The address `i` states on network `net_name` (`net`), checked: `address`
/// and `host` are exclusive, a static address must be a host address in the
/// subnet and not its gateway, and a lease needs `dhcp = true`. `Ok(None)`
/// when the image states no address. `at` names the interface table.
pub fn stated_address(
    at: &str,
    net_name: &str,
    net: &NetworkDef,
    i: &ImageNetIf,
) -> Result<Option<StatedAddress>, String> {
    let subnet = net.subnet(net_name)?;
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
            return Ok(Some(StatedAddress::Dhcp));
        }
        (Some(a), None) => parse_ipv4(a).map_err(|e| format!("{at} address: {e}"))?,
        (None, Some(h)) => {
            let mut o = subnet.base;
            if 32 - u32::from(subnet.prefix) < 8 {
                return Err(format!(
                    "{at}: `host` is the last octet, and network {net_name:?} ({}) is smaller \
                     than a /24 — write the full `address`",
                    net.subnet.as_deref().unwrap_or_default()
                ));
            }
            o[3] = h;
            o
        }
        (None, None) => return Ok(None),
    };
    if !subnet.contains_host(ip) {
        return Err(format!(
            "{at}: {} is not a host address in network {net_name:?} ({})",
            dotted(ip),
            net.subnet.as_deref().unwrap_or_default()
        ));
    }
    if net.gateway_octets(net_name)? == Some(ip) {
        return Err(format!(
            "{at}: {} is network {net_name:?}'s gateway",
            dotted(ip)
        ));
    }
    Ok(Some(StatedAddress::Static(ip)))
}

/// Parse a stated `mac` (`02:00:00:00:00:01`).
pub fn parse_mac(at: &str, s: &str) -> Result<[u8; 6], String> {
    let parts: Vec<&str> = s.split(':').collect();
    let bad = || format!("{at}: mac = {s:?} is not six hex bytes (`02:00:00:00:00:01`)");
    if parts.len() != 6 {
        return Err(bad());
    }
    let mut out = [0u8; 6];
    for (o, p) in out.iter_mut().zip(&parts) {
        if p.len() != 2 {
            return Err(bad());
        }
        *o = u8::from_str_radix(p, 16).map_err(|_| bad())?;
    }
    if out[0] & 1 == 1 {
        return Err(format!(
            "{at}: mac = {s:?} is a multicast address (the low bit of the first byte is set); \
             an interface's MAC is unicast"
        ));
    }
    Ok(out)
}

/// Check what an image's interfaces state WITHOUT its board (the
/// `nros::main!` reader has no board catalog): each named network exists, and
/// each stated address and MAC is well formed on it. The board's half — which
/// interfaces exist, the default network, the derived MAC — is the CLI's
/// resolver. `id` is the image id.
pub fn check_image_interfaces(
    id: &str,
    networks: &BTreeMap<String, NetworkDef>,
    ifs: &BTreeMap<String, ImageNetIf>,
) -> Result<(), String> {
    for (ifname, i) in ifs {
        let at = format!("[image.{id}.net.{ifname}]");
        if let Some(m) = &i.mac {
            parse_mac(&at, m)?;
        }
        // No network: the board's default one, which the resolver takes. An
        // address needs to know its subnet, so it cannot be checked here; the
        // resolver does.
        if let Some(n) = &i.network {
            let net = network_def(n, networks, &at)?;
            stated_address(&at, n, &net, i)?;
        }
    }
    Ok(())
}

/// D10d — the locator an image dials by default, from its networks: the
/// `router` of the network its FIRST interface sits on. `None` when that
/// network states none (on [`QEMU_NETWORK`] the board's own default, the
/// gateway, is right; elsewhere the caller refuses an image with no locator).
#[must_use]
pub fn network_router(
    networks: &BTreeMap<String, NetworkDef>,
    ifs: &BTreeMap<String, ImageNetIf>,
) -> Option<String> {
    let (_, first) = ifs.iter().next()?;
    networks.get(first.network.as_deref()?)?.router.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory unique to this process and test.
    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("nros-ir-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// RFC-0109 D4 (phase-514 W2) — a board network file supplies an image's
    /// interfaces; an interface stated inline AND in a file has two owners and
    /// is refused; a non-`.toml` entry belongs to the RTOS and is not read.
    #[test]
    fn os_config_board_net_files_fold_into_the_image_net() {
        let dir = scratch("fold");
        std::fs::write(
            dir.join("board-net.toml"),
            "[eth0]\nnetwork = \"qemu\"\nhost = 10\n",
        )
        .unwrap();
        let files = vec!["prj.conf".to_string(), "board-net.toml".to_string()];

        let got = fold_board_net_files(&BTreeMap::new(), &dir, &files, "image.t").unwrap();
        assert_eq!(got["eth0"].network.as_deref(), Some("qemu"));
        assert_eq!(got["eth0"].host, Some(10));

        let inline: BTreeMap<String, ImageNetIf> = [(
            "eth0".to_string(),
            ImageNetIf {
                host: Some(11),
                ..ImageNetIf::default()
            },
        )]
        .into();
        let e = fold_board_net_files(&inline, &dir, &files, "image.t").unwrap_err();
        assert!(
            e.contains("`eth0` is stated twice") && e.contains("[image.t.net.eth0]"),
            "{e}"
        );

        let e = fold_board_net_files(
            &BTreeMap::new(),
            &dir,
            &["missing.toml".to_string()],
            "image.t",
        )
        .unwrap_err();
        assert!(e.contains("missing.toml") && e.contains("os_config"), "{e}");

        std::fs::write(dir.join("bad.toml"), "[eth0]\nhots = 1\n").unwrap();
        let e = fold_board_net_files(&BTreeMap::new(), &dir, &["bad.toml".to_string()], "image.t")
            .unwrap_err();
        assert!(e.contains("RFC-0109 D4"), "a misspelt key is refused: {e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

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

    fn check(src: &str) -> Result<(), String> {
        let d = doc(src);
        check_image_interfaces("a", &parse_networks(&d)?, &image_net(&d, "a"))
    }

    fn addr(src: &str) -> Result<Option<StatedAddress>, String> {
        let d = doc(src);
        let nets = parse_networks(&d)?;
        let ifs = image_net(&d, "a");
        let (_, i) = ifs.iter().next().expect("one interface");
        let n = i.network.as_deref().expect("a network");
        stated_address("at", n, &network_def(n, &nets, "at")?, i)
    }

    #[test]
    fn host_on_the_builtin_qemu_network_is_an_address_in_its_subnet() {
        assert_eq!(
            addr("[image.a.net.eth0]\nnetwork = \"qemu\"\nhost = 50\n").unwrap(),
            Some(StatedAddress::Static([10, 0, 2, 50]))
        );
    }

    #[test]
    fn an_explicit_physical_network_needs_no_attach() {
        let src = "[network.lab]\nsubnet = \"192.168.50.0/24\"\ngateway = \"192.168.50.1\"\n\n\
                   [image.a.net.eth0]\nnetwork = \"lab\"\naddress = \"192.168.50.20\"\n";
        let nets = parse_networks(&doc(src)).unwrap();
        assert!(nets["lab"].attach.is_none());
        assert!(!nets["lab"].is_per_image(), "a physical network is shared");
        assert!(builtin("qemu").unwrap().is_per_image(), "SLIRP is per VM");
        assert_eq!(
            addr(src).unwrap(),
            Some(StatedAddress::Static([192, 168, 50, 20]))
        );
    }

    #[test]
    fn an_interface_that_states_nothing_is_left_to_the_resolver() {
        assert_eq!(
            addr("[image.a.net.eth0]\nnetwork = \"qemu\"\n").unwrap(),
            None
        );
        // No network at all: the board's default network, which only the
        // resolver knows — not checkable here, and not an error here.
        check("[image.a.net.eth0]\nhost = 50\n").unwrap();
    }

    #[test]
    fn a_lease_needs_a_network_that_leases() {
        let src = "[network.lab]\nsubnet = \"192.168.50.0/24\"\ndhcp = true\n\n\
                   [image.a.net.eth0]\nnetwork = \"lab\"\naddress = \"dhcp\"\n";
        assert_eq!(addr(src).unwrap(), Some(StatedAddress::Dhcp));
    }

    #[test]
    fn refusals_name_what_to_write() {
        let cases = [
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
                "[image.a.net.eth0]\nnetwork = \"qemu\"\nmac = \"02:00:00:00:01\"\n",
                "six hex bytes",
            ),
            (
                "[image.a.net.eth0]\nnetwork = \"qemu\"\nmac = \"03:00:00:00:00:01\"\n",
                "multicast",
            ),
        ];
        for (src, want) in cases {
            let e = check(src).expect_err(src);
            assert!(e.contains(want), "{src}\n-> {e}");
        }
    }

    #[test]
    fn the_router_of_the_first_interfaces_network_is_the_default_locator() {
        let d = doc(
            "[network.lab]\nsubnet = \"192.168.50.0/24\"\nrouter = \"tcp/192.168.50.1:7447\"\n\n\
             [image.a.net.eth0]\nnetwork = \"lab\"\nhost = 20\n",
        );
        let nets = parse_networks(&d).unwrap();
        assert_eq!(
            network_router(&nets, &image_net(&d, "a")).as_deref(),
            Some("tcp/192.168.50.1:7447")
        );
        let e = parse_networks(&doc(
            "[network.lab]\nsubnet = \"192.168.50.0/24\"\nrouter = \"192.168.50.1\"\n",
        ))
        .unwrap_err();
        assert!(e.contains("not a locator"), "{e}");
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
        let br = parse_networks(&doc(
            "[network.br]\nsubnet = \"192.168.1.0/24\"\nattach = { bridge = \"br0\" }\n",
        ))
        .unwrap();
        assert_eq!(
            br["br"].attach,
            Some(Attach::Bridge {
                bridge: "br0".into()
            })
        );
        assert!(!br["br"].is_per_image(), "a bridge is shared");
        let e = parse_networks(&doc(
            "[network.br]\nsubnet = \"192.168.1.0/24\"\nattach = { tap = \"tap 0\" }\n",
        ))
        .unwrap_err();
        assert!(e.contains("not a device name"), "{e}");
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
