//! `nros ws leaf-system <dir>` — phase-445 W3 (RFC-0098 D3/D5).
//!
//! Prints a single-package leaf's deployment — board, RMW, domain, network
//! identity — as `KEY=VALUE` lines, for a build system that cannot link the
//! reader itself. The C/C++ lanes are that consumer: `find_package(nano_ros)`
//! runs this before it imports nano-ros, so the leaf's `system.toml` chooses
//! the platform module exactly as the retiring `package.xml`
//! `<nano_ros deploy= board= rmw=/>` tuple does.
//!
//! The answer is `nros_orchestration_ir::leaf_system::read`'s — the SAME reader
//! the `nros::main!` proc-macro and `nros sync` use — so a Rust and a C leaf
//! cannot read one `system.toml` two ways. The only thing added here is the
//! DEPLOY token (the C/C++ platform-module axis), derived from the board
//! through the board catalog rather than authored.

use std::path::PathBuf;

use clap::Args as ClapArgs;
use eyre::{Result, eyre};
use nros_orchestration_ir::leaf_system::{self, LeafSystem};

use crate::orchestration::board_descriptor::{BoardCatalog, DeployResolution};

#[derive(Debug, ClapArgs)]
pub struct LeafSystemArgs {
    /// The leaf package directory (holding `system.toml` beside its
    /// `Cargo.toml` / `CMakeLists.txt`).
    pub path: PathBuf,

    /// nano-ros checkout whose board catalog maps the board to its platform.
    /// Default: `$NROS_REPO_DIR`, then the checkout enclosing the leaf.
    #[arg(long)]
    pub nano_ros_path: Option<PathBuf>,
}

/// The C/C++ deploy token (the `NANO_ROS_PLATFORM` axis) a board implies:
/// its descriptor's platform FAMILY (`PlatformKind::cmake_deploy`). A board
/// no single descriptor claims, or one whose platform has no C/C++ module, is
/// passed through verbatim, so the platform module reports it the way it
/// reports any unknown deploy today.
pub fn deploy_token(catalog: Option<&BoardCatalog>, board: &str) -> String {
    let Some(catalog) = catalog else {
        return board.to_string();
    };
    match catalog.resolve_deploy(board) {
        DeployResolution::Board(d) => d
            .platform
            .cmake_deploy()
            .map_or_else(|| board.to_string(), str::to_string),
        _ => board.to_string(),
    }
}

/// The `KEY=VALUE` rows, every key always present (empty when unset) so a
/// consumer never has to distinguish "absent" from "not printed".
///
/// `NROS_LEAF_SETTINGS` is where `nros sync` / `nros build` write this leaf's
/// generated cargo settings (phase-445 W4b, `cmd::leaf_settings`), empty for a
/// leaf that road does not build (a C/C++ leaf, a Zephyr one). The fixture
/// lane and its staleness probe read it here rather than re-deriving the path.
pub fn rows(
    leaf: &LeafSystem,
    deploy: &str,
    settings: Option<&std::path::Path>,
) -> Vec<(&'static str, String)> {
    let s = |v: &Option<String>| v.clone().unwrap_or_default();
    let net = &leaf.network;
    vec![
        (
            "NROS_LEAF_SETTINGS",
            settings
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
        ),
        ("NROS_LEAF_ORIGIN", leaf.origin_path().display().to_string()),
        (
            "NROS_LEAF_FALLBACK",
            if leaf.is_fallback() { "1" } else { "0" }.into(),
        ),
        ("NROS_LEAF_IMAGE", s(&leaf.image)),
        ("NROS_LEAF_BOARD", s(&leaf.board)),
        ("NROS_LEAF_DEPLOY", deploy.to_string()),
        ("NROS_LEAF_RMW", s(&leaf.rmw)),
        (
            "NROS_LEAF_DOMAIN_ID",
            net.domain_id.map(|d| d.to_string()).unwrap_or_default(),
        ),
        ("NROS_LEAF_LOCATOR", s(&net.locator)),
        ("NROS_LEAF_IP", s(&net.ip)),
        ("NROS_LEAF_GATEWAY", s(&net.gateway)),
        ("NROS_LEAF_NETMASK", s(&net.netmask)),
        ("NROS_LEAF_TRANSPORT", s(&net.transport)),
    ]
}

pub fn run(args: LeafSystemArgs) -> Result<()> {
    let dir = args
        .path
        .canonicalize()
        .map_err(|e| eyre!("{}: {e}", args.path.display()))?;
    let leaf = leaf_system::read(&dir)
        .map_err(|e| eyre!(e))?
        .ok_or_else(|| {
            eyre!(
                "{}: declares no deployment — write {} with `[image.<id>] board = \"<board>\"` \
             (RFC-0098 D3)",
                dir.display(),
                dir.join(leaf_system::SYSTEM_TOML).display()
            )
        })?;
    let board = leaf.board.clone().ok_or_else(|| {
        eyre!(
            "{}: names no board (RFC-0098 D3: `[image.<id>] board`)",
            leaf.origin_path().display()
        )
    })?;
    if let Some(w) = leaf.deprecation() {
        eprintln!("{w}");
    }
    let root = args
        .nano_ros_path
        .or_else(|| std::env::var_os("NROS_REPO_DIR").map(PathBuf::from))
        .or_else(|| crate::cmd::ws::autodetect_nano_ros_path(&dir));
    let catalog = root.as_deref().and_then(|r| BoardCatalog::load(r).ok());
    let deploy = deploy_token(catalog.as_ref(), &board);
    let settings = match root.as_deref() {
        Some(r) => crate::cmd::leaf_settings::resolve(&dir, r)?.map(|i| i.config_path),
        None => None,
    };
    for (k, v) in rows(&leaf, &deploy, settings.as_deref()) {
        println!("{k}={v}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf_dir(system: &str) -> tempfile::TempDir {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("CMakeLists.txt"), "project(x C)\n").unwrap();
        std::fs::write(td.path().join("system.toml"), system).unwrap();
        td
    }

    #[test]
    fn a_cmake_leaf_prints_every_key_from_its_system_toml() {
        let td = leaf_dir(
            "[system]\nname = \"c_talker\"\nrmw = \"zenoh\"\ndomain_id = 0\n\
             locator = \"tcp/192.0.3.1:7447\"\n\n\
             [image.freertos]\nboard = \"mps2-an385-freertos\"\n",
        );
        let leaf = leaf_system::read(td.path()).unwrap().unwrap();
        let rows: std::collections::BTreeMap<_, _> =
            rows(&leaf, "freertos", None).into_iter().collect();
        assert_eq!(
            rows["NROS_LEAF_SETTINGS"], "",
            "no settings road for a C leaf"
        );
        assert_eq!(rows["NROS_LEAF_BOARD"], "mps2-an385-freertos");
        assert_eq!(rows["NROS_LEAF_DEPLOY"], "freertos");
        assert_eq!(rows["NROS_LEAF_RMW"], "zenoh");
        assert_eq!(rows["NROS_LEAF_DOMAIN_ID"], "0");
        assert_eq!(rows["NROS_LEAF_LOCATOR"], "tcp/192.0.3.1:7447");
        assert_eq!(rows["NROS_LEAF_FALLBACK"], "0");
        // Absent keys are printed empty, never omitted.
        assert_eq!(rows["NROS_LEAF_IP"], "");
    }

    /// The deploy token comes from the board catalog, so the in-tree boards
    /// map to the platform module axis the C/C++ leaves have always named.
    #[test]
    fn the_deploy_token_is_the_boards_platform() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root")
            .to_path_buf();
        let catalog = BoardCatalog::load(&root).expect("in-tree board catalog");
        assert_eq!(deploy_token(Some(&catalog), "native"), "native");
        assert_eq!(
            deploy_token(Some(&catalog), "mps2-an385-freertos"),
            "freertos"
        );
        // Both ThreadX boards are one platform FAMILY on the C/C++ axis — the
        // tuple spelled them `deploy="threadx"` with the board beside it.
        assert_eq!(deploy_token(Some(&catalog), "threadx-linux"), "threadx");
        assert_eq!(
            deploy_token(Some(&catalog), "threadx-qemu-riscv64"),
            "threadx"
        );
        assert_eq!(deploy_token(Some(&catalog), "qemu-armv7a-nuttx"), "nuttx");
        assert_eq!(deploy_token(Some(&catalog), "zephyr"), "zephyr");
        // No C/C++ platform module for esp32: passed through, not invented.
        assert_eq!(
            deploy_token(Some(&catalog), "esp32-c3-baremetal"),
            "esp32-c3-baremetal"
        );
        assert_eq!(
            deploy_token(Some(&catalog), "no-such-board"),
            "no-such-board"
        );
        assert_eq!(deploy_token(None, "freertos"), "freertos");
    }
}
