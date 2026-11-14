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

use crate::platform_config::{BoardKnobsFile, StackOwner};

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

    /// `tcp/<gateway>:7447`: the router default of a board that leaves it to
    /// its network ([`default_locator`]).
    #[must_use]
    pub fn gateway_locator(&self) -> String {
        let g = self.gateway;
        format!(
            "tcp/{}.{}.{}.{}:{DEFAULT_ROUTER_PORT}",
            g[0], g[1], g[2], g[3]
        )
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
    // Issue 2064 — an `"os"` board's address is its kernel's: read the
    // configuration it names, the same file its build script reads.
    if let Some(rel) = net.kernel_config.as_deref() {
        let path = descriptor.parent().unwrap_or(Path::new(".")).join(rel);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("[board.net] kernel_config {}: {e}", path.display()))?;
        return from_kconfig(&text, fallback).map_err(|e| format!("{}: {e}", path.display()));
    }
    // RFC-0103 D10 — a board on the QEMU network starts from that network's
    // plan. Nothing else here is the board's: an address is the IMAGE's
    // (`NROS_IMAGE_NET`, phase-497 W2), and the board-level `ip` / `mac` keys
    // that used to override this are refused by `board_net` (D10g).
    Ok(match net.network()? {
        Some(_) => ResolvedNet::qemu_network(),
        None => fallback,
    })
}

/// The port a board-derived router default dials: the one a stock
/// `rmw_zenohd` listens on.
pub const DEFAULT_ROUTER_PORT: u16 = 7447;

/// The router an image on this board dials when nothing states one: no
/// `locator` in its `system.toml`, no `NROS_ENTRY_LOCATOR`, no `$NROS_LOCATOR`.
///
/// A locator is the image's selection (RFC-0103 D1); this is only its
/// fallback, and it is DERIVED from the board's network facts rather than
/// stated by the board (RFC-0109, which absorbed RFC-0103 D10):
///
/// - `stack = "host"`: `tcp/127.0.0.1:7447`. The image is a host process
///   (linux, freertos-posix, threadx-linux's NSOS), and loopback is the one
///   address it reaches without root-provisioned interfaces (issue 1793).
/// - `stack = "os"` with no `kernel_config` (every Zephyr board): `None`. The
///   RTOS's own configuration owns the default (`CONFIG_NROS_ZENOH_LOCATOR`,
///   which a board package's `prj.conf` may set), and nothing here reads it.
/// - otherwise: `tcp/<the board's resolved gateway>:7447` ([`resolve`]): QEMU's
///   `10.0.2.2` for `network = "qemu"`, the kernel's `CONFIG_NETINIT_DRIPADDR`
///   for a NuttX board, the platform default for a board stating neither.
///
/// ONE rule, read by the board crates (`BOARD_NET_LOCATOR` and their C
/// `NROS_APP_CONFIG` emitters), by `nros dev board net` (so the cmake entry-locator
/// rung and the ThreadX cmake board modules) and by `nros run`'s planner, whose
/// `link` it is when the image states none.
///
/// # Errors
///
/// The descriptor's `[board.net]` is malformed.
pub fn default_locator(descriptor: &Path, board: Option<&str>) -> Result<Option<String>, String> {
    if !descriptor.is_file() {
        return Ok(None);
    }
    let net = resolve(descriptor, board, ResolvedNet::platform_default())?;
    default_locator_over(descriptor, board, &net)
}

/// [`default_locator`] over a network already resolved for the image, whose
/// own interface may state a gateway of its own (`--image-net`, RFC-0103
/// D10f): the gateway form reads `net`, not the board's bare default.
///
/// # Errors
///
/// As [`default_locator`].
pub fn default_locator_over(
    descriptor: &Path,
    board: Option<&str>,
    net: &ResolvedNet,
) -> Result<Option<String>, String> {
    if !descriptor.is_file() {
        return Ok(None);
    }
    let file = BoardKnobsFile::load_for_board(descriptor, board).map_err(|e| e.to_string())?;
    if let Some(n) = file.board_net(board)? {
        match n.stack_owner()? {
            Some(StackOwner::Host) => {
                return Ok(Some(format!("tcp/127.0.0.1:{DEFAULT_ROUTER_PORT}")));
            }
            Some(StackOwner::Os) if n.kernel_config.is_none() => return Ok(None),
            _ => {}
        }
    }
    Ok(Some(net.gateway_locator()))
}

/// [`default_locator_over`] `net`, for a board crate's build script: this
/// crate's own descriptor (`$CARGO_MANIFEST_DIR/nros-board.toml`) and the board
/// the build names (`NROS_BOARD`), as [`emit_for_crate`] reads them. The
/// crate's C `NROS_APP_CONFIG` emitter bakes it beside `net`'s `.network`
/// block, so the Rust and C roads of one board fall back to one router.
/// Panics with the descriptor's error: the right failure in a build script.
#[must_use]
pub fn crate_locator(net: ResolvedNet) -> String {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let board = std::env::var("NROS_BOARD").ok().filter(|b| !b.is_empty());
    locator_for(&manifest.join("nros-board.toml"), board.as_deref(), net)
}

/// [`default_locator_over`], or `net`'s gateway form where the board leaves
/// the default to its RTOS: a board crate's `BOARD_NET_LOCATOR` always has a
/// value.
fn locator_for(descriptor: &Path, board: Option<&str>, net: ResolvedNet) -> String {
    default_locator_over(descriptor, board, &net)
        .unwrap_or_else(|e| panic!("{}: {e}", descriptor.display()))
        .unwrap_or_else(|| net.gateway_locator())
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
    let locator = locator_for(descriptor, board.as_deref(), net);
    write_consts(
        &format!("{} [board.net]", descriptor.display()),
        net,
        &locator,
    );
    net
}

/// RFC-0109 D2 (phase-514 W4) — a board whose RTOS owns its network
/// (`[board.net] stack = "os"`) reads the address its kernel configuration
/// states rather than restating one: `CONFIG_NETINIT_IPADDR` /
/// `CONFIG_NETINIT_DRIPADDR` / `CONFIG_NETINIT_NETMASK` of a NuttX `.config` or
/// `defconfig` (hex, `0x0a00020f` = `10.0.2.15`). A key the file does not set
/// keeps `fallback`'s value, as NuttX's own Kconfig default would; the MAC is
/// always `fallback`'s, since the RTOS takes its MAC from the device.
///
/// # Errors
///
/// A key that is set but is not a 32-bit hex value.
pub fn from_kconfig(text: &str, fallback: ResolvedNet) -> Result<ResolvedNet, String> {
    let mut net = fallback;
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        let slot = match key {
            "CONFIG_NETINIT_IPADDR" => &mut net.ip,
            "CONFIG_NETINIT_DRIPADDR" => &mut net.gateway,
            "CONFIG_NETINIT_NETMASK" => &mut net.netmask,
            _ => continue,
        };
        let v = value
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .ok_or_else(|| format!("{key}={value} is not a 32-bit hex value (0x0a00020f)"))?;
        *slot = v.to_be_bytes();
    }
    Ok(net)
}

/// For an `"os"`-stack board's build script: [`from_kconfig`] over the
/// kernel configuration at `config`, watched, written to
/// `$OUT_DIR/nros_board_net.rs` the way [`emit_for_crate`] writes it. Panics
/// with the file's error: the right failure in a build script.
pub fn emit_from_kconfig(config: &Path, fallback: ResolvedNet) -> ResolvedNet {
    println!("cargo:rerun-if-changed={}", config.display());
    let text = std::fs::read_to_string(config)
        .unwrap_or_else(|e| panic!("read {}: {e}", config.display()));
    let net = from_kconfig(&text, fallback).unwrap_or_else(|e| panic!("{}: {e}", config.display()));
    write_consts(&config.display().to_string(), net, &net.gateway_locator());
    net
}

fn write_consts(source: &str, net: ResolvedNet, locator: &str) {
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("nros_board_net.rs");
    // phase-484 W4b, RFC-0109 D2.
    let body = format!(
        "// Generated from {}. Do not edit.\n\
         #[allow(dead_code)]\n\
         pub(crate) const BOARD_NET_IP: [u8; 4] = {:?};\n\
         #[allow(dead_code)]\n\
         pub(crate) const BOARD_NET_NETMASK: [u8; 4] = {:?};\n\
         #[allow(dead_code)]\n\
         pub(crate) const BOARD_NET_GATEWAY: [u8; 4] = {:?};\n\
         #[allow(dead_code)]\n\
         pub(crate) const BOARD_NET_MAC: [u8; 6] = {:?};\n\
         /// The board's router default (`board_net::default_locator`).\n\
         #[allow(dead_code)]\n\
         pub(crate) const BOARD_NET_LOCATOR: &str = {:?};\n\
         /// `tcp/<the board's gateway>:<port>` for a board whose router\n\
         /// listens elsewhere (`board_net_locator!(\"7448\")`).\n\
         #[allow(unused_macros)]\n\
         macro_rules! board_net_locator {{ ($port:literal) => {{ concat!(\"tcp/{}.{}.{}.{}:\", $port) }}; }}\n",
        source,
        net.ip,
        net.netmask,
        net.gateway,
        net.mac,
        locator,
        net.gateway[0],
        net.gateway[1],
        net.gateway[2],
        net.gateway[3]
    );
    std::fs::write(&out, body).unwrap_or_else(|e| panic!("write {}: {e}", out.display()));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The router default each in-tree board's images fall back to, through
    /// the one rule: loopback for a host stack, the gateway otherwise, and
    /// nothing where the RTOS's own configuration owns it (Zephyr).
    #[test]
    fn each_board_derives_its_router_default() {
        let boards = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        for (pkg, board, want) in [
            (
                "nros-board-threadx-linux",
                "threadx-linux",
                Some("tcp/127.0.0.1:7447"),
            ),
            (
                "nros-board-freertos-posix",
                "freertos-posix",
                Some("tcp/127.0.0.1:7447"),
            ),
            (
                "nros-board-threadx-qemu-riscv64",
                "rv-virt-threadx",
                Some("tcp/10.0.2.2:7447"),
            ),
            (
                "nros-board-mps2-an385-freertos",
                "mps2-an385-freertos",
                Some("tcp/10.0.2.2:7447"),
            ),
            (
                "nros-board-mps3-an536-freertos",
                "mps3-an536-freertos",
                Some("tcp/10.0.2.2:7447"),
            ),
            (
                "nros-board-mps2-an385",
                "mps2-an385-baremetal",
                Some("tcp/10.0.2.2:7447"),
            ),
            (
                "nros-board-nuttx-qemu",
                "qemu-armv7a-nuttx",
                Some("tcp/10.0.2.2:7447"),
            ),
            (
                "nros-board-nuttx-qemu",
                "rv-virt-nuttx",
                Some("tcp/10.0.2.2:7447"),
            ),
            (
                "nros-board-s32z270-freertos",
                "s32z270-freertos",
                Some("tcp/192.0.3.1:7447"),
            ),
            ("zephyr", "zephyr", None),
            ("nros-board-mps2-an385-zephyr", "mps2-an385-zephyr", None),
        ] {
            let d = boards.join(pkg).join("nros-board.toml");
            assert_eq!(
                default_locator(&d, Some(board)).unwrap().as_deref(),
                want,
                "{board} ({})",
                d.display()
            );
        }
        // No descriptor: nothing to derive from.
        assert_eq!(
            default_locator(Path::new("/nonexistent"), None).unwrap(),
            None
        );
    }

    /// RFC-0109 D2 — an `"os"`-stack board reads its kernel config's address:
    /// each of the three keys, an unset one keeping the fallback, and a value
    /// that is not hex refused naming the key.
    #[test]
    fn a_kernel_config_states_the_address() {
        let q = ResolvedNet::qemu_network();
        let got = from_kconfig(
            "# CONFIG_NETINIT_NOMAC is not set\nCONFIG_NETINIT_IPADDR=0x0a00020f\n\
             CONFIG_NETINIT_DRIPADDR=0x0a000202\nCONFIG_NETINIT_NETMASK=0xffff0000\n",
            ResolvedNet::platform_default(),
        )
        .unwrap();
        assert_eq!((got.ip, got.gateway), (q.ip, q.gateway));
        assert_eq!(got.netmask, [255, 255, 0, 0]);
        assert_eq!(got.mac, ResolvedNet::platform_default().mac);
        let partial = from_kconfig("CONFIG_NETINIT_IPADDR=0x0a000210\n", q).unwrap();
        assert_eq!((partial.ip, partial.netmask), ([10, 0, 2, 16], q.netmask));
        let e = from_kconfig("CONFIG_NETINIT_IPADDR=10.0.2.15\n", q).unwrap_err();
        assert!(e.contains("CONFIG_NETINIT_IPADDR"), "{e}");
    }

    /// The board package's own NuttX kernel configs agree with QEMU's SLIRP
    /// plan, which is what the runtime `SIOCSIFADDR` push applies.
    #[test]
    fn the_in_tree_nuttx_defconfigs_state_qemus_plan() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../nros-board-nuttx-qemu/nuttx-config");
        for arch in ["arm", "arm-smp", "riscv"] {
            let text = std::fs::read_to_string(dir.join(arch).join("defconfig")).unwrap();
            let got = from_kconfig(&text, ResolvedNet::platform_default()).unwrap();
            let q = ResolvedNet::qemu_network();
            assert_eq!(
                (got.ip, got.gateway, got.netmask),
                (q.ip, q.gateway, q.netmask),
                "{arch}"
            );
        }
    }

    /// Issue 2064 — each NuttX board's descriptor names the kernel config its
    /// crate's build script reads, so `resolve` (what `nros board net` and the
    /// cmake entry locator ask) answers the same network as the crate's
    /// `BOARD_NET_*` constants: QEMU's SLIRP plan, never the platform default.
    #[test]
    fn a_nuttx_board_resolves_the_network_its_kernel_config_states() {
        let crate_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../nros-board-nuttx-qemu");
        let desc = crate_dir.join("nros-board.toml");
        // (board, the defconfig `nros-board-nuttx-qemu/build.rs` picks for its target)
        for (board, arch) in [("qemu-armv7a-nuttx", "arm"), ("rv-virt-nuttx", "riscv")] {
            let got = resolve(&desc, Some(board), ResolvedNet::platform_default()).unwrap();
            let text = std::fs::read_to_string(
                crate_dir.join("nuttx-config").join(arch).join("defconfig"),
            )
            .unwrap();
            let crate_side = from_kconfig(&text, ResolvedNet::platform_default()).unwrap();
            assert_eq!(got, crate_side, "{board}");
            assert_eq!(got.gateway, [10, 0, 2, 2], "{board}");
        }
    }

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
            ("network = \"lab\"\n", "not supported yet"),
            ("ip = \"10.0.2.40\"\n", "is retired"),
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
