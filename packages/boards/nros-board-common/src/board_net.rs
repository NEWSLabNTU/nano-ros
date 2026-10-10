//! phase-484 W4b (RFC-0103 D1) — a board's network identity defaults, from its
//! own descriptor, for its build script.
//!
//! A board's fallback IP / netmask / gateway / MAC used to be written into the
//! board crate's `Config::default()`, its build script's C `NROS_APP_CONFIG`
//! emitter, and a cmake board module — three copies of one fact. Each copy is
//! now generated from `[board.net]` in the board's `nros-board.toml`
//! (`nros_platform_config::BoardNet`), and a field the board does not state
//! falls to the platform default, [`crate::BaseConfig::default`] (owner order:
//! platform < board < image).
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
    Ok(ResolvedNet {
        ip: net.ip()?.unwrap_or(fallback.ip),
        netmask: net.netmask()?.unwrap_or(fallback.netmask),
        gateway: net.gateway()?.unwrap_or(fallback.gateway),
        mac: net.mac()?.unwrap_or(fallback.mac),
    })
}

/// For a board crate's `build.rs`: resolve this crate's own descriptor
/// (`$CARGO_MANIFEST_DIR/nros-board.toml`) over `fallback`, watch it, and write
/// `$OUT_DIR/nros_board_net.rs` for `include!` — four `BOARD_NET_*` constants
/// the crate's `Config::default()` reads. Panics with the descriptor's error:
/// the right failure in a build script.
pub fn emit_for_crate(fallback: ResolvedNet) -> ResolvedNet {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let descriptor = manifest.join("nros-board.toml");
    println!("cargo:rerun-if-changed={}", descriptor.display());
    println!("cargo:rerun-if-env-changed=NROS_BOARD");
    let board = std::env::var("NROS_BOARD").ok().filter(|b| !b.is_empty());
    let net = resolve(&descriptor, board.as_deref(), fallback)
        .unwrap_or_else(|e| panic!("{}: {e}", descriptor.display()));
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("nros_board_net.rs");
    let body = format!(
        "// Generated from {} [board.net] (phase-484 W4b). Do not edit.\n\
         pub(crate) const BOARD_NET_IP: [u8; 4] = {:?};\n\
         pub(crate) const BOARD_NET_NETMASK: [u8; 4] = {:?};\n\
         pub(crate) const BOARD_NET_GATEWAY: [u8; 4] = {:?};\n\
         pub(crate) const BOARD_NET_MAC: [u8; 6] = {:?};\n",
        descriptor.display(),
        net.ip,
        net.netmask,
        net.gateway,
        net.mac
    );
    std::fs::write(&out, body).unwrap_or_else(|e| panic!("write {}: {e}", out.display()));
    net
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_board_overrides_only_what_it_states_and_a_typo_is_refused() {
        let d = std::env::temp_dir().join(format!("nros-board-net-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("nros-board.toml");
        std::fs::write(
            &f,
            "[[board]]\nnames = [\"b\"]\n\n[board.net]\nip = \"10.0.2.40\"\nmac = \"52:54:00:12:34:56\"\n",
        )
        .unwrap();
        let base = ResolvedNet::platform_default();
        let got = resolve(&f, None, base).unwrap();
        assert_eq!(got.ip, [10, 0, 2, 40]);
        assert_eq!(got.mac, [0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
        assert_eq!(
            got.gateway, base.gateway,
            "an unstated field keeps the platform default"
        );

        std::fs::write(
            &f,
            "[[board]]\nnames = [\"b\"]\n\n[board.net]\nipp = \"1.2.3.4\"\n",
        )
        .unwrap();
        assert!(resolve(&f, None, base).unwrap_err().contains("ipp"));
        std::fs::write(
            &f,
            "[[board]]\nnames = [\"b\"]\n\n[board.net]\nip = \"1.2.3\"\n",
        )
        .unwrap();
        assert!(resolve(&f, None, base).unwrap_err().contains("expected 4"));
        let _ = std::fs::remove_dir_all(&d);
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
