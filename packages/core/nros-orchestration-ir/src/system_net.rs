//! RFC-0109 D4 (phase-514) — an image's network, read ONE way: the board
//! network files its `os_config` names, and the runner's NIC in
//! `[image.<id>.run]`.
//!
//! An image's address is a layer-1 fact. Where nano-ros is the OS (a
//! reference FreeRTOS / ThreadX / bare-metal board, `[board.net] stack =
//! "board"`), it lives in the board package's small network file — one
//! `[<interface>]` table per interface, and a `[network.<name>]` table for each
//! network other than the built-in [`QEMU_NETWORK`] — named by the image's
//! `os_config` and read here. How an EMULATED image joins that network is a
//! runner fact, `[image.<id>.run] nic`, and nothing else in the image reads it.
//!
//! The board's half — which interfaces it has, the default network an
//! unnamed interface lands on, the MAC an image does not state, and the
//! duplicate-address refusal across images — needs the board catalog, so it is
//! the CLI's resolver (`nros_cli_core::orchestration::image_net`). Its answer
//! reaches every build as one text (`nros_board_common::image_net`);
//! `nros::main!` bakes that text and does not resolve anything itself.
//!
//! The spellings this replaced — `[network.<name>]` and `[image.<id>.net.*]`
//! in `system.toml` (RFC-0103 D10, phase-497), and before them the flat
//! `[image.<id>] ip` / `gateway` / `netmask` (D10g) — are refused by name,
//! naming the new home (`crate::retired_tables`, [`refuse_flat_image_net`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The built-in network every QEMU board defaults onto: QEMU's own user-mode
/// (SLIRP) network as `-nic user` configures it.
pub const QEMU_NETWORK: &str = "qemu";

/// The keys D10g retired from `[image.<id>]` / `[image_defaults]`.
pub const RETIRED_FLAT_KEYS: &[&str] = &["ip", "gateway", "netmask"];

/// `[network.<name>]` in a board network file: what a network IS. How an
/// emulated image attaches to it is the image's `[image.<id>.run] nic`, and
/// the locator it dials is the image's `locator` (RFC-0109 D6).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkDef {
    /// The subnet, CIDR (`192.168.50.0/24`). Required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subnet: Option<String>,
    /// The subnet's default route, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,
    /// The network hands out DHCP leases.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dhcp: bool,
}

/// `nic = "slirp"`, `{ bridge = "<host dev>" }` or `{ tap = "<host dev>" }`:
/// how an emulated image's NIC joins the host.
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

impl Attach {
    /// QEMU's user-mode network, private to each VM.
    #[must_use]
    pub fn slirp() -> Self {
        Self::Mode("slirp".to_string())
    }

    /// Whether every image attached this way gets a PRIVATE instance of its
    /// network, so two of them never meet and cannot collide: QEMU's SLIRP,
    /// which every `-nic user` creates per VM. A bridge or a TAP is shared,
    /// and the resolver refuses a duplicate address or MAC on it.
    #[must_use]
    pub fn is_per_image(&self) -> bool {
        matches!(self, Self::Mode(m) if m == "slirp")
    }

    /// Check the value. `at` names the key, for the message.
    pub fn validate(&self, at: &str) -> Result<(), String> {
        match self {
            Self::Mode(m) if m == "slirp" => Ok(()),
            // phase-497 W3 — the QEMU launchers render these (`-nic bridge` /
            // `-nic tap`, `nros_board_common::image_net::qemu_nic`).
            Self::Bridge { bridge: d } | Self::Tap { tap: d } => check_host_device(at, d),
            Self::Mode(m) => Err(format!(
                "{at}: nic = {m:?} — the bare value is \"slirp\"; a bridge or TAP names its \
                 host device: `nic = {{ bridge = \"br0\" }}` / `{{ tap = \"tap0\" }}`"
            )),
        }
    }
}

/// `[image.<id>.run]` (RFC-0109 D6, phase-514 W6): facts about how this image
/// is RUN, read by the QEMU launchers and the test harness only. The image's
/// build never reads them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageRun {
    /// How the emulator attaches the image's first interface. Absent: QEMU's
    /// user-mode network on [`QEMU_NETWORK`], and none (a physical board) on
    /// any other network.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nic: Option<Attach>,
}

impl ImageRun {
    /// Check the stated facts. `id` is the image id.
    pub fn validate(&self, id: &str) -> Result<(), String> {
        match &self.nic {
            Some(n) => n.validate(&format!("[image.{id}.run]")),
            None => Ok(()),
        }
    }

    /// How an interface on `network` attaches: the stated `nic` for the
    /// image's FIRST interface (`first`), else QEMU's user-mode network on
    /// [`QEMU_NETWORK`], else none.
    #[must_use]
    pub fn attach_for(&self, network: &str, first: bool) -> Option<Attach> {
        match (&self.nic, first) {
            (Some(n), true) => Some(n.clone()),
            _ if network == QEMU_NETWORK => Some(Attach::slirp()),
            _ => None,
        }
    }
}

/// `[image.<id>.run]` from an image's raw table (the leaf road), checked.
pub fn parse_run(image: Option<&toml::Table>, id: &str) -> Result<ImageRun, String> {
    let Some(v) = image.and_then(|t| t.get("run")) else {
        return Ok(ImageRun::default());
    };
    let run: ImageRun = v
        .clone()
        .try_into()
        .map_err(|e| format!("[image.{id}.run]: {e}"))?;
    run.validate(id)?;
    Ok(run)
}

/// One interface's address, as a board network file states it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageNetIf {
    /// The network this interface sits on: `qemu`, or a `[network.<name>]` of
    /// the same file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    /// A full IPv4 address, or `"dhcp"` for a lease.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// The last octet within the network's subnet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<u8>,
    /// The interface's MAC (`02:00:00:00:00:01`). Derived when absent.
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
        dhcp: false,
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

/// A bridge or TAP `nic` names a host network device, which the QEMU
/// launcher splices into a `-nic` argument: empty, whitespace or a comma
/// would not survive that.
fn check_host_device(at: &str, d: &str) -> Result<(), String> {
    if d.is_empty() || d.contains(char::is_whitespace) || d.contains(',') {
        return Err(format!(
            "{at}: nic names host device {d:?}, which is not a device name"
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
        Ok(())
    }

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
}

/// The keys a `[network.<name>]` held in `system.toml` (RFC-0103 D10a) that do
/// not move into a board network file with it, and where each went.
const MOVED_NETWORK_KEYS: &[(&str, &str)] = &[
    (
        "router",
        "the locator an image dials is the image's own `locator` (or `[system] locator`) — \
         RFC-0109 D6",
    ),
    (
        "attach",
        "how an emulated image joins a network is a runner fact: `[image.<id>.run] nic = \
         \"slirp\" | { bridge = \"<dev>\" } | { tap = \"<dev>\" }` — RFC-0109 D6",
    ),
];

/// The `[network.*]` of one board network file (`file` names it), typed and
/// validated. Redefining the built-in `qemu` is refused: it is QEMU's own
/// plan, not a project's.
fn parse_network_table(
    v: &toml::Value,
    file: &str,
) -> Result<BTreeMap<String, NetworkDef>, String> {
    let t = v
        .as_table()
        .ok_or_else(|| format!("{file}: `network` must be a table of `[network.<name>]`"))?;
    let mut out = BTreeMap::new();
    for (name, def) in t {
        let at = format!("{file}: [network.{name}]");
        if builtin(name).is_some() {
            return Err(format!(
                "{at}: `{name}` is built in (QEMU's own SLIRP network, 10.0.2.0/24) and \
                 cannot be redefined; name your network something else"
            ));
        }
        if let Some((key, home)) = MOVED_NETWORK_KEYS
            .iter()
            .find(|(k, _)| def.get(k).is_some())
        {
            return Err(format!("{at}: `{key}` does not belong here — {home}"));
        }
        let def: NetworkDef = def.clone().try_into().map_err(|e| format!("{at}: {e}"))?;
        def.validate(name).map_err(|e| format!("{file}: {e}"))?;
        out.insert(name.clone(), def);
    }
    Ok(out)
}

/// One board network file (RFC-0109 D4), read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoardNetFile {
    /// `[<interface>]` — the image's address on each interface.
    pub interfaces: BTreeMap<String, ImageNetIf>,
    /// `[network.<name>]` — the networks those interfaces sit on, other than
    /// the built-in [`QEMU_NETWORK`].
    pub networks: BTreeMap<String, NetworkDef>,
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

/// Read one board network file: a table per interface, each with the fields
/// of [`ImageNetIf`] (`network`, `host` or `address`, `mac`), and a
/// `[network.<name>]` table per network they name.
pub fn read_board_net_file(path: &std::path::Path) -> Result<BoardNetFile, String> {
    let file = path.display().to_string();
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("{file}: {e} (named by `os_config`)"))?;
    let mut doc: toml::Table = toml::from_str(&text).map_err(|e| format!("{file}: {e}"))?;
    let networks = match doc.remove("network") {
        Some(v) => parse_network_table(&v, &file)?,
        None => BTreeMap::new(),
    };
    let interfaces = toml::Value::Table(doc).try_into().map_err(|e| {
        format!(
            "{file}: {e} (a board network file is one `[<interface>]` table per interface, \
             with `network`, `host` or `address`, and `mac`, plus a `[network.<name>]` per \
             network other than `qemu` — RFC-0109 D4)"
        )
    })?;
    Ok(BoardNetFile {
        interfaces,
        networks,
    })
}

/// An image's network: every board network file its `os_config` names,
/// resolved against `dir` (the `system.toml`'s directory). An interface stated
/// in two files is refused, naming both: one owner per fact (RFC-0103), and a
/// file is that owner (RFC-0109 D4). A network two files define differently is
/// refused the same way. `at` names the image table.
pub fn fold_board_net_files(
    dir: &std::path::Path,
    os_config: &[String],
    at: &str,
) -> Result<BoardNetFile, String> {
    let mut out = BoardNetFile::default();
    let mut owner: BTreeMap<String, String> = BTreeMap::new();
    for file in board_net_files(os_config) {
        let path = dir.join(file);
        let read = read_board_net_file(&path)?;
        for (name, decl) in read.interfaces {
            if let Some(prev) = owner.get(&name) {
                return Err(format!(
                    "`{at}`: interface `{name}` is stated twice — in {prev} and in {} — so \
                     the image has two owners for one address (RFC-0109 D4: one board \
                     network file owns each interface)",
                    path.display()
                ));
            }
            owner.insert(name.clone(), path.display().to_string());
            out.interfaces.insert(name, decl);
        }
        merge_networks(
            &mut out.networks,
            read.networks,
            &path.display().to_string(),
        )?;
    }
    Ok(out)
}

/// Add `more` (from `from`) to `into`, refusing a network defined twice with
/// different facts. Used within one image's files and, by the workspace
/// resolver, across images: one network has one subnet.
pub fn merge_networks(
    into: &mut BTreeMap<String, NetworkDef>,
    more: BTreeMap<String, NetworkDef>,
    from: &str,
) -> Result<(), String> {
    for (name, def) in more {
        match into.get(&name) {
            Some(prev) if *prev != def => {
                return Err(format!(
                    "{from}: [network.{name}] disagrees with another board network file's \
                     ({prev:?} vs {def:?}) — one network has one subnet, gateway and lease \
                     policy; make the files agree"
                ));
            }
            Some(_) => {}
            None => {
                into.insert(name, def);
            }
        }
    }
    Ok(())
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
        "`{at}` states `{key}`, which is retired (RFC-0103 D10g): an image's address is a \
         board fact now (RFC-0109 D4). Name a board network file in the image's \
         `os_config = [\"board-net.toml\"]`, holding\n\n    [eth0]\n    network = \"qemu\"          \
         # or a [network.<name>] defined in the same file\n    host = 50                   \
         # or: address = \"10.0.2.50\"\n\nand drop `ip` / `gateway` / `netmask` — the gateway \
         and netmask are the network's (`[network.<name>] subnet` / `gateway`)."
    )
}

/// The network named `name`: the built-in [`QEMU_NETWORK`] or one of the
/// image's board network files'. `at` is the table that named it, for the
/// message.
pub fn network_def(
    name: &str,
    networks: &BTreeMap<String, NetworkDef>,
    at: &str,
) -> Result<NetworkDef, String> {
    match builtin(name) {
        Some(b) => Ok(b),
        None => networks.get(name).cloned().ok_or_else(|| {
            format!(
                "{at}: network = {name:?} is not the built-in \"{QEMU_NETWORK}\", and no \
                 board network file of this image defines `[network.{name}]`"
            )
        }),
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
        let at = format!("image `{id}`, interface `{ifname}`");
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
    /// interfaces; an interface stated in two files has two owners and is
    /// refused; a non-`.toml` entry belongs to the RTOS and is not read.
    #[test]
    fn os_config_board_net_files_fold_into_the_image_net() {
        let dir = scratch("fold");
        std::fs::write(
            dir.join("board-net.toml"),
            "[eth0]\nnetwork = \"qemu\"\nhost = 10\n",
        )
        .unwrap();
        std::fs::write(dir.join("again.toml"), "[eth0]\nhost = 11\n").unwrap();
        let files = vec!["prj.conf".to_string(), "board-net.toml".to_string()];

        let got = fold_board_net_files(&dir, &files, "image.t").unwrap();
        assert_eq!(got.interfaces["eth0"].network.as_deref(), Some("qemu"));
        assert_eq!(got.interfaces["eth0"].host, Some(10));
        assert!(got.networks.is_empty());

        let twice = vec!["board-net.toml".to_string(), "again.toml".to_string()];
        let e = fold_board_net_files(&dir, &twice, "image.t").unwrap_err();
        assert!(
            e.contains("`eth0` is stated twice") && e.contains("again.toml"),
            "{e}"
        );

        let e = fold_board_net_files(&dir, &["missing.toml".to_string()], "image.t").unwrap_err();
        assert!(e.contains("missing.toml") && e.contains("os_config"), "{e}");

        std::fs::write(dir.join("bad.toml"), "[eth0]\nhots = 1\n").unwrap();
        let e = fold_board_net_files(&dir, &["bad.toml".to_string()], "image.t").unwrap_err();
        assert!(e.contains("RFC-0109 D4"), "a misspelt key is refused: {e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RFC-0109 D4/D6 (phase-514 W4) — a project network moves into the board
    /// network file with its `subnet` / `gateway` / `dhcp`; its `router` and
    /// `attach` do not, and are refused there naming where they went.
    #[test]
    fn a_board_net_file_defines_its_networks_and_refuses_router_and_attach() {
        let dir = scratch("nets");
        std::fs::write(
            dir.join("lab.toml"),
            "[eth0]\nnetwork = \"lab\"\nhost = 20\n\n\
             [network.lab]\nsubnet = \"192.168.50.0/24\"\ngateway = \"192.168.50.1\"\n",
        )
        .unwrap();
        let got = fold_board_net_files(&dir, &["lab.toml".to_string()], "image.a").unwrap();
        assert_eq!(got.networks["lab"].gateway.as_deref(), Some("192.168.50.1"));
        check_image_interfaces("a", &got.networks, &got.interfaces).unwrap();

        for (body, key, names) in [
            (
                "router = \"tcp/192.168.50.1:7447\"\n",
                "router",
                "`locator`",
            ),
            (
                "attach = { tap = \"tap0\" }\n",
                "attach",
                "[image.<id>.run] nic",
            ),
        ] {
            std::fs::write(
                dir.join("moved.toml"),
                format!("[network.lab]\nsubnet = \"192.168.50.0/24\"\n{body}"),
            )
            .unwrap();
            let e = fold_board_net_files(&dir, &["moved.toml".to_string()], "image.a").unwrap_err();
            assert!(
                e.contains(&format!("`{key}` does not belong here")) && e.contains(names),
                "{e}"
            );
        }

        // Two files of one image, one network, two subnets: refused.
        std::fs::write(
            dir.join("lab2.toml"),
            "[eth1]\nnetwork = \"lab\"\nhost = 21\n\n\
             [network.lab]\nsubnet = \"192.168.51.0/24\"\n",
        )
        .unwrap();
        let e = fold_board_net_files(
            &dir,
            &["lab.toml".to_string(), "lab2.toml".to_string()],
            "image.a",
        )
        .unwrap_err();
        assert!(e.contains("disagrees"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_run_nic_is_the_first_interfaces_attach() {
        let run = ImageRun {
            nic: Some(Attach::Tap { tap: "tap0".into() }),
        };
        assert_eq!(
            run.attach_for("lab", true),
            Some(Attach::Tap { tap: "tap0".into() })
        );
        assert_eq!(run.attach_for("qemu", false), Some(Attach::slirp()));
        assert_eq!(run.attach_for("lab", false), None, "a physical network");
        let none = ImageRun::default();
        assert_eq!(none.attach_for("qemu", true), Some(Attach::slirp()));
        assert!(Attach::slirp().is_per_image(), "SLIRP is per VM");
        assert!(
            !Attach::Bridge {
                bridge: "br0".into()
            }
            .is_per_image(),
            "a bridge is shared"
        );

        let parse = |src: &str| {
            let t: toml::Table = src.parse().unwrap();
            parse_run(t["image"]["a"].as_table(), "a")
        };
        assert!(
            parse("[image.a.run]\nnic = \"tap\"\n")
                .unwrap_err()
                .contains("bare value")
        );
        assert!(
            parse("[image.a.run]\nnic = { tap = \"tap 0\" }\n")
                .unwrap_err()
                .contains("not a device name")
        );
        assert!(
            parse("[image.a.run]\nmode = 1\n")
                .unwrap_err()
                .contains("unknown field")
        );
        assert_eq!(
            parse("[image.a.run]\nnic = { bridge = \"qemu-br\" }\n")
                .unwrap()
                .nic,
            Some(Attach::Bridge {
                bridge: "qemu-br".into()
            })
        );
    }

    fn one(network: &str, host: Option<u8>, address: Option<&str>) -> ImageNetIf {
        ImageNetIf {
            network: Some(network.into()),
            host,
            address: address.map(str::to_string),
            mac: None,
        }
    }

    fn lab(dhcp: bool) -> BTreeMap<String, NetworkDef> {
        [(
            "lab".to_string(),
            NetworkDef {
                subnet: Some("192.168.50.0/24".into()),
                gateway: Some("192.168.50.1".into()),
                dhcp,
            },
        )]
        .into()
    }

    fn addr(
        nets: &BTreeMap<String, NetworkDef>,
        i: &ImageNetIf,
    ) -> Result<Option<StatedAddress>, String> {
        let n = i.network.as_deref().expect("a network");
        stated_address("at", n, &network_def(n, nets, "at")?, i)
    }

    #[test]
    fn host_on_the_builtin_qemu_network_is_an_address_in_its_subnet() {
        assert_eq!(
            addr(&BTreeMap::new(), &one("qemu", Some(50), None)).unwrap(),
            Some(StatedAddress::Static([10, 0, 2, 50]))
        );
    }

    #[test]
    fn an_address_on_a_defined_network_is_checked_against_its_subnet() {
        assert_eq!(
            addr(&lab(false), &one("lab", None, Some("192.168.50.20"))).unwrap(),
            Some(StatedAddress::Static([192, 168, 50, 20]))
        );
        assert_eq!(addr(&lab(false), &one("qemu", None, None)).unwrap(), None);
        assert_eq!(
            addr(&lab(true), &one("lab", None, Some("dhcp"))).unwrap(),
            Some(StatedAddress::Dhcp)
        );
    }

    #[test]
    fn refusals_name_what_to_write() {
        let nets = lab(false);
        let mac = |m: &str| ImageNetIf {
            mac: Some(m.into()),
            ..one("qemu", None, None)
        };
        let cases = [
            (one("other", Some(5), None), "defines `[network.other]`"),
            (one("qemu", Some(2), None), "gateway"),
            (one("qemu", None, Some("10.0.3.5")), "not a host address"),
            (one("qemu", Some(5), Some("10.0.2.5")), "not both"),
            (one("lab", None, Some("dhcp")), "states no `dhcp = true`"),
            (mac("02:00:00:00:01"), "six hex bytes"),
            (mac("03:00:00:00:00:01"), "multicast"),
        ];
        for (i, want) in cases {
            let ifs = [("eth0".to_string(), i.clone())].into();
            let e = check_image_interfaces("a", &nets, &ifs).expect_err(want);
            assert!(e.contains(want), "{i:?}\n-> {e}");
        }
    }

    #[test]
    fn network_tables_are_validated() {
        let dir = scratch("validate");
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
                "[network.lab]\nsubnet = \"192.168.1.0/24\"\nmode = \"bridge\"\n",
                "unknown field",
            ),
        ] {
            std::fs::write(dir.join("n.toml"), src).unwrap();
            let e = read_board_net_file(&dir.join("n.toml")).expect_err(src);
            assert!(e.contains(want), "{src}\n-> {e}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_flat_spellings_are_refused_naming_the_new_home() {
        let d: toml::Table = "[image.a]\nboard = \"x\"\nip = \"10.0.2.50\"\n"
            .parse()
            .unwrap();
        let img = d
            .get("image")
            .and_then(|i| i.get("a"))
            .and_then(|i| i.as_table());
        let e = refuse_flat_image_net(img, "image.a").unwrap_err();
        assert!(e.contains("os_config") && e.contains("D10g"), "{e}");
    }
}
