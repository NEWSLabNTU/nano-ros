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
//!   project's (D10d).
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
        let mut out = format!(
            "# Generated by nros (RFC-0103 D10f, phase-497 W2) — the resolved network of\n\
             # image `{image}`. Do not edit: state an address in the board network\n\
             # file its `os_config` names (RFC-0109 D4), and how it is attached in\n\
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
    // D10d — off QEMU's network the gateway is rarely the router, so the
    // image must name one somewhere.
    if let Some(first) = interfaces.first()
        && first.network != QEMU_NETWORK
        && d.locator.is_none()
    {
        return Err(format!(
            "image `{}` sits on network {:?}, off QEMU's own, and states no `locator` \
             (RFC-0109 D6): add `locator = \"tcp/<router>:7447\"` to `[image.{}]` or \
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
/// board's `[board.net]`; it is asked only for an image that states an
/// interface. The map holds the images that do.
pub fn resolve_system(
    sys: &super::cargo_metadata_schema::SystemToml,
    mut board_net_of: impl FnMut(&str) -> Result<BoardNet, String>,
) -> Result<BTreeMap<String, ImageNet>, String> {
    let mut out = BTreeMap::new();
    // One network has one subnet across the project, whichever image's file
    // states it.
    let mut networks = BTreeMap::new();
    for (id, img) in sys.resolved_images() {
        if img.net.is_empty() {
            continue;
        }
        system_net::merge_networks(
            &mut networks,
            img.networks.clone(),
            &format!("image `{id}`"),
        )?;
        let locator = img.locator.clone().or_else(|| sys.system.locator.clone());
        let board = board_net_of(&id)?;
        if let Some(n) = resolve(
            &ImageDecl {
                system: Some(&sys.system.name),
                image: &id,
                networks: &img.networks,
                ifs: &img.net,
                run: &img.run,
                locator: locator.as_deref(),
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
    resolve(
        &ImageDecl {
            system: n.system.as_deref(),
            image,
            networks: &n.networks,
            ifs: &n.net,
            run: &n.run,
            locator: n.locator.as_deref(),
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
                .contains("D10e")
        );
    }
}
