//! RFC-0103 D10f (phase-497 W2) — an image's resolved network, as ONE line of
//! text every carrier reads.
//!
//! `nros sync` / `nros build` resolve an image's `[image.<id>.net.<if>]`
//! against its project's `[network.<name>]` tables and its board's
//! `[board.net]`, write the answer to `<build>/nros/net/<image>.toml`, and hand
//! it to the build as `NROS_IMAGE_NET` — a cargo `[env]` row, a cmake cache
//! variable. The consumers are:
//!
//! * `nros::main!`, which bakes the text into the image's
//!   `nros_platform::DeployOverlay::net`; the board applies it at boot with
//!   [`primary`];
//! * a board crate that boots a C/C++ app from its own `main.rs`
//!   (`nros_board_mps2_an385::Config::from_image`), through `option_env!`;
//! * `nros board net <board> --image-net <text>`, which renders the C
//!   `nros_app_config_t.network` initializer the cmake road compiles.
//!
//! So the text is the one CARRIER, and this module is its one reader and its
//! one writer. The decoder is `no_std` and allocation-free, because a board
//! decodes at boot; the encoder needs `alloc`, so it is behind
//! `build-helpers`, which the CLI enables.
//!
//! Format — interfaces separated by `|`, each eight `,`-separated fields:
//!
//! ```text
//! <if>,<kind>,<network>,<address>,<prefix>,<gateway>,<mac>,<attach>
//! eth0,ethernet,qemu,10.0.2.50,24,10.0.2.2,02:00:00:4f:1c:a2,slirp
//! ```
//!
//! `<address>` is dotted IPv4 or `dhcp`; `<gateway>` is dotted IPv4 or `-`
//! (the network states none); `<mac>` is six hex bytes; `<attach>` is how an
//! EMULATED image joins the network (D10a, phase-497 W3) — `slirp`,
//! `bridge=<dev>`, `tap=<dev>`, or `-` for a physical-only network. Firmware
//! ignores `<attach>`; a QEMU launcher renders it with [`qemu_nic`]. A board that cannot
//! take a leased address never sees `dhcp`: the resolver refuses it for a
//! board whose interface does not declare a DHCP client.

/// The variable the text travels in, on every road.
pub const ENV: &str = "NROS_IMAGE_NET";

/// An interface's address: static, or leased by DHCP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Address {
    /// A static IPv4 address.
    Static([u8; 4]),
    /// A DHCP lease.
    Dhcp,
}

/// One resolved interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetIf<'a> {
    /// The board's interface name (`eth0`).
    pub name: &'a str,
    /// Its kind: `ethernet` or `wifi` (only those take an address).
    pub kind: &'a str,
    /// The network it sits on (`qemu`, or a project `[network.<name>]`).
    pub network: &'a str,
    /// Its address.
    pub address: Address,
    /// The network's prefix length.
    pub prefix: u8,
    /// The network's gateway, if it states one.
    pub gateway: Option<[u8; 4]>,
    /// The interface's MAC: stated by the image, else derived.
    pub mac: [u8; 6],
    /// How an emulated image joins the network.
    pub attach: Attach<'a>,
}

/// `[network.<name>] attach` (D10a): how an EMULATED image joins a network.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attach<'a> {
    /// QEMU's user-mode network, private to the VM.
    Slirp,
    /// A host bridge device (`-nic bridge,br=<dev>`).
    Bridge(&'a str),
    /// A host TAP device (`-nic tap,ifname=<dev>`).
    Tap(&'a str),
    /// None: a physical-only network, which no emulated image can join.
    None,
}

impl NetIf<'_> {
    /// The netmask of [`Self::prefix`].
    #[must_use]
    pub const fn netmask(&self) -> [u8; 4] {
        crate::netmask_from_prefix(self.prefix)
    }

    /// The static address, or `None` for a lease.
    #[must_use]
    pub const fn ip(&self) -> Option<[u8; 4]> {
        match self.address {
            Address::Static(ip) => Some(ip),
            Address::Dhcp => None,
        }
    }
}

/// Why a text failed to decode. The text is written by the CLI, so this is a
/// mismatch between the CLI and this crate, never a user's typo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeError(pub &'static str);

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{ENV}: {}", self.0)
    }
}

fn parse_u8(s: &str, radix: u32) -> Option<u8> {
    if s.is_empty() || s.len() > 3 {
        return None;
    }
    u8::from_str_radix(s, radix).ok()
}

fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut it = s.split('.');
    for o in &mut out {
        *o = parse_u8(it.next()?, 10)?;
    }
    it.next().is_none().then_some(out)
}

fn parse_mac(s: &str) -> Option<[u8; 6]> {
    let mut out = [0u8; 6];
    let mut it = s.split(':');
    for b in &mut out {
        let p = it.next()?;
        if p.len() != 2 {
            return None;
        }
        *b = parse_u8(p, 16)?;
    }
    it.next().is_none().then_some(out)
}

fn decode_one(s: &str) -> Result<NetIf<'_>, DecodeError> {
    let mut f = s.split(',');
    let mut next = |what: &'static str| f.next().ok_or(DecodeError(what));
    let name = next("an interface has no name")?;
    let kind = next("an interface has no kind")?;
    let network = next("an interface has no network")?;
    let address = match next("an interface has no address")? {
        "dhcp" => Address::Dhcp,
        a => Address::Static(parse_ipv4(a).ok_or(DecodeError("an address is not IPv4"))?),
    };
    let prefix = parse_u8(next("an interface has no prefix")?, 10)
        .filter(|p| *p <= 32)
        .ok_or(DecodeError("a prefix is not 0..=32"))?;
    let gateway = match next("an interface has no gateway field")? {
        "-" => None,
        g => Some(parse_ipv4(g).ok_or(DecodeError("a gateway is not IPv4"))?),
    };
    let mac = parse_mac(next("an interface has no MAC")?)
        .ok_or(DecodeError("a MAC is not six hex bytes"))?;
    let attach = match next("an interface has no attach field")? {
        "slirp" => Attach::Slirp,
        "-" => Attach::None,
        a => match a.split_once('=') {
            Some(("bridge", d)) if !d.is_empty() => Attach::Bridge(d),
            Some(("tap", d)) if !d.is_empty() => Attach::Tap(d),
            _ => {
                return Err(DecodeError(
                    "an attach is not slirp, bridge=<dev>, tap=<dev> or -",
                ));
            }
        },
    };
    if f.next().is_some() {
        return Err(DecodeError("an interface has more than eight fields"));
    }
    if name.is_empty() || kind.is_empty() || network.is_empty() {
        return Err(DecodeError(
            "an interface has an empty name, kind or network",
        ));
    }
    Ok(NetIf {
        name,
        kind,
        network,
        address,
        prefix,
        gateway,
        mac,
        attach,
    })
}

/// Every interface in `text`, in order.
pub fn interfaces(text: &str) -> impl Iterator<Item = Result<NetIf<'_>, DecodeError>> {
    text.split('|').filter(|s| !s.is_empty()).map(decode_one)
}

/// The image's primary interface — the first, which is the only one a
/// single-NIC board applies. `Ok(None)` for an empty text.
pub fn primary(text: &str) -> Result<Option<NetIf<'_>>, DecodeError> {
    interfaces(text).next().transpose()
}

/// The interface named `name`.
pub fn interface<'a>(text: &'a str, name: &str) -> Result<Option<NetIf<'a>>, DecodeError> {
    for i in interfaces(text) {
        let i = i?;
        if i.name == name {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

/// The primary interface a board applies at boot, from the overlay's
/// `net` text (`nros_platform::DeployOverlay::net`). `None` when the image
/// states no interface.
///
/// # Panics
///
/// On a text this crate cannot decode. The text is the CLI's, so that is a
/// CLI/board version mismatch; booting the board default instead would put
/// the image on an address nobody stated, which is the failure D10 removes.
#[must_use]
pub fn boot_interface(text: Option<&'static str>) -> Option<NetIf<'static>> {
    match primary(text?) {
        Ok(i) => i,
        Err(e) => panic!("{e}"),
    }
}

/// Encode `ifs` — the CLI's half. Interfaces keep the order given.
#[cfg(feature = "build-helpers")]
#[must_use]
pub fn encode(ifs: &[NetIf<'_>]) -> String {
    let dotted = |o: [u8; 4]| format!("{}.{}.{}.{}", o[0], o[1], o[2], o[3]);
    ifs.iter()
        .map(|i| {
            let addr = match i.address {
                Address::Static(ip) => dotted(ip),
                Address::Dhcp => "dhcp".to_string(),
            };
            let gw = i.gateway.map_or_else(|| "-".to_string(), dotted);
            let attach = match i.attach {
                Attach::Slirp => "slirp".to_string(),
                Attach::Bridge(d) => format!("bridge={d}"),
                Attach::Tap(d) => format!("tap={d}"),
                Attach::None => "-".to_string(),
            };
            format!(
                "{},{},{},{addr},{},{gw},{},{attach}",
                i.name,
                i.kind,
                i.network,
                i.prefix,
                format_mac(i.mac)
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// QEMU's `-nic` value for a guest with no image network: its own user-mode
/// (SLIRP) network, which is the built-in `qemu` network (RFC-0103 D10).
/// `model` is the board's NIC model (`lan9118`, `open_eth`,
/// `virtio-net-device`). One spelling for every launcher.
#[cfg(feature = "build-helpers")]
#[must_use]
pub fn slirp_nic(model: &str) -> String {
    format!("user,model={model}")
}

/// QEMU's `-nic` value for interface `i` of an emulated image (D10a,
/// phase-497 W3): its network's `attach`, the board's NIC `model`, and the
/// image's MAC. A user-mode network off QEMU's own `10.0.2.0/24` passes its
/// subnet and gateway (`net=` / `host=`), so the guest's static address is
/// on the network QEMU makes.
///
/// # Errors
///
/// A network with no `attach` is physical-only: an emulated image cannot
/// join it, and launching one anyway would put the guest on no network.
#[cfg(feature = "build-helpers")]
pub fn qemu_nic(i: &NetIf<'_>, model: &str) -> Result<String, String> {
    let mac = format_mac(i.mac);
    let dotted = |o: [u8; 4]| format!("{}.{}.{}.{}", o[0], o[1], o[2], o[3]);
    match i.attach {
        Attach::Slirp => {
            let mut v = format!("user,model={model},mac={mac}");
            let base = match i.address {
                Address::Static(ip) => {
                    let m = u32::from_be_bytes(i.netmask());
                    Some((u32::from_be_bytes(ip) & m).to_be_bytes())
                }
                Address::Dhcp => None,
            };
            if i.network != "qemu"
                && let Some(base) = base
            {
                v.push_str(&format!(",net={}/{}", dotted(base), i.prefix));
                if let Some(g) = i.gateway {
                    v.push_str(&format!(",host={}", dotted(g)));
                }
            }
            Ok(v)
        }
        Attach::Bridge(dev) => Ok(format!("bridge,br={dev},model={model},mac={mac}")),
        Attach::Tap(dev) => Ok(format!(
            "tap,ifname={dev},script=no,downscript=no,model={model},mac={mac}"
        )),
        Attach::None => Err(format!(
            "interface `{}` sits on network `{}`, which states no `attach` — it is a \
             physical-only network, and an emulated image cannot join it (RFC-0103 D10a). \
             Add `attach = \"slirp\"`, `{{ bridge = \"<dev>\" }}` or `{{ tap = \"<dev>\" }}` to \
             `[network.{}]`",
            i.name, i.network, i.network
        )),
    }
}

/// `02:00:00:4f:1c:a2`.
#[cfg(feature = "build-helpers")]
#[must_use]
pub fn format_mac(mac: [u8; 6]) -> String {
    mac.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "build-helpers")]
    const ETH0: NetIf<'static> = NetIf {
        name: "eth0",
        kind: "ethernet",
        network: "qemu",
        address: Address::Static([10, 0, 2, 50]),
        prefix: 24,
        gateway: Some([10, 0, 2, 2]),
        mac: [0x02, 0x00, 0x00, 0x4f, 0x1c, 0xa2],
        attach: Attach::Slirp,
    };

    #[cfg(feature = "build-helpers")]
    #[test]
    fn a_text_round_trips_every_field() {
        let eth1 = NetIf {
            name: "eth1",
            network: "lab",
            address: Address::Dhcp,
            prefix: 16,
            gateway: None,
            mac: [0x52, 0x54, 0, 0, 0, 1],
            attach: Attach::Bridge("br0"),
            ..ETH0
        };
        let text = encode(&[ETH0, eth1]);
        assert_eq!(
            text,
            "eth0,ethernet,qemu,10.0.2.50,24,10.0.2.2,02:00:00:4f:1c:a2,slirp|\
             eth1,ethernet,lab,dhcp,16,-,52:54:00:00:00:01,bridge=br0"
        );
        let back: Vec<_> = interfaces(&text).collect::<Result<_, _>>().unwrap();
        assert_eq!(back, [ETH0, eth1]);
        assert_eq!(primary(&text).unwrap(), Some(ETH0));
        assert_eq!(interface(&text, "eth1").unwrap(), Some(eth1));
        assert_eq!(back[0].netmask(), [255, 255, 255, 0]);
        assert_eq!(back[1].ip(), None);
    }

    /// phase-497 W3 — the launcher's `-nic` comes from the network's
    /// `attach`, and a physical-only network refuses an emulated image.
    #[cfg(feature = "build-helpers")]
    #[test]
    fn qemu_nic_renders_the_attach_and_refuses_a_physical_network() {
        assert_eq!(
            qemu_nic(&ETH0, "lan9118").unwrap(),
            "user,model=lan9118,mac=02:00:00:4f:1c:a2"
        );
        let lan = NetIf {
            network: "lan",
            address: Address::Static([192, 168, 1, 20]),
            gateway: Some([192, 168, 1, 1]),
            ..ETH0
        };
        assert_eq!(
            qemu_nic(&lan, "lan9118").unwrap(),
            "user,model=lan9118,mac=02:00:00:4f:1c:a2,net=192.168.1.0/24,host=192.168.1.1"
        );
        let br = NetIf {
            attach: Attach::Bridge("qemu-br"),
            ..lan
        };
        assert_eq!(
            qemu_nic(&br, "open_eth").unwrap(),
            "bridge,br=qemu-br,model=open_eth,mac=02:00:00:4f:1c:a2"
        );
        let tap = NetIf {
            attach: Attach::Tap("tap0"),
            ..lan
        };
        assert!(
            qemu_nic(&tap, "lan9118")
                .unwrap()
                .starts_with("tap,ifname=tap0,script=no,downscript=no,")
        );
        let phys = NetIf {
            attach: Attach::None,
            ..lan
        };
        assert!(qemu_nic(&phys, "lan9118").unwrap_err().contains("D10a"));
        assert_eq!(slirp_nic("lan9118"), "user,model=lan9118");
    }

    #[test]
    fn an_empty_text_has_no_interface() {
        assert_eq!(primary("").unwrap(), None);
    }

    #[test]
    fn a_malformed_text_is_an_error_not_a_default() {
        for bad in [
            "eth0,ethernet,qemu,10.0.2,24,-,02:00:00:00:00:01,slirp",
            "eth0,ethernet,qemu,10.0.2.5,33,-,02:00:00:00:00:01,slirp",
            "eth0,ethernet,qemu,10.0.2.5,24,-,02:00:00:00:01,slirp",
            "eth0,ethernet,qemu,10.0.2.5,24,-,02:00:00:00:00:01",
            "eth0,ethernet,qemu,10.0.2.5,24,-,02:00:00:00:00:01,wifi",
            "eth0,ethernet,qemu,10.0.2.5,24,-,02:00:00:00:00:01,bridge=",
            "eth0,ethernet,qemu,10.0.2.5,24,-,02:00:00:00:00:01,slirp,x",
            ",ethernet,qemu,10.0.2.5,24,-,02:00:00:00:00:01,slirp",
        ] {
            assert!(primary(bad).is_err(), "{bad}");
        }
    }
}
