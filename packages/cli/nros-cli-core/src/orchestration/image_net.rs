//! RFC-0103 D10 (phase-497 W2) — the one resolver of an image's network.
//!
//! An image's network has three sources: the board states its INTERFACES
//! (`[board.net]`), the board network files the image's `os_config` names
//! state its ADDRESS on each and the NETWORKS they sit on (RFC-0109 D4), and
//! `[image.<id>.run] nic` says how an emulated image attaches (RFC-0109 D6).
//! This module puts them together, once, for every road:
//!
//! * an interface with no `network` lands on the board's default network
//!   (`[board.net] network`, `qemu` on every QEMU board);
//! * an interface with no address on [`QEMU_NETWORK`] takes QEMU's first guest
//!   address, which is what a plain `-nic user` guest would pick; on any other
//!   network the image must say;
//! * a MAC the image does not state is DERIVED from the interface's
//!   `mac_prefix` and `(system, image, network, interface)` — never copied from
//!   a board constant, because a board constant shared by two images was
//!   exactly the "identical IP and MAC, silent no-delivery" trap (D10b);
//! * a lease (`address = "dhcp"`) needs a network that leases AND a board that
//!   runs a DHCP client on that interface;
//! * an image on a network other than `qemu` needs a locator — its own or the
//!   project's (D10d) -- when its RMW dials a host peer: the RMW descriptor's
//!   `[rmw.host] peer` is not `"none"` (issue 2679). A Cyclone DDS or uORB
//!   image has no router to name.
//!
//! On a Zephyr image the resolved interface is also rendered into Zephyr's
//! own `NET_CONFIG_*` / `NET_DHCPV4` as a Kconfig fragment,
//! `<build>/nros/net/<image>.conf` ([`zephyr_conf`], issue 2679), which the
//! west road appends last to `EXTRA_CONF_FILE`.
//!
//! The answer is written to `<build>/nros/net/<image>.toml` for a person to
//! read, and handed to the build as ONE text, `NROS_IMAGE_NET`
//! (`nros_board_common::image_net`), which every carrier reads: the
//! `nros::main!` overlay, a board that boots a C app from its own `main.rs`,
//! and `nros board net --image-net` on the cmake road.
//!
//! Across images, [`check_collisions`] refuses two equal static addresses or
//! two equal MACs on one SHARED network, naming both images. QEMU's SLIRP
//! network is not shared: every `-nic user` makes a private copy per VM, so two
//! images on `qemu` never meet.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use nros_board_common::{
    image_net::{self as text, Address},
    platform_config::BoardNet,
};
use nros_extension::plan::HostPeer;
use nros_orchestration_ir::system_net::{
    self, ImageNetIf, NetworkDef, QEMU_FIRST_GUEST, QEMU_NETWORK, StatedAddress,
};

/// One resolved interface, owned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interface {
    /// The board's interface name.
    pub name: String,
    /// Its kind (`ethernet` / `wifi`).
    pub kind: String,
    /// The network it sits on.
    pub network: String,
    /// Its address.
    pub address: Address,
    /// The network's prefix length.
    pub prefix: u8,
    /// The network's gateway.
    pub gateway: Option<[u8; 4]>,
    /// The interface's MAC, stated or derived.
    pub mac: [u8; 6],
    /// Whether the MAC was derived.
    pub mac_derived: bool,
    /// Whether every image on this network gets a private instance of it.
    pub per_image: bool,
    /// How an emulated image joins the network (D10a), for the launcher.
    pub attach: NetAttach,
}

/// How an interface attaches (`[image.<id>.run] nic`, or the network's
/// default), owned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetAttach {
    /// `"slirp"`.
    Slirp,
    /// `{ bridge = "<dev>" }`.
    Bridge(String),
    /// `{ tap = "<dev>" }`.
    Tap(String),
    /// Absent: a physical-only network.
    None,
}

impl NetAttach {
    fn of(attach: Option<&system_net::Attach>) -> Self {
        match attach {
            Some(system_net::Attach::Mode(_)) => Self::Slirp,
            Some(system_net::Attach::Bridge { bridge }) => Self::Bridge(bridge.clone()),
            Some(system_net::Attach::Tap { tap }) => Self::Tap(tap.clone()),
            None => Self::None,
        }
    }

    fn text(&self) -> text::Attach<'_> {
        match self {
            Self::Slirp => text::Attach::Slirp,
            Self::Bridge(d) => text::Attach::Bridge(d),
            Self::Tap(d) => text::Attach::Tap(d),
            Self::None => text::Attach::None,
        }
    }
}

/// An image's resolved network: its interfaces, by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageNet {
    /// The interfaces, in name order; the first is the primary.
    pub interfaces: Vec<Interface>,
}

impl ImageNet {
    /// The `NROS_IMAGE_NET` text.
    #[must_use]
    pub fn text(&self) -> String {
        let ifs: Vec<text::NetIf<'_>> = self
            .interfaces
            .iter()
            .map(|i| text::NetIf {
                name: &i.name,
                kind: &i.kind,
                network: &i.network,
                address: i.address,
                prefix: i.prefix,
                gateway: i.gateway,
                mac: i.mac,
                attach: i.attach.text(),
            })
            .collect();
        text::encode(&ifs)
    }

    /// `<build>/nros/net/<image>.toml`'s content.
    #[must_use]
    pub fn to_toml(&self, image: &str) -> String {
        let dotted = |o: [u8; 4]| format!("{}.{}.{}.{}", o[0], o[1], o[2], o[3]);
        // RFC-0103 D10f, phase-497 W2; the board network file is RFC-0109 D4.
        let mut out = format!(
            "# Generated by nros — the resolved network of\n\
             # image `{image}`. Do not edit: state an address in the board network\n\
             # file its `os_config` names, and how it is attached in\n\
             # `[image.{image}.run] nic`.\n\
             text = {:?}\n",
            self.text()
        );
        for i in &self.interfaces {
            out.push_str(&format!("\n[interface.{}]\n", i.name));
            out.push_str(&format!("kind = {:?}\nnetwork = {:?}\n", i.kind, i.network));
            match i.address {
                Address::Static(ip) => out.push_str(&format!("address = {:?}\n", dotted(ip))),
                Address::Dhcp => out.push_str("address = \"dhcp\"\n"),
            }
            out.push_str(&format!(
                "netmask = {:?}\n",
                dotted(nros_board_common::netmask_from_prefix(i.prefix))
            ));
            if let Some(g) = i.gateway {
                out.push_str(&format!("gateway = {:?}\n", dotted(g)));
            }
            out.push_str(&format!(
                "mac = {:?}{}\n",
                text::format_mac(i.mac),
                if i.mac_derived { "  # derived" } else { "" }
            ));
            match &i.attach {
                NetAttach::Slirp => out.push_str("attach = \"slirp\"\n"),
                NetAttach::Bridge(d) => out.push_str(&format!("attach = {{ bridge = {d:?} }}\n")),
                NetAttach::Tap(d) => out.push_str(&format!("attach = {{ tap = {d:?} }}\n")),
                NetAttach::None => out.push_str("# attach: none — a physical-only network\n"),
            }
        }
        out
    }
}

/// Where an image's resolved network is written: `<build>/nros/net/<image>.toml`,
/// under the workspace's or the leaf's own `build/` — one path per image id,
/// not per coordinate, so a launcher finds it from the id alone (phase-497 W3).
#[must_use]
pub fn file_path(build_dir: &Path, image: &str) -> PathBuf {
    build_dir
        .join("nros")
        .join("net")
        .join(format!("{image}.toml"))
}

/// Write [`ImageNet::to_toml`] to [`file_path`], only when it changed.
pub fn write(build_dir: &Path, image: &str, net: &ImageNet) -> Result<PathBuf, String> {
    let path = file_path(build_dir, image);
    let body = net.to_toml(image);
    if std::fs::read_to_string(&path).ok().as_deref() == Some(body.as_str()) {
        return Ok(path);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// What one image states, for [`resolve`].
pub struct ImageDecl<'a> {
    /// `[system] name`, part of the derived-MAC key.
    pub system: Option<&'a str>,
    /// The image id.
    pub image: &'a str,
    /// The `[network.<name>]` of its board network files.
    pub networks: &'a BTreeMap<String, NetworkDef>,
    /// Its interfaces, from those files.
    pub ifs: &'a BTreeMap<String, ImageNetIf>,
    /// `[image.<id>.run]`.
    pub run: &'a system_net::ImageRun,
    /// The image's locator as resolved (image > system).
    pub locator: Option<&'a str>,
    /// What the image's RMW needs on the host side, from its descriptor's
    /// `[rmw.host] peer` (issue 2679, amending D10d). `None` when it could
    /// not be read: the locator rule then applies, as before.
    pub host_peer: Option<HostPeer>,
}

/// Issue 2679 -- the `[rmw.host] peer` the descriptor of `rmw` states, over
/// the RMW index of the directory `dir` sits in (its workspace's, so an
/// out-of-tree RMW answers as an in-tree one does). `None` when the RMW does
/// not resolve or its descriptor states no peer.
#[must_use]
pub fn host_peer_in(dir: &Path, rmw: &str) -> Option<HostPeer> {
    let index = crate::orchestration::rmw_index::for_dir(dir).ok()?;
    host_peer_of(&index, rmw)
}

/// [`host_peer_in`] over an index already built.
#[must_use]
pub fn host_peer_of(
    index: &crate::orchestration::rmw_index::RmwIndex,
    rmw: &str,
) -> Option<HostPeer> {
    crate::cmd::run_exec::declared_host_peer_in(index, rmw)
        .ok()
        .map(|(p, _)| p)
}

/// D10b — a MAC the image does not state: `prefix` + three bytes of an FNV-1a
/// hash of `(system, image, network, interface)`. Deterministic, so a rebuild
/// keeps it; keyed per image, so two images never share one by default.
#[must_use]
pub fn derive_mac(
    prefix: [u8; 3],
    system: Option<&str>,
    image: &str,
    network: &str,
    interface: &str,
) -> [u8; 6] {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in [system.unwrap_or(""), image, network, interface] {
        for b in part.bytes().chain([0u8]) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    let t = h.to_be_bytes();
    [prefix[0], prefix[1], prefix[2], t[0], t[1], t[2]]
}

/// Resolve one image against its board. `Ok(None)` when the image states no
/// interface: it keeps the board's default, and nothing is handed to the build.
pub fn resolve(d: &ImageDecl<'_>, board: &BoardNet) -> Result<Option<ImageNet>, String> {
    if d.ifs.is_empty() {
        return Ok(None);
    }
    let mut interfaces = Vec::new();
    for (n, (name, i)) in d.ifs.iter().enumerate() {
        let at = format!("image `{}`, interface `{name}`", d.image);
        // Every resolved interface carries an address, so only an IP kind may
        // be named here (D10c): a serial or custom link's endpoint is the
        // image's `locator`.
        board
            .check_image_interface(name, true)
            .map_err(|e| format!("{at}: {e}"))?;
        let decl = &board.interfaces[name];
        let network = match (&i.network, board.network()?) {
            (Some(n), _) => n.clone(),
            (None, Some(default)) => default.to_string(),
            (None, None) => {
                return Err(format!(
                    "{at}: names no `network`, and this board has no default network \
                     (`[board.net] network`) — add `network = \"<name>\"`"
                ));
            }
        };
        let net = system_net::network_def(&network, d.networks, &at)?;
        let address = match system_net::stated_address(&at, &network, &net, i)? {
            Some(StatedAddress::Static(ip)) => Address::Static(ip),
            Some(StatedAddress::Dhcp) => {
                if !decl.dhcp() {
                    return Err(format!(
                        "{at}: address = \"dhcp\", but this board runs no DHCP client on \
                         `{name}` (`[board.net] interfaces.{name}` states no `dhcp = true`) — \
                         state a static `host` / `address`"
                    ));
                }
                Address::Dhcp
            }
            None if network == QEMU_NETWORK => Address::Static(QEMU_FIRST_GUEST),
            None => {
                return Err(format!(
                    "{at}: states no address on network {network:?} — add `host = <n>`, \
                     `address = \"a.b.c.d\"`, or `address = \"dhcp\"` on a network that leases"
                ));
            }
        };
        let (mac, mac_derived) = match &i.mac {
            Some(m) => (system_net::parse_mac(&at, m)?, false),
            None => (
                derive_mac(decl.mac_prefix(name)?, d.system, d.image, &network, name),
                true,
            ),
        };
        let attach = d.run.attach_for(&network, n == 0);
        interfaces.push(Interface {
            name: name.clone(),
            kind: decl.kind().to_string(),
            prefix: net.subnet(&network)?.prefix,
            gateway: net.gateway_octets(&network)?,
            per_image: attach
                .as_ref()
                .is_some_and(system_net::Attach::is_per_image),
            attach: NetAttach::of(attach.as_ref()),
            network,
            address,
            mac,
            mac_derived,
        });
    }
    // D10d -- off QEMU's network the gateway is rarely the router, so an
    // image whose RMW dials a host peer (a zenoh router, an XRCE agent) must
    // name one somewhere. Issue 2679: read from the RMW's descriptor, never
    // from its name, so a Cyclone DDS image (`peer = "none"`) is not refused
    // and an out-of-tree RMW takes the rule its descriptor states.
    if let Some(first) = interfaces.first()
        && first.network != QEMU_NETWORK
        && d.locator.is_none()
        && d.host_peer != Some(HostPeer::None)
    {
        // RFC-0109 D6.
        return Err(format!(
            "image `{}` sits on network {:?}, off QEMU's own, and states no `locator` \
             for its router: add `locator = \"tcp/<router>:7447\"` to `[image.{}]` or \
             `[system]`",
            d.image, first.network, d.image
        ));
    }
    Ok(Some(ImageNet { interfaces }))
}

/// Refuse two equal static addresses, or two equal MACs, on one SHARED
/// network (D10b), naming both images. `images` is `(image id, its net)`.
pub fn check_collisions(images: &[(&str, &ImageNet)]) -> Result<(), String> {
    let mut addrs: BTreeMap<(&str, [u8; 4]), (&str, &str)> = BTreeMap::new();
    let mut macs: BTreeMap<(&str, [u8; 6]), (&str, &str)> = BTreeMap::new();
    let dotted = |o: [u8; 4]| format!("{}.{}.{}.{}", o[0], o[1], o[2], o[3]);
    for (image, net) in images {
        for i in net.interfaces.iter().filter(|i| !i.per_image) {
            if let Address::Static(ip) = i.address
                && let Some((other, oif)) = addrs.insert((&i.network, ip), (image, &i.name))
            {
                return Err(format!(
                    "images `{other}` ({oif}) and `{image}` ({}) both take {} on network {:?} \
                     — two hosts on one network cannot share an address; change one `host` / \
                     `address`",
                    i.name,
                    dotted(ip),
                    i.network
                ));
            }
            if let Some((other, oif)) = macs.insert((&i.network, i.mac), (image, &i.name)) {
                return Err(format!(
                    "images `{other}` ({oif}) and `{image}` ({}) both use MAC {} on network {:?} \
                     — a switch or DHCP server keys on it; change one image's `mac` (or drop \
                     both and let them be derived)",
                    i.name,
                    text::format_mac(i.mac),
                    i.network
                ));
            }
        }
    }
    Ok(())
}

/// Resolve every image of a workspace bringup's `system.toml`, then refuse a
/// collision across them. `board_net_of(image id)` answers the image's
/// board's `[board.net]`; it is asked only for an image that names an
/// `os_config` file, each of which is checked against the board's network
/// owner (RFC-0109 D9 check 6). The map holds the images that do.
///
/// `host_peer_of(rmw)` answers the `[rmw.host] peer` of an RMW's descriptor
/// ([`host_peer_in`]); it is asked only for an image that states no locator.
pub fn resolve_system(
    sys: &super::cargo_metadata_schema::SystemToml,
    mut board_net_of: impl FnMut(&str) -> Result<BoardNet, String>,
    mut host_peer_of: impl FnMut(&str) -> Option<HostPeer>,
) -> Result<BTreeMap<String, ImageNet>, String> {
    let mut out = BTreeMap::new();
    // One network has one subnet across the project, whichever image's file
    // states it.
    let mut networks = BTreeMap::new();
    for (id, img) in sys.resolved_images() {
        let files = img.os_config_files();
        if files.is_empty() {
            continue;
        }
        // An image with no interface is resolved here only for this check, so
        // a board this checkout cannot resolve is that image's own build's
        // error to report, not every sibling's.
        let board = match board_net_of(&id) {
            Ok(b) => b,
            Err(_) if img.net.is_empty() => continue,
            Err(e) => return Err(e),
        };
        // RFC-0109 D9 check 6 — each layer-1 file goes to the network's owner.
        board
            .check_os_config(&files)
            .map_err(|e| format!("image `{id}`: {e}"))?;
        if img.net.is_empty() {
            continue;
        }
        system_net::merge_networks(
            &mut networks,
            img.networks.clone(),
            &format!("image `{id}`"),
        )?;
        let locator = img.locator.clone().or_else(|| sys.system.locator.clone());
        let host_peer = if locator.is_none() {
            host_peer_of(&sys.resolved_rmw(Some(&id), None))
        } else {
            None
        };
        if let Some(n) = resolve(
            &ImageDecl {
                system: Some(&sys.system.name),
                image: &id,
                networks: &img.networks,
                ifs: &img.net,
                run: &img.run,
                locator: locator.as_deref(),
                host_peer,
            },
            &board,
        )? {
            out.insert(id, n);
        }
    }
    let pairs: Vec<(&str, &ImageNet)> = out.iter().map(|(k, v)| (k.as_str(), v)).collect();
    check_collisions(&pairs)?;
    Ok(out)
}

/// Resolve a single-package leaf's image (`nros_orchestration_ir::leaf_system`),
/// whose locator the leaf reader already resolved (image > system).
pub fn resolve_leaf(
    decl: &nros_orchestration_ir::leaf_system::LeafSystem,
    board: &BoardNet,
) -> Result<Option<ImageNet>, String> {
    let n = &decl.network;
    let image = decl.image.as_deref().unwrap_or("default");
    // Issue 2679 -- the leaf's RMW (`[image]` > `[image_defaults]` >
    // `[system]`, then the toolchain default `SystemToml::resolved_rmw`
    // falls back to), over the index of the leaf's own directory.
    let host_peer = if n.locator.is_none() && !n.net.is_empty() {
        decl.origin_path()
            .parent()
            .and_then(|dir| host_peer_in(dir, decl.rmw.as_deref().unwrap_or("zenoh")))
    } else {
        None
    };
    resolve(
        &ImageDecl {
            system: n.system.as_deref(),
            image,
            networks: &n.networks,
            ifs: &n.net,
            run: &n.run,
            locator: n.locator.as_deref(),
            host_peer,
        },
        board,
    )
}

/// `[board.net]` of `board` from the descriptor at `path` (the board
/// catalog's `NROS_BOARD_TOML`). A descriptor that states none is the empty
/// table: no interfaces, so an image naming one is refused.
pub fn board_net(path: &Path, board: &str) -> Result<BoardNet, String> {
    let file = nros_board_common::platform_config::BoardKnobsFile::load(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(file
        .board_net(Some(board))
        .map_err(|e| format!("{}: {e}", path.display()))?
        .unwrap_or_default())
}

/// Issue 2679 -- where a Zephyr image's rendered network fragment is written:
/// `<build>/nros/net/<image>.conf`, beside [`file_path`]'s `.toml`.
#[must_use]
pub fn zephyr_conf_path(build_dir: &Path, image: &str) -> PathBuf {
    file_path(build_dir, image).with_extension("conf")
}

/// Issue 2679 D3 -- the refusal of a second interface: Zephyr's `NET_CONFIG`
/// configures one.
fn second_interface(name: &str) -> String {
    format!(
        "Zephyr's NET_CONFIG configures one interface (the first auto-started); `{name}` \
         is a second -- configure it in the application, or remove it from the file"
    )
}

/// Issue 2679 D2 -- render a Zephyr image's resolved network as Zephyr's own
/// Kconfig (`NET_CONFIG_*`, `NET_DHCPV4`), the fragment the west road appends
/// LAST to `EXTRA_CONF_FILE`.
///
/// `NET_CONFIG_AUTO_INIT` is rendered because `NET_CONFIG_NEED_IPV4` exists
/// only under it (and its default is `n` on a USB network device).
/// `NET_DHCPV4=n` on a static address is required: with DHCPv4 on, Zephyr
/// adds the static address as overridable and a lease replaces it.
///
/// Refused (D3, D5): a board that declares a second interface, an image that
/// addresses one, and a stated `mac` (a Zephyr Ethernet driver takes its MAC
/// from devicetree or generates one).
pub fn zephyr_conf(net: &ImageNet, board: &BoardNet, image: &str) -> Result<String, String> {
    // D3 -- the board first: it is what declares the second interface.
    if let Some(second) = board.interfaces.keys().nth(1) {
        return Err(format!(
            "[board.net] interfaces: {}",
            second_interface(second)
        ));
    }
    if let Some(second) = net.interfaces.get(1) {
        return Err(format!(
            "image `{image}`: {}",
            second_interface(&second.name)
        ));
    }
    let Some(i) = net.interfaces.first() else {
        return Err(format!("image `{image}` resolved no interface"));
    };
    // D5.
    if !i.mac_derived {
        return Err(format!(
            "image `{image}`, interface `{}`: states `mac`, which a Zephyr image does not \
             take -- its Ethernet driver takes the MAC from devicetree \
             (`local-mac-address`) or generates one; drop `mac` from the board network file",
            i.name
        ));
    }
    let dotted = |o: [u8; 4]| format!("{}.{}.{}.{}", o[0], o[1], o[2], o[3]);
    // RFC-0103 D10f's comment shape; issue 2679 D2's table, row by row. The
    // header names no issue: a user reads this file.
    let mut out = format!(
        "# Generated by nros -- the network of image `{image}`, interface `{}`,\n\
         # from its board network file. Do not edit: state the address in that\n\
         # file. Appended last to EXTRA_CONF_FILE.\n\
         CONFIG_NET_CONFIG_SETTINGS=y\n\
         CONFIG_NET_CONFIG_AUTO_INIT=y\n\
         CONFIG_NET_CONFIG_NEED_IPV4=y\n",
        i.name
    );
    match i.address {
        Address::Static(ip) => {
            out.push_str("CONFIG_NET_DHCPV4=n\n");
            out.push_str(&format!(
                "CONFIG_NET_CONFIG_MY_IPV4_ADDR=\"{}\"\n",
                dotted(ip)
            ));
            out.push_str(&format!(
                "CONFIG_NET_CONFIG_MY_IPV4_NETMASK=\"{}\"\n",
                dotted(nros_board_common::netmask_from_prefix(i.prefix))
            ));
        }
        Address::Dhcp => {
            out.push_str("CONFIG_NET_DHCPV4=y\n");
            out.push_str("CONFIG_NET_CONFIG_MY_IPV4_ADDR=\"\"\n");
        }
    }
    if let Some(g) = i.gateway {
        out.push_str(&format!("CONFIG_NET_CONFIG_MY_IPV4_GW=\"{}\"\n", dotted(g)));
    }
    Ok(out)
}

/// The `CONFIG_<SYM>` rows a Kconfig fragment sets, in order: `SYM=value`,
/// and `# CONFIG_SYM is not set` as `n`.
fn kconfig_rows(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            if let Some(rest) = l.strip_prefix("# ")
                && let Some(sym) = rest.strip_suffix(" is not set")
                && sym.starts_with("CONFIG_")
            {
                return Some((sym.to_string(), "n".to_string()));
            }
            if l.starts_with('#') {
                return None;
            }
            let (k, v) = l.split_once('=')?;
            let k = k.trim();
            k.starts_with("CONFIG_")
                .then(|| (k.to_string(), v.trim().to_string()))
        })
        .collect()
}

/// Issue 2679 D4 -- refuse an `os_config` fragment that also sets a symbol
/// the board network file renders, naming the fragment, the symbol, its value
/// there and the board network file whose rendered value it contradicts. A
/// warning with one value winning would be issue 0876's class (a stated value
/// silently losing).
///
/// `fragments` are the image's own `os_config` fragments as resolved;
/// `net_files` the board network files it names.
pub fn check_zephyr_fragments(
    rendered: &str,
    fragments: &[PathBuf],
    net_files: &[String],
) -> Result<(), String> {
    let ours: BTreeMap<String, String> = kconfig_rows(rendered).into_iter().collect();
    for f in fragments {
        let text = std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?;
        if let Some((sym, theirs)) = kconfig_rows(&text)
            .into_iter()
            .find(|(sym, _)| ours.contains_key(sym))
        {
            return Err(format!(
                "os_config fragment `{}` sets {sym}={theirs}, and the board network file {} \
                 renders {sym}={} -- the address is the board network file's: remove \
                 {sym} from the fragment",
                f.display(),
                net_files
                    .iter()
                    .map(|n| format!("`{n}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
                ours[&sym]
            ));
        }
    }
    Ok(())
}

/// Write a Zephyr image's rendered fragment to [`zephyr_conf_path`], only
/// when it changed (so a configure is not re-run for nothing).
pub fn write_zephyr_conf(build_dir: &Path, image: &str, body: &str) -> Result<PathBuf, String> {
    let path = zephyr_conf_path(build_dir, image);
    if std::fs::read_to_string(&path).ok().as_deref() == Some(body) {
        return Ok(path);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nros_board_common::platform_config::{InterfaceDecl, InterfaceTable};

    fn qemu_board() -> BoardNet {
        BoardNet {
            network: Some("qemu".into()),
            interfaces: [("eth0".to_string(), InterfaceDecl::Kind("ethernet".into()))].into(),
            ..BoardNet::default()
        }
    }

    /// A test-only shorthand for what an image's board network files and
    /// `[image.<id>.run]` state: `[network.<n>]`, `[image.<id>.net.<if>]` and
    /// `[image.<id>.run]` in one document.
    fn parse(
        src: &str,
        image: &str,
    ) -> (
        BTreeMap<String, NetworkDef>,
        BTreeMap<String, ImageNetIf>,
        system_net::ImageRun,
    ) {
        let doc: toml::Table = src.parse().unwrap();
        let nets: BTreeMap<String, NetworkDef> = doc
            .get("network")
            .map(|v| v.clone().try_into().unwrap())
            .unwrap_or_default();
        let img = doc.get("image").and_then(|i| i.get(image));
        let ifs = img
            .and_then(|i| i.get("net"))
            .map(|v| v.clone().try_into().unwrap())
            .unwrap_or_default();
        let run = system_net::parse_run(img.and_then(|i| i.as_table()), image).unwrap();
        (nets, ifs, run)
    }

    fn run(
        src: &str,
        image: &str,
        board: &BoardNet,
        locator: Option<&str>,
    ) -> Result<Option<ImageNet>, String> {
        let (nets, ifs, r) = parse(src, image);
        resolve(
            &ImageDecl {
                system: Some("sys"),
                image,
                networks: &nets,
                ifs: &ifs,
                run: &r,
                locator,
                host_peer: None,
            },
            board,
        )
    }

    #[test]
    fn an_image_that_states_nothing_hands_nothing_to_the_build() {
        assert_eq!(
            run("[image.a]\nboard = \"x\"\n", "a", &qemu_board(), None).unwrap(),
            None
        );
    }

    #[test]
    fn an_unnamed_network_is_the_boards_default_and_an_unstated_address_qemus_first_guest() {
        let n = run("[image.a.net.eth0]\n", "a", &qemu_board(), None)
            .unwrap()
            .unwrap();
        let i = &n.interfaces[0];
        assert_eq!(i.network, "qemu");
        assert_eq!(i.address, Address::Static([10, 0, 2, 15]));
        assert_eq!((i.prefix, i.gateway), (24, Some([10, 0, 2, 2])));
        assert!(i.mac_derived && i.per_image);
        assert_eq!(&i.mac[..3], &[0x02, 0x00, 0x00], "the default prefix");
        // The text every carrier reads round-trips.
        let t = n.text();
        let back = nros_board_common::image_net::primary(&t).unwrap().unwrap();
        assert_eq!(back.ip(), Some([10, 0, 2, 15]));
        assert_eq!(back.mac, i.mac);
    }

    #[test]
    fn a_derived_mac_is_per_image_and_stable_and_takes_the_interfaces_prefix() {
        let mut board = qemu_board();
        board.interfaces.insert(
            "eth0".into(),
            InterfaceDecl::Full(InterfaceTable {
                kind: "ethernet".into(),
                mac_prefix: Some("52:54:00".into()),
                dhcp: false,
            }),
        );
        let src = "[image.a.net.eth0]\nhost = 10\n[image.b.net.eth0]\nhost = 11\n";
        let a = run(src, "a", &board, None).unwrap().unwrap();
        let b = run(src, "b", &board, None).unwrap().unwrap();
        assert_eq!(&a.interfaces[0].mac[..3], &[0x52, 0x54, 0x00]);
        assert_ne!(a.interfaces[0].mac, b.interfaces[0].mac);
        assert_eq!(a, run(src, "a", &board, None).unwrap().unwrap(), "stable");
        // A stated MAC wins and is not marked derived.
        let s = run(
            "[image.a.net.eth0]\nhost = 40\nmac = \"52:54:00:12:34:56\"\n",
            "a",
            &board,
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(s.interfaces[0].mac, [0x52, 0x54, 0, 0x12, 0x34, 0x56]);
        assert!(!s.interfaces[0].mac_derived);
    }

    /// The phase's acceptance, at the resolver: two images on one bridge
    /// network resolve, and swapping one's address (or MAC) onto the other is
    /// refused naming both.
    #[test]
    fn two_images_on_a_shared_network_resolve_and_a_duplicate_is_refused_naming_both() {
        let lab = "[network.lab]\nsubnet = \"192.0.3.0/24\"\ngateway = \"192.0.3.1\"\n";
        let src = format!(
            "{lab}[image.talker.net.eth0]\nnetwork = \"lab\"\nhost = 10\n\
             [image.listener.net.eth0]\nnetwork = \"lab\"\nhost = 11\n"
        );
        let board = qemu_board();
        let router = Some("tcp/192.0.3.1:7447");
        let t = run(&src, "talker", &board, router).unwrap().unwrap();
        let l = run(&src, "listener", &board, router).unwrap().unwrap();
        assert!(
            !t.interfaces[0].per_image,
            "a physical/bridge network is shared"
        );
        check_collisions(&[("talker", &t), ("listener", &l)]).unwrap();
        // A physical-only network: the launcher refuses an emulated image.
        let t_text = t.text();
        let ti = nros_board_common::image_net::primary(&t_text)
            .unwrap()
            .unwrap();
        assert!(
            nros_board_common::image_net::qemu_nic(&ti, "lan9118")
                .unwrap_err()
                .contains("physical-only")
        );
        // RFC-0109 D6 — the same network with the image's `run.nic` on a
        // bridge reaches the launcher.
        let bridged = format!("{src}[image.talker.run]\nnic = {{ bridge = \"qemu-br\" }}\n");
        let tb = run(&bridged, "talker", &board, router).unwrap().unwrap();
        assert!(tb.text().ends_with(",bridge=qemu-br"), "{}", tb.text());
        let tb_text = tb.text();
        let bi = nros_board_common::image_net::primary(&tb_text)
            .unwrap()
            .unwrap();
        assert!(
            nros_board_common::image_net::qemu_nic(&bi, "lan9118")
                .unwrap()
                .starts_with("bridge,br=qemu-br,model=lan9118,mac=")
        );
        assert!(
            tb.to_toml("talker")
                .contains("attach = { bridge = \"qemu-br\" }")
        );

        let dup = src.replace("host = 11", "host = 10");
        let l2 = run(&dup, "listener", &board, router).unwrap().unwrap();
        let e = check_collisions(&[("talker", &t), ("listener", &l2)]).unwrap_err();
        assert!(
            e.contains("`talker`") && e.contains("`listener`") && e.contains("192.0.3.10"),
            "{e}"
        );

        let mac = src
            .replace("host = 10\n", "host = 10\nmac = \"02:00:00:00:00:07\"\n")
            .replace("host = 11\n", "host = 11\nmac = \"02:00:00:00:00:07\"\n");
        let t3 = run(&mac, "talker", &board, router).unwrap().unwrap();
        let l3 = run(&mac, "listener", &board, router).unwrap().unwrap();
        let e = check_collisions(&[("talker", &t3), ("listener", &l3)]).unwrap_err();
        assert!(
            e.contains("MAC 02:00:00:00:00:07") && e.contains("`talker`"),
            "{e}"
        );
    }

    #[test]
    fn two_images_on_qemus_slirp_never_collide() {
        let src = "[image.a.net.eth0]\n[image.b.net.eth0]\n";
        let a = run(src, "a", &qemu_board(), None).unwrap().unwrap();
        let b = run(src, "b", &qemu_board(), None).unwrap().unwrap();
        assert_eq!(a.interfaces[0].address, b.interfaces[0].address);
        check_collisions(&[("a", &a), ("b", &b)]).unwrap();
    }

    /// W4 — a board with two interfaces states both, and an image addresses
    /// each on its own network; the text carries both, primary first.
    #[test]
    fn a_two_nic_image_resolves_and_renders_both_interfaces() {
        let mut board = qemu_board();
        board
            .interfaces
            .insert("eth1".into(), InterfaceDecl::Kind("ethernet".into()));
        let src = "[network.lab]\nsubnet = \"192.168.50.0/24\"\n\
                   [image.a.net.eth0]\nnetwork = \"qemu\"\nhost = 20\n\
                   [image.a.net.eth1]\nnetwork = \"lab\"\nhost = 20\n";
        let n = run(src, "a", &board, Some("tcp/10.0.2.2:7447"))
            .unwrap()
            .unwrap();
        let t = n.text();
        let ifs: Vec<_> = nros_board_common::image_net::interfaces(&t)
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(ifs.len(), 2);
        assert_eq!((ifs[0].name, ifs[0].ip()), ("eth0", Some([10, 0, 2, 20])));
        assert_eq!(
            (ifs[1].name, ifs[1].ip()),
            ("eth1", Some([192, 168, 50, 20]))
        );
        assert_ne!(ifs[0].mac, ifs[1].mac);
        let toml = n.to_toml("a");
        assert!(toml.contains("[interface.eth1]") && toml.contains("network = \"lab\""));
    }

    #[test]
    fn refusals_name_what_to_write() {
        let board = qemu_board();
        let lab = "[network.lab]\nsubnet = \"192.168.50.0/24\"\ndhcp = true\n";
        for (src, locator, want) in [
            (
                format!("{lab}[image.a.net.eth0]\nnetwork = \"lab\"\nhost = 5\n"),
                None,
                "states no `locator`",
            ),
            (
                format!("{lab}[image.a.net.eth0]\nnetwork = \"lab\"\n"),
                Some("tcp/x:1"),
                "states no address",
            ),
            (
                format!("{lab}[image.a.net.eth0]\nnetwork = \"lab\"\naddress = \"dhcp\"\n"),
                Some("tcp/x:1"),
                "runs no DHCP client",
            ),
            (
                "[image.a.net.wlan0]\nhost = 5\n".to_string(),
                None,
                "not one of this board's",
            ),
        ] {
            let e = run(&src, "a", &board, locator).unwrap_err();
            assert!(e.contains(want), "{src}\n-> {e}");
        }
        let host = BoardNet {
            stack: Some("host".into()),
            ..BoardNet::default()
        };
        assert!(
            run("[image.a.net.eth0]\n", "a", &host, None)
                .unwrap_err()
                .contains("drop `[image.<id>.net.eth0]`")
        );
    }

    // ---- issue 2679: a board network file on a Zephyr image -------------

    /// The FVP's `[board.net]` as issue 2679 D1 writes it.
    fn zephyr_board() -> BoardNet {
        BoardNet {
            stack: Some("os".into()),
            interfaces: [(
                "eth0".to_string(),
                InterfaceDecl::Full(InterfaceTable {
                    kind: "ethernet".into(),
                    mac_prefix: None,
                    dhcp: true,
                }),
            )]
            .into(),
            ..BoardNet::default()
        }
    }

    const TAP: &str = "[network.tap]\nsubnet = \"192.168.10.0/24\"\ngateway = \"192.168.10.1\"\n\
                       [image.a.net.eth0]\nnetwork = \"tap\"\naddress = \"192.168.10.2\"\n";
    const FVP: &str = "[network.fvp]\nsubnet = \"172.20.51.0/24\"\ndhcp = true\n\
                       [image.a.net.eth0]\nnetwork = \"fvp\"\naddress = \"dhcp\"\n";

    fn zephyr_net(src: &str) -> ImageNet {
        run(src, "a", &zephyr_board(), Some("tcp/192.168.10.1:7447"))
            .unwrap()
            .unwrap()
    }

    /// D2 -- a static address: the three `NET_CONFIG` switches, DHCPv4 off
    /// (else a lease replaces the address), the address, the subnet's
    /// netmask and the network's gateway.
    #[test]
    fn a_static_address_renders_net_config_with_dhcpv4_off() {
        let c = zephyr_conf(&zephyr_net(TAP), &zephyr_board(), "a").unwrap();
        let rows: Vec<&str> = c.lines().filter(|l| !l.starts_with('#')).collect();
        assert_eq!(
            rows,
            [
                "CONFIG_NET_CONFIG_SETTINGS=y",
                "CONFIG_NET_CONFIG_AUTO_INIT=y",
                "CONFIG_NET_CONFIG_NEED_IPV4=y",
                "CONFIG_NET_DHCPV4=n",
                "CONFIG_NET_CONFIG_MY_IPV4_ADDR=\"192.168.10.2\"",
                "CONFIG_NET_CONFIG_MY_IPV4_NETMASK=\"255.255.255.0\"",
                "CONFIG_NET_CONFIG_MY_IPV4_GW=\"192.168.10.1\"",
            ],
            "{c}"
        );
        // `host = n` is a static address too.
        let host = TAP.replace("address = \"192.168.10.2\"", "host = 7");
        let c = zephyr_conf(&zephyr_net(&host), &zephyr_board(), "a").unwrap();
        assert!(
            c.contains("CONFIG_NET_CONFIG_MY_IPV4_ADDR=\"192.168.10.7\"\n"),
            "{c}"
        );
        assert!(c.contains("CONFIG_NET_DHCPV4=n\n"), "{c}");
    }

    /// D2 -- a lease: DHCPv4 on, an empty static address (Zephyr then adds
    /// none and waits for the lease), no netmask; no gateway on a network
    /// that states none.
    #[test]
    fn a_lease_renders_dhcpv4_on_and_an_empty_address() {
        let c = zephyr_conf(&zephyr_net(FVP), &zephyr_board(), "a").unwrap();
        let rows: Vec<&str> = c.lines().filter(|l| !l.starts_with('#')).collect();
        assert_eq!(
            rows,
            [
                "CONFIG_NET_CONFIG_SETTINGS=y",
                "CONFIG_NET_CONFIG_AUTO_INIT=y",
                "CONFIG_NET_CONFIG_NEED_IPV4=y",
                "CONFIG_NET_DHCPV4=y",
                "CONFIG_NET_CONFIG_MY_IPV4_ADDR=\"\"",
            ],
            "{c}"
        );
        let dir = tempfile::tempdir().unwrap();
        let p = write_zephyr_conf(dir.path(), "a", &c).unwrap();
        assert_eq!(p, dir.path().join("nros/net/a.conf"));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), c);
    }

    /// D3 -- Zephyr's NET_CONFIG configures one interface: a board that
    /// declares a second is refused, and so is an image that addresses one.
    #[test]
    fn a_second_interface_is_refused_on_zephyr() {
        let mut two = zephyr_board();
        two.interfaces
            .insert("eth1".into(), InterfaceDecl::Kind("ethernet".into()));
        let net = zephyr_net(TAP);
        let e = zephyr_conf(&net, &two, "a").unwrap_err();
        assert!(
            e.contains("[board.net] interfaces")
                && e.contains("configures one interface (the first auto-started)")
                && e.contains("`eth1` is a second"),
            "{e}"
        );
        // An image file that addresses a second interface (resolved against
        // a board that has one), refused naming it.
        let src = format!("{TAP}[image.a.net.eth1]\nnetwork = \"tap\"\nhost = 9\n");
        let both = run(&src, "a", &two, Some("tcp/x:1")).unwrap().unwrap();
        let e = zephyr_conf(&both, &zephyr_board(), "a").unwrap_err();
        assert!(
            e.contains("image `a`") && e.contains("`eth1` is a second"),
            "{e}"
        );
    }

    /// D5 -- a stated `mac` is refused: a Zephyr Ethernet driver takes its MAC
    /// from devicetree or generates one. A derived one is simply not rendered.
    #[test]
    fn a_stated_mac_is_refused_on_zephyr() {
        let src = TAP.replace(
            "address = \"192.168.10.2\"\n",
            "address = \"192.168.10.2\"\nmac = \"02:00:00:12:34:56\"\n",
        );
        let e = zephyr_conf(&zephyr_net(&src), &zephyr_board(), "a").unwrap_err();
        assert!(
            e.contains("states `mac`") && e.contains("local-mac-address"),
            "{e}"
        );
        let c = zephyr_conf(&zephyr_net(TAP), &zephyr_board(), "a").unwrap();
        assert!(!c.contains("MAC") && !c.contains("mac"), "{c}");
    }

    /// D4 -- an `os_config` fragment that sets a rendered symbol is refused,
    /// naming the fragment, the symbol, both values and the board network
    /// file; one that sets only other symbols (the DHCP client's own delay)
    /// passes.
    #[test]
    fn a_fragment_that_sets_a_rendered_symbol_is_refused_naming_both_sources() {
        let dir = tempfile::tempdir().unwrap();
        let rendered = zephyr_conf(&zephyr_net(TAP), &zephyr_board(), "a").unwrap();
        let ok = dir.path().join("prj-fvp.conf");
        std::fs::write(
            &ok,
            "# boot faster\nCONFIG_NET_DHCPV4_INITIAL_DELAY_MAX=2\nCONFIG_NET_TCP=y\n",
        )
        .unwrap();
        let net_files = vec!["zephyr/net-fvp-tap.toml".to_string()];
        check_zephyr_fragments(&rendered, std::slice::from_ref(&ok), &net_files).unwrap();

        let bad = dir.path().join("prj-fvp-tap.conf");
        std::fs::write(&bad, "CONFIG_NET_L2_ETHERNET=y\nCONFIG_NET_DHCPV4=y\n").unwrap();
        let e =
            check_zephyr_fragments(&rendered, &[ok.clone(), bad.clone()], &net_files).unwrap_err();
        assert!(
            e.contains(&format!("`{}`", bad.display()))
                && e.contains("sets CONFIG_NET_DHCPV4=y")
                && e.contains("renders CONFIG_NET_DHCPV4=n")
                && e.contains("`zephyr/net-fvp-tap.toml`"),
            "{e}"
        );
        // Kconfig's own spelling of `n` is a statement too.
        std::fs::write(&bad, "# CONFIG_NET_CONFIG_AUTO_INIT is not set\n").unwrap();
        let e = check_zephyr_fragments(&rendered, &[bad], &net_files).unwrap_err();
        assert!(
            e.contains("sets CONFIG_NET_CONFIG_AUTO_INIT=n")
                && e.contains("renders CONFIG_NET_CONFIG_AUTO_INIT=y"),
            "{e}"
        );
    }

    /// D7 (amends RFC-0103 D10d) -- off `qemu`, no locator is refused only
    /// when the RMW dials a host peer. Cyclone DDS (`peer = "none"`) is
    /// accepted; zenoh still refused; an unreadable descriptor keeps the rule.
    #[test]
    fn the_locator_rule_follows_the_rmw_descriptors_host_peer() {
        let in_tree = cargo_nano_ros::rmw_resolver::test_support::in_tree_index();
        let cyclone = host_peer_of(&in_tree, "cyclonedds");
        let zenoh = host_peer_of(&in_tree, "zenoh");
        assert_eq!(cyclone, Some(HostPeer::None));
        assert_eq!(zenoh, Some(HostPeer::ZenohRouter));
        let decl = |peer: Option<HostPeer>| {
            let (nets, ifs, r) = parse(TAP, "a");
            resolve(
                &ImageDecl {
                    system: Some("sys"),
                    image: "a",
                    networks: &nets,
                    ifs: &ifs,
                    run: &r,
                    locator: None,
                    host_peer: peer,
                },
                &zephyr_board(),
            )
        };
        assert!(decl(cyclone).unwrap().is_some(), "Cyclone DDS: accepted");
        for peer in [zenoh, Some(HostPeer::XrceAgent), None] {
            let e = decl(peer).unwrap_err();
            assert!(e.contains("states no `locator`"), "{peer:?}: {e}");
        }
    }

    /// D7 -- the rule is read from the descriptor, never from an RMW name: an
    /// out-of-tree RMW (`acme`, unknown to the tree) takes the rule its own
    /// `[rmw.host] peer` states, either way.
    #[test]
    fn an_out_of_tree_rmw_takes_the_locator_rule_its_descriptor_states() {
        use cargo_nano_ros::rmw_resolver::{RmwIndex, test_support};
        for (peer, want) in [
            ("none", Some(HostPeer::None)),
            ("zenoh-router", Some(HostPeer::ZenohRouter)),
        ] {
            let ws = tempfile::tempdir().unwrap();
            let acme = ws.path().join("src/acme_rmw");
            test_support::write_acme_provider(&acme);
            let desc = acme.join("nros-rmw.toml");
            let mut text = std::fs::read_to_string(&desc).unwrap();
            text.push_str(&format!("\n[rmw.host]\npeer = \"{peer}\"\n"));
            std::fs::write(&desc, text).unwrap();
            let roots = vec![
                test_support::checkout().join("packages/rmw"),
                ws.path().to_path_buf(),
            ];
            let scan = cargo_nano_ros::provider_scan::scan_roots(&roots).unwrap();
            let index = RmwIndex::new(scan, roots);
            let got = host_peer_of(&index, "acme");
            assert_eq!(got, want, "peer = {peer}");
            let (nets, ifs, r) = parse(TAP, "a");
            let out = resolve(
                &ImageDecl {
                    system: Some("sys"),
                    image: "a",
                    networks: &nets,
                    ifs: &ifs,
                    run: &r,
                    locator: None,
                    host_peer: got,
                },
                &zephyr_board(),
            );
            match peer {
                "none" => assert!(out.unwrap().is_some()),
                _ => assert!(out.unwrap_err().contains("states no `locator`")),
            }
        }
    }
}
