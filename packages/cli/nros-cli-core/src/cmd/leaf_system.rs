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

    /// Issue 1556 item 4 -- answer for THIS board instead of the image's.
    ///
    /// The host census of a standalone leaf builds the leaf's own `src/` for
    /// the host (`nros ws entity-census take --leaf`, which configures with
    /// `-DNANO_ROS_LEAF_BOARD=native`). Every other row -- RMW, domain,
    /// locator -- stays the leaf's: only the board, and the deploy token it
    /// implies, change.
    #[arg(long, value_name = "BOARD")]
    pub board: Option<String>,

    /// Issue 1712 -- write the image's own layers (what its `transport`
    /// implies, then its `[image.<id>] env`, RFC-0049's APP rung) to PATH as a
    /// cargo config holding one `[env]` table, and print the path as
    /// `NROS_LEAF_IMAGE_ENV`.
    ///
    /// The CMAKE road's carrier: every cargo command cmake spawns for the image
    /// passes it as `--config`, so each row lands BELOW an exported variable
    /// exactly as the cargo road's `nros-cargo.toml` does (no `force`). Written
    /// only when the content changes; REMOVED, and the row printed empty, when
    /// the image states nothing, so such a build's cargo command is unchanged.
    #[arg(long, value_name = "PATH")]
    pub image_env_out: Option<PathBuf>,

    /// phase-481 W1 -- answer for THIS image of the `system.toml` in PATH.
    ///
    /// A leaf with several images (one per RMW) is built once per image, and
    /// the Zephyr module hook passes `-DNROS_IMAGE=<id>` here. With it, PATH
    /// may also be a workspace BRINGUP (a `system.toml` with no build file
    /// beside it), which is how a generated workspace application names its
    /// image. Without it, the leaf's own rule applies: one image, or
    /// `[system] default_images`.
    #[arg(long, value_name = "ID")]
    pub image: Option<String>,

    /// phase-481 W1 (RFC-0098 D11) -- write the image as a Zephyr Kconfig
    /// fragment to PATH: the RMW choice, the package's language API, the
    /// deploy endpoint from `locator`, and every `[image.<id>] env` row whose
    /// Kconfig symbol (through `nros_zephyr_build::KCONFIG_PAIRS`) exists in
    /// the module's `zephyr/Kconfig`. Printed as `NROS_LEAF_KCONFIG`.
    ///
    /// With it, `--image-env-out` carries only the rows NO Kconfig symbol
    /// holds, each also printed as `NROS_LEAF_IMAGE_ENV_ROW=<KEY>=<VALUE>` for
    /// the Zephyr cmake lane's own knob ladder. Written only when the content
    /// changes; removed (and printed empty) when the image states nothing.
    #[arg(long, value_name = "PATH")]
    pub kconfig_out: Option<PathBuf>,

    /// The language of the image's ENTRY, for the fragment's API row (`rust`,
    /// `c`, `cpp`), for a caller that knows it better than PATH does -- a
    /// generated workspace application, whose bringup holds no build file.
    /// Default: `Cargo.toml` in PATH is Rust; a `CMakeLists.txt` is the TYPED
    /// Zephyr carrier, C++.
    #[arg(long, value_name = "LANG")]
    pub language: Option<String>,
}

/// The cargo config the cmake road hands every cargo command it spawns for
/// the image (issue 1712): one `[env]` table, no `force`.
///
/// Values are TOML strings, so any byte a value can hold is escaped by the
/// serializer rather than by a cmake `string(REPLACE)`.
#[must_use]
pub fn image_env_config(rows: &std::collections::BTreeMap<String, String>) -> String {
    let mut env = toml::Table::new();
    for (k, v) in rows {
        env.insert(k.clone(), toml::Value::String(v.clone()));
    }
    let mut doc = toml::Table::new();
    doc.insert("env".to_string(), toml::Value::Table(env));
    format!(
        "# GENERATED by nros (`ws leaf-system --image-env-out`, issue 1712; `build`, phase-481 W4) -- do not edit.\n\
         # This image's own layers from its system.toml: what `transport` implies, then\n\
         # `[image.<id>] env` (RFC-0049's APP rung). No row is `force`d, so a variable\n\
         # the calling shell exports still wins, as on the cargo road.\n{}",
        toml::to_string(&doc).expect("a table of strings serializes")
    )
}

/// Write [`image_env_config`] to `out` when `rows` is non-empty (only if the
/// bytes differ, so an unchanged image leaves no new mtime for an edge to see),
/// or remove a stale one when it is empty. Returns the path when written.
pub fn write_image_env(
    out: &std::path::Path,
    rows: &std::collections::BTreeMap<String, String>,
) -> Result<Option<PathBuf>> {
    if rows.is_empty() {
        match std::fs::remove_file(out) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(eyre!("removing stale {}: {e}", out.display())),
        }
        return Ok(None);
    }
    let body = image_env_config(rows);
    if std::fs::read_to_string(out).ok().as_deref() != Some(body.as_str()) {
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).map_err(|e| eyre!("{}: {e}", dir.display()))?;
        }
        std::fs::write(out, body).map_err(|e| eyre!("{}: {e}", out.display()))?;
    }
    Ok(Some(out.to_path_buf()))
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
///
/// `NROS_LEAF_IMAGE_ENV` is the cargo config `--image-env-out` wrote (issue
/// 1712), empty when none was asked for or the image states nothing.
pub fn rows(
    leaf: &LeafSystem,
    deploy: &str,
    settings: Option<&std::path::Path>,
    image_env: Option<&std::path::Path>,
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
        (
            "NROS_LEAF_IMAGE_ENV",
            image_env
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
        ),
        ("NROS_LEAF_ORIGIN", leaf.origin_path().display().to_string()),
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
    let read = match &args.image {
        Some(id) => leaf_system::read_image(&dir, id),
        None => leaf_system::read(&dir),
    };
    let leaf = read.map_err(|e| eyre!(e))?.ok_or_else(|| {
        eyre!(
            "{}: declares no deployment — write {} with `[image.<id>] board = \"<board>\"` \
             (RFC-0098 D3)",
            dir.display(),
            dir.join(leaf_system::SYSTEM_TOML).display()
        )
    })?;
    let mut leaf = leaf;
    if let Some(over) = &args.board {
        leaf.board = Some(over.clone());
    }
    let board = leaf.board.clone().ok_or_else(|| {
        eyre!(
            "{}: names no board (RFC-0098 D3: `[image.<id>] board`)",
            leaf.origin_path().display()
        )
    })?;
    // issue 1641 — the 1510 ladder: the leaf's own checkout outranks an
    // inherited `$NROS_REPO_DIR` (in a linked worktree, the PARENT's).
    let root = crate::orchestration::nano_ros_root::resolve(args.nano_ros_path, &dir);
    let catalog = root.as_deref().and_then(|r| BoardCatalog::load(r).ok());
    let deploy = deploy_token(catalog.as_ref(), &board);
    let settings = match root.as_deref() {
        Some(r) if leaf_system::is_package_dir(&dir) => {
            crate::cmd::leaf_settings::resolve(&dir, r)?.map(|i| i.config_path)
        }
        _ => None,
    };
    // phase-481 W1 -- the Zephyr road splits the image's rows: those a Kconfig
    // symbol carries go to the fragment, the rest to the `--config` file.
    let (kconfig, env_rows) = match &args.kconfig_out {
        Some(out) => {
            let root = root.as_deref().ok_or_else(|| {
                eyre!(
                    "--kconfig-out: no nano-ros checkout to read {} from (pass --nano-ros-path)",
                    crate::cmd::leaf_kconfig::KCONFIG_FILE
                )
            })?;
            let syms = crate::cmd::leaf_kconfig::KconfigSymbols::load(root)?;
            let language = match &args.language {
                Some(l) => {
                    Some(nros_lang::Language::parse(l).map_err(|e| eyre!("--language: {e}"))?)
                }
                None => crate::cmd::leaf_kconfig::package_language(&dir),
            };
            let r =
                crate::cmd::leaf_kconfig::render(&leaf, language, &syms).map_err(|e| eyre!(e))?;
            let text = crate::cmd::leaf_kconfig::fragment_text(&leaf, &r);
            let written = crate::cmd::leaf_kconfig::write_fragment(out, text.as_deref())?;
            (Some(written), r.cargo_rows)
        }
        None => (None, crate::cmd::leaf_settings::image_layers(&leaf)),
    };
    let image_env = match &args.image_env_out {
        Some(out) => write_image_env(out, &env_rows)?,
        None => None,
    };
    for (k, v) in rows(&leaf, &deploy, settings.as_deref(), image_env.as_deref()) {
        println!("{k}={v}");
    }
    if let Some(written) = kconfig {
        println!(
            "NROS_LEAF_KCONFIG={}",
            written.map(|p| p.display().to_string()).unwrap_or_default()
        );
        for (k, v) in &env_rows {
            println!("NROS_LEAF_IMAGE_ENV_ROW={k}={v}");
        }
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
            rows(&leaf, "freertos", None, None).into_iter().collect();
        assert_eq!(
            rows["NROS_LEAF_SETTINGS"], "",
            "no settings road for a C leaf"
        );
        assert_eq!(rows["NROS_LEAF_BOARD"], "mps2-an385-freertos");
        assert_eq!(rows["NROS_LEAF_DEPLOY"], "freertos");
        assert_eq!(rows["NROS_LEAF_RMW"], "zenoh");
        assert_eq!(rows["NROS_LEAF_DOMAIN_ID"], "0");
        assert_eq!(rows["NROS_LEAF_LOCATOR"], "tcp/192.0.3.1:7447");
        // No fallback row: the manifest fallback it reported is deleted
        // (phase-445 W5), so it could only ever print `0`.
        assert!(!rows.contains_key("NROS_LEAF_FALLBACK"));
        // Absent keys are printed empty, never omitted.
        assert_eq!(rows["NROS_LEAF_IP"], "");
        assert_eq!(rows["NROS_LEAF_IMAGE_ENV"], "", "nothing asked for");
    }

    /// Issue 1712 -- a C leaf's `[image.<id>] env` becomes a cargo config the
    /// cmake road passes as `--config`; transport implications come first and
    /// the image's own row outranks them; values are TOML-escaped.
    #[test]
    fn image_env_is_written_as_an_unforced_env_table() {
        let td = leaf_dir(
            "[system]\nname = \"c\"\nrmw = \"zenoh\"\n\n\
             [image.native]\nboard = \"native\"\ntransport = \"serial\"\n\
             env = { NROS_LOG_MAX_LEVEL = \"warn\", NROS_LINK_IP = \"1\", Q = \"a\\\"b\" }\n",
        );
        let leaf = leaf_system::read(td.path()).unwrap().unwrap();
        let layers = crate::cmd::leaf_settings::image_layers(&leaf);
        assert_eq!(layers["ZPICO_NO_SMOLTCP"], "1", "the serial implication");
        assert_eq!(
            layers["NROS_LINK_IP"], "1",
            "the image outranks its implication"
        );
        let out = td.path().join("b/nros-image-env.toml");
        assert_eq!(
            write_image_env(&out, &layers).unwrap().as_deref(),
            Some(out.as_path())
        );
        let text = std::fs::read_to_string(&out).unwrap();
        let doc: toml::Table = text.parse().unwrap();
        let env = doc["env"].as_table().unwrap();
        assert_eq!(env["NROS_LOG_MAX_LEVEL"].as_str(), Some("warn"));
        assert_eq!(env["Q"].as_str(), Some("a\"b"));
        assert!(
            env.values().all(toml::Value::is_str),
            "a `{{ value, force }}` table would outrank the shell: {text}"
        );
        // Unchanged content leaves the file alone; an image that states
        // nothing removes it.
        let before = std::fs::metadata(&out).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_image_env(&out, &layers).unwrap();
        assert_eq!(std::fs::metadata(&out).unwrap().modified().unwrap(), before);
        assert_eq!(write_image_env(&out, &Default::default()).unwrap(), None);
        assert!(!out.exists());
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
