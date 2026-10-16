//! phase-484 W4b (RFC-0103 D1) — a board's network identity defaults, from its
//! own descriptor, for its build script.
//!
//! A board's fallback IP / netmask / gateway / MAC used to be written into the
//! board crate's `Config::default()`, its build script's C `NROS_APP_CONFIG`
//! emitter, and a cmake board module — three copies of one fact. Each copy is
//! now generated from `[board.net]` in the board's `nros-board.toml`
//! (`nros_platform_config::BoardNet`): a board on the `qemu` network gets that
//! network's plan, any other the platform default, [`crate::BaseConfig::default`].
//! An image's own address is applied over it ([`ResolvedNet::with_interface`],
//! phase-497 W2), never stated by the board.
//!
//! Build-script only: it reads files and writes `OUT_DIR`.

use std::path::{Path, PathBuf};

use crate::platform_config::BoardKnobsFile;

/// The resolved identity: the board's `[board.net]` over the platform default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedNet {
    pub ip: [u8; 4],
    pub netmask: [u8; 4],
    pub gateway: [u8; 4],
    pub mac: [u8; 6],
}

impl ResolvedNet {
    /// The platform default, before any board states anything.
    #[must_use]
    pub fn platform_default() -> Self {
        let b = crate::BaseConfig::default();
        Self {
            ip: b.ip,
            netmask: b.netmask,
            gateway: b.gateway,
            mac: b.mac,
        }
    }

    /// The built-in QEMU network's defaults (RFC-0103 D10): QEMU's own SLIRP
    /// plan — `10.0.2.15/24`, gateway `10.0.2.2` — with the platform's MAC.
    #[must_use]
    pub fn qemu_network() -> Self {
        Self {
            ip: [10, 0, 2, 15],
            netmask: [255, 255, 255, 0],
            gateway: [10, 0, 2, 2],
            mac: Self::platform_default().mac,
        }
    }

    /// The prefix length of [`Self::netmask`].
    #[must_use]
    pub fn prefix(&self) -> u8 {
        crate::prefix_from_netmask(self.netmask)
    }

    /// The `.network = { … }` body of a C `nros_app_config_t` initializer.
    #[must_use]
    pub fn c_network_initializer(&self) -> String {
        let q = |o: &[u8]| o.iter().map(u8::to_string).collect::<Vec<_>>().join(", ");
        let mac = self
            .mac
            .iter()
            .map(|b| format!("0x{b:02x}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{{\n            .ip = {{{}}},\n            .mac = {{{mac}}},\n            \
             .gateway = {{{}}},\n            .netmask = {{{}}},\n            .prefix = {},\n        }}",
            q(&self.ip),
            q(&self.gateway),
            q(&self.netmask),
            self.prefix()
        )
    }
}

/// `[board.net]` of the board `descriptor` describes, over `fallback`.
///
/// `board` selects the entry when the file declares several (`NROS_BOARD`, the
/// carrier `nros ws board-facts` writes). A descriptor with no `[board.net]`
/// leaves `fallback` untouched. A malformed value is an error naming the key.
pub fn resolve(
    descriptor: &Path,
    board: Option<&str>,
    fallback: ResolvedNet,
) -> Result<ResolvedNet, String> {
    if !descriptor.is_file() {
        return Ok(fallback);
    }
    let file = BoardKnobsFile::load_for_board(descriptor, board).map_err(|e| e.to_string())?;
    let Some(net) = file.board_net(board)? else {
        return Ok(fallback);
    };
    // RFC-0103 D10 — a board on the QEMU network starts from that network's
    // plan. Nothing else here is the board's: an address is the IMAGE's
    // (`NROS_IMAGE_NET`, phase-497 W2), and the board-level `ip` / `mac` keys
    // that used to override this are refused by `board_net` (D10g).
    Ok(match net.network()? {
        Some(_) => ResolvedNet::qemu_network(),
        None => fallback,
    })
}

impl ResolvedNet {
    /// This default with an image's interface applied (RFC-0103 D10f): its
    /// address, prefix, gateway and MAC. A lease keeps the default address —
    /// the board's DHCP client replaces it at run time.
    #[must_use]
    pub fn with_interface(mut self, i: &crate::image_net::NetIf<'_>) -> Self {
        if let Some(ip) = i.ip() {
            self.ip = ip;
        }
        self.netmask = i.netmask();
        if let Some(gw) = i.gateway {
            self.gateway = gw;
        }
        self.mac = i.mac;
        self
    }
}

/// For a board crate's `build.rs`: resolve this crate's own descriptor
/// (`$CARGO_MANIFEST_DIR/nros-board.toml`) over `fallback`, watch it, and write
/// `$OUT_DIR/nros_board_net.rs` for `include!` — four `BOARD_NET_*` constants
/// the crate's `Config::default()` reads. Panics with the descriptor's error:
/// the right failure in a build script.
pub fn emit_for_crate(fallback: ResolvedNet) -> ResolvedNet {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    emit_from(&manifest.join("nros-board.toml"), fallback)
}

/// For a FAMILY crate that serves several boards (`nros-board-freertos`): the
/// descriptor of the board THIS build is for, which the build names through
/// `NROS_BOARD_TOML` (`nros ws board-facts` / the generated cargo config).
/// Absent, the build named no board and `fallback` stands — a workspace-wide
/// `cargo check`, which is what the crate's `Config::default()` always was.
/// The path is watched by CONTENT, never as env text (issue 0491).
pub fn emit_for_named_board(fallback: ResolvedNet) -> ResolvedNet {
    let descriptor = crate::nros_build_paths::env_path("NROS_BOARD_TOML")
        .map(|p| crate::nros_build_paths::watch_path(&p))
        .unwrap_or_default();
    emit_from(&descriptor, fallback)
}

/// For a build script whose C half BAKES the address into a TU the board's
/// own startup applies (riscv64's `NROS_APP_CONFIG`, read by `startup.c`
/// before the kernel runs): `net` with the image's resolved network over it,
/// from `NROS_IMAGE_NET` (RFC-0103 D10f). On every other board the image's
/// address arrives through `nros::main!`'s deploy overlay instead, so their
/// build scripts do not read the variable and stay one compile per board.
///
/// # Panics
///
/// On a text this crate cannot decode — a CLI/board version mismatch.
pub fn with_image_env(net: ResolvedNet) -> ResolvedNet {
    println!("cargo:rerun-if-env-changed={}", crate::image_net::ENV);
    let Ok(text) = std::env::var(crate::image_net::ENV) else {
        return net;
    };
    match crate::image_net::primary(&text) {
        Ok(Some(i)) => net.with_interface(&i),
        Ok(None) => net,
        Err(e) => panic!("{e}"),
    }
}

fn emit_from(descriptor: &Path, fallback: ResolvedNet) -> ResolvedNet {
    if !descriptor.as_os_str().is_empty() {
        println!("cargo:rerun-if-changed={}", descriptor.display());
    }
    println!("cargo:rerun-if-env-changed=NROS_BOARD");
    let board = std::env::var("NROS_BOARD").ok().filter(|b| !b.is_empty());
    let net = resolve(descriptor, board.as_deref(), fallback)
        .unwrap_or_else(|e| panic!("{}: {e}", descriptor.display()));
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("nros_board_net.rs");
    let body = format!(
        "// Generated from {} [board.net] (phase-484 W4b). Do not edit.\n\
         #[allow(dead_code)]\n\
         pub(crate) const BOARD_NET_IP: [u8; 4] = {:?};\n\
         #[allow(dead_code)]\n\
         pub(crate) const BOARD_NET_NETMASK: [u8; 4] = {:?};\n\
         #[allow(dead_code)]\n\
         pub(crate) const BOARD_NET_GATEWAY: [u8; 4] = {:?};\n\
         #[allow(dead_code)]\n\
         pub(crate) const BOARD_NET_MAC: [u8; 6] = {:?};\n\
         /// The board's router default: its gateway (RFC-0103 D10).\n\
         #[allow(dead_code)]\n\
         pub(crate) const BOARD_NET_LOCATOR: &str = \"tcp/{}.{}.{}.{}:7447\";\n\
         /// `tcp/<the board's gateway>:<port>` for a board whose router\n\
         /// listens elsewhere (`board_net_locator!(\"7448\")`).\n\
         #[allow(unused_macros)]\n\
         macro_rules! board_net_locator {{ ($port:literal) => {{ concat!(\"tcp/{}.{}.{}.{}:\", $port) }}; }}\n",
        descriptor.display(),
        net.ip,
        net.netmask,
        net.gateway,
        net.mac,
        net.gateway[0],
        net.gateway[1],
        net.gateway[2],
        net.gateway[3],
        net.gateway[0],
        net.gateway[1],
        net.gateway[2],
        net.gateway[3]
    );
    std::fs::write(&out, body).unwrap_or_else(|e| panic!("write {}: {e}", out.display()));
    net
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC-0103 D10 — `network = "qemu"` starts from QEMU's SLIRP plan, an
    /// unknown network is refused, and the board-level identity is retired.
    #[test]
    fn a_board_on_the_qemu_network_starts_from_its_plan() {
        let d = std::env::temp_dir().join(format!("nros-board-net-q-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("nros-board.toml");
        std::fs::write(
            &f,
            "[[board]]\nnames = [\"b\"]\n\n[board.net]\nnetwork = \"qemu\"\n",
        )
        .unwrap();
        let got = resolve(&f, None, ResolvedNet::platform_default()).unwrap();
        assert_eq!(got.ip, [10, 0, 2, 15]);
        assert_eq!(got.gateway, [10, 0, 2, 2]);
        assert_eq!(got.mac, ResolvedNet::platform_default().mac);
        for (body, want) in [
            ("network = \"lab\"\n", "phase-497"),
            ("ip = \"10.0.2.40\"\n", "D10g"),
            ("ipp = \"1.2.3.4\"\n", "ipp"),
        ] {
            std::fs::write(
                &f,
                format!("[[board]]\nnames = [\"b\"]\n\n[board.net]\n{body}"),
            )
            .unwrap();
            let e = resolve(&f, None, ResolvedNet::platform_default()).unwrap_err();
            assert!(e.contains(want), "{body}: {e}");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_image_interface_replaces_the_board_default() {
        let i = crate::image_net::primary(
            "eth0,ethernet,qemu,10.0.2.50,24,10.0.2.2,02:00:00:01:02:03,slirp",
        )
        .unwrap()
        .unwrap();
        let got = ResolvedNet::qemu_network().with_interface(&i);
        assert_eq!(got.ip, [10, 0, 2, 50]);
        assert_eq!(got.mac, [2, 0, 0, 1, 2, 3]);
        assert_eq!(got.netmask, [255, 255, 255, 0]);
    }

    #[test]
    fn the_c_initializer_spells_every_field() {
        let s = ResolvedNet::platform_default().c_network_initializer();
        assert!(
            s.contains(".ip = {192, 0, 3, 10}") && s.contains(".prefix = 24"),
            "{s}"
        );
        assert!(
            s.contains(".mac = {0x02, 0x00, 0x00, 0x00, 0x00, 0x00}"),
            "{s}"
        );
    }
}
