//! phase-445 W4b (RFC-0098 D1/D2/D7) — a single-package example is a
//! workspace of one.
//!
//! A single-package Rust leaf (a `Cargo.toml` `[package]` with a `system.toml`
//! beside it naming its board) gets the SAME generated settings file a
//! workspace image does, written by the SAME writer
//! ([`crate::builder::cargo_config`]):
//!
//! ```text
//!   <leaf>/build/<image>/nros-cargo.toml
//! ```
//!
//! Both `nros sync` (for a user who drives cargo) and `nros build` write it;
//! both build it as
//!
//! ```text
//!   cargo build --manifest-path <leaf>/Cargo.toml --config <leaf>/build/<image>/nros-cargo.toml
//! ```
//!
//! ## Two facts about cargo this relies on
//!
//! **Relative paths resolve against the file's GRANDPARENT** (measured by
//! phase-445 W4). For `<leaf>/build/<image>/nros-cargo.toml` that is
//! `<leaf>/build/`, and [`crate::builder::cargo_config::render`] already writes
//! every relative path against that base — the writer is shared, not re-spelled.
//!
//! **The working directory still decides which `.cargo/config.toml` files cargo
//! DISCOVERS.** Two consequences, handled separately:
//!
//! * The nano-ros `--profile nros-*` presets used to come only from the
//!   checkout's root `.cargo/config.toml`, i.e. only when cargo ran inside the
//!   checkout. The writer now carries them, so the working directory carries no
//!   build fact.
//! * Until phase-445 W6 deletes it, the leaf still has its own
//!   `.cargo/config.toml`, whose `include` of the board projection repeats the
//!   board's `[target.<triple>] rustflags`. cargo JOINS arrays across config
//!   files, so reading both doubles the link script — measured on the mps2
//!   bare-metal talker: `rust-lld: error: memory.x:19: region 'FLASH' already
//!   defined`. So this road runs cargo from the directory ABOVE the leaf: every
//!   ancestor config (a user's own preference, RFC-0098 D1) is still read, the
//!   leaf's own is not. Once W6 has deleted the leaf `.cargo/`, running from the
//!   leaf is equivalent.
//!
//! ## What the file carries
//!
//! Lowest precedence first, each later layer overriding a key the earlier one
//! also sets:
//!
//! 1. the board descriptor's `cargo_config` (triple, link group, build-std);
//! 2. the derived pool knobs and the image's entity FACTS
//!    ([`crate::leaf_entity_env::leaf_env`] — the same computation the per-leaf
//!    sidecar uses);
//! 3. the board facts (`nros ws board-facts`: `NROS_BOARD_TOML`,
//!    `NROS_PLATFORM_NAME`, site config), which the fixture lane used to export
//!    per invocation;
//! 4. **transitional**: the `[env]` rows the leaf's tracked `.cargo/config.toml`
//!    still AUTHORS (esp32's arena and large-buffer budgets, the XRCE pool
//!    sizes, the FreeRTOS provisioning paths). They have no other home yet, and
//!    running from above the leaf means cargo would not see them otherwise. W6
//!    has to re-home each before it can delete the file; this layer then empties.
//!
//! Plus the in-repo `[patch.crates-io]` rows the graph names registry-style.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use eyre::{Result, bail, eyre};
use nros_orchestration_ir::leaf_system::{self, LeafSystem};

use crate::{
    builder::{
        cargo_config,
        plan::{self, Driver},
    },
    orchestration::board_descriptor::BoardCatalog,
};

/// A single-package leaf this road builds, resolved.
#[derive(Debug, Clone)]
pub struct LeafImage {
    /// The leaf directory, canonical.
    pub leaf: PathBuf,
    /// Its `[package] name`.
    pub package: String,
    /// What its `system.toml` declares.
    pub decl: LeafSystem,
    /// `[image.<id>]`.
    pub image_id: String,
    /// The board, as authored.
    pub board: String,
    /// The board's platform token.
    pub platform: String,
    /// The rustc triple the board pins, if any.
    pub target: Option<String>,
    /// The descriptor's `cargo_config` template.
    pub cargo_config: Option<String>,
    /// Where the settings file lives.
    pub config_path: PathBuf,
}

/// `<leaf>/build/<image>/nros-cargo.toml`.
#[must_use]
pub fn settings_path(leaf: &Path, image_id: &str) -> PathBuf {
    leaf.join("build")
        .join(image_id)
        .join(cargo_config::FILE_NAME)
}

/// The leaf manifest's `[package] name`; `None` for a directory whose
/// `Cargo.toml` is absent or is not a package.
fn package_name(leaf: &Path) -> Option<String> {
    let text = std::fs::read_to_string(leaf.join("Cargo.toml")).ok()?;
    let doc = text.parse::<toml::Table>().ok()?;
    doc.get("package")?
        .get("name")?
        .as_str()
        .map(str::to_string)
}

/// The leaf this road builds, or `None` when `leaf` is not one.
///
/// Not one: no `system.toml` beside a `[package]` manifest, or a board whose
/// driver is not cargo (a Zephyr leaf is a west application with its own
/// build). A leaf that IS one but names a board no descriptor claims is an
/// error, not a `None`: that is a typo in the one line RFC-0098 D3 says a user
/// edits. (A leaf still on the retiring manifest keys used to land here too;
/// phase-445 W5 made `leaf_system::read` refuse it instead, so it never
/// reaches this function.)
pub fn resolve(leaf: &Path, nano_ros_root: &Path) -> Result<Option<LeafImage>> {
    if !leaf.join(leaf_system::SYSTEM_TOML).is_file() {
        return Ok(None);
    }
    let Some(package) = package_name(leaf) else {
        return Ok(None);
    };
    let Some(decl) = leaf_system::read(leaf).map_err(|e| eyre!(e))? else {
        return Ok(None);
    };
    let Some(board) = decl.board.clone() else {
        bail!(
            "{}: names no board (RFC-0098 D3: `[image.<id>] board = \"<board>\"`)",
            decl.origin_path().display()
        );
    };
    let catalog = BoardCatalog::load(nano_ros_root)
        .map_err(|e| eyre!("board catalog under {}: {e}", nano_ros_root.display()))?;
    let Some(d) = crate::cmd::board_facts::resolve_board(&catalog, &board) else {
        bail!(
            "{}: board `{board}` is claimed by no board descriptor under {}",
            decl.origin_path().display(),
            nano_ros_root.display()
        );
    };
    let platform = d.platform.kebab().to_string();
    if plan::driver_for_board(&platform, d.entry_kind, false) != Driver::Cargo {
        return Ok(None);
    }
    let image_id = decl.image.clone().unwrap_or_else(|| board.clone());
    let leaf = leaf.canonicalize().unwrap_or_else(|_| leaf.to_path_buf());
    Ok(Some(LeafImage {
        config_path: settings_path(&leaf, &image_id),
        leaf,
        package,
        image_id,
        board,
        platform,
        target: d.target.clone(),
        cargo_config: d.cargo_config.clone(),
        decl,
    }))
}

/// The command a user retypes, from the directory ABOVE the leaf (see the
/// module docs for why above).
#[must_use]
pub fn build_command(img: &LeafImage) -> (PathBuf, Vec<String>) {
    let parent = img
        .leaf
        .parent()
        .map_or_else(|| img.leaf.clone(), Path::to_path_buf);
    let name = img
        .leaf
        .file_name()
        .map_or_else(|| ".".into(), |n| n.to_string_lossy().into_owned());
    let config = img
        .config_path
        .strip_prefix(&parent)
        .map_or_else(|_| img.config_path.clone(), Path::to_path_buf);
    (
        parent,
        vec![
            "build".to_string(),
            "--manifest-path".to_string(),
            format!("{name}/Cargo.toml"),
            "--config".to_string(),
            config.display().to_string(),
        ],
    )
}

/// Is `value` a path this file should write relative (an absolute path that
/// exists)? Anything else — a board name, a `:`-joined search path — is a
/// plain string.
fn is_path_value(value: &str) -> bool {
    let p = Path::new(value);
    p.is_absolute() && p.exists()
}

/// Layer 4 — the `[env]` rows the leaf's tracked `.cargo/config.toml` still
/// authors. See the module docs; W6 re-homes each and deletes the file.
///
/// A `relative = true` row is resolved against the LEAF (cargo resolves it
/// against the parent of `.cargo/`) and re-expressed against the settings
/// file's own base by the writer. A `force` row is refused: this file never
/// forces, because a lane that sets the variable must win (RFC-0049), and a
/// leaf that relied on overriding its caller has to say so somewhere W6 can
/// see rather than have it silently weakened here.
fn authored_leaf_env(
    leaf: &Path,
    env: &mut BTreeMap<String, String>,
    path_env: &mut BTreeMap<String, PathBuf>,
) -> Result<Vec<String>> {
    let cfg = leaf.join(".cargo").join("config.toml");
    let Ok(text) = std::fs::read_to_string(&cfg) else {
        return Ok(Vec::new());
    };
    let doc: toml::Table = text
        .parse()
        .map_err(|e| eyre!("{}: does not parse: {e}", cfg.display()))?;
    let Some(rows) = doc.get("env").and_then(|e| e.as_table()) else {
        return Ok(Vec::new());
    };
    let mut carried = Vec::new();
    for (k, v) in rows {
        match v {
            toml::Value::String(s) => {
                path_env.remove(k);
                env.insert(k.clone(), s.clone());
            }
            toml::Value::Table(t) => {
                if t.get("force").and_then(|f| f.as_bool()) == Some(true) {
                    bail!(
                        "{}: `[env] {k}` sets `force = true`. The generated settings file never \
                         forces (a lane's value must win, RFC-0049); move this row to its home \
                         (the board descriptor's `[board.knobs]`, the image, or a derivation) \
                         before building through `build/<image>/nros-cargo.toml`.",
                        cfg.display()
                    );
                }
                let Some(value) = t.get("value").and_then(|v| v.as_str()) else {
                    bail!("{}: `[env] {k}` has no string `value`", cfg.display());
                };
                if t.get("relative").and_then(|r| r.as_bool()) == Some(true) {
                    env.remove(k);
                    path_env.insert(k.clone(), leaf.join(value));
                } else {
                    path_env.remove(k);
                    env.insert(k.clone(), value.to_string());
                }
            }
            other => bail!(
                "{}: `[env] {k}` is a {}, not a string or a table",
                cfg.display(),
                other.type_str()
            ),
        }
        carried.push(k.clone());
    }
    Ok(carried)
}

/// Write `<leaf>/build/<image>/nros-cargo.toml`, or return `None` when `leaf`
/// is not a leaf this road builds ([`resolve`]).
///
/// `who` prefixes diagnostics (`sync`, `nros build`).
pub fn write(leaf: &Path, nano_ros_root: &Path, who: &str) -> Result<Option<LeafImage>> {
    let Some(img) = resolve(leaf, nano_ros_root)? else {
        return Ok(None);
    };
    let leaf = img.leaf.as_path();

    // Layer 2 — derived pools + facts.
    let mut env = crate::leaf_entity_env::leaf_env(leaf, who).env;
    let mut path_env: BTreeMap<String, PathBuf> = BTreeMap::new();

    // Layer 3 — the board facts, by the one resolver the other lanes use.
    let facts =
        crate::cmd::board_facts::resolve(leaf, nano_ros_root, None, Some(&img.board), &|k| {
            std::env::var(k).ok()
        })
        .map_err(|e| eyre!("{}: board facts for `{}`: {e}", leaf.display(), img.board))?;
    for (k, v) in facts {
        if is_path_value(&v) {
            env.remove(&k);
            path_env.insert(k, PathBuf::from(v));
        } else {
            path_env.remove(&k);
            env.insert(k, v);
        }
    }

    // Layer 4 — transitional, until W6.
    authored_leaf_env(leaf, &mut env, &mut path_env)?;

    // No absolute path in the hint: the file is otherwise checkout-independent,
    // and the command is written relative to the directory it names.
    let (_cwd, args) = build_command(&img);
    let hint = format!(
        "(from the directory ABOVE the package, so the package's own\n\
         `.cargo/config.toml` is not read a second time; phase-445 W6 deletes it)\n\
         cargo {}",
        args.join(" ")
    );
    let spec = cargo_config::CargoConfigSpec {
        image_id: img.image_id.clone(),
        board: img.board.clone(),
        cargo_config: img.cargo_config.clone(),
        target: img.target.clone(),
        nano_ros_root: nano_ros_root
            .canonicalize()
            .unwrap_or_else(|_| nano_ros_root.to_path_buf()),
        workspace: leaf.to_path_buf(),
        target_dir: leaf.join("build").join(&img.image_id).join("target"),
        env,
        path_env,
        patches: crate::cmd::build::registry_patches(leaf, nano_ros_root, leaf),
        build_hint: Some(hint),
    };
    cargo_config::write(&spec, &img.config_path)
        .map_err(|e| eyre!("writing the settings for `{}`: {e}", img.image_id))?;
    Ok(Some(img))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(system: &str, cargo_env: Option<&str>) -> tempfile::TempDir {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(td.path().join("system.toml"), system).unwrap();
        if let Some(env) = cargo_env {
            std::fs::create_dir_all(td.path().join(".cargo")).unwrap();
            std::fs::write(td.path().join(".cargo/config.toml"), env).unwrap();
        }
        td
    }

    #[test]
    fn the_settings_file_sits_under_the_leafs_build_dir_per_image() {
        assert_eq!(
            settings_path(Path::new("/w/talker"), "native"),
            PathBuf::from("/w/talker/build/native/nros-cargo.toml")
        );
    }

    #[test]
    fn authored_rows_are_carried_and_a_relative_one_is_rebased_on_the_leaf() {
        let td = leaf(
            "",
            Some(
                "[env]\nNROS_EXECUTOR_ARENA_SIZE = \"16384\"\n\
                 SRC = { value = \"../../src\", relative = true }\n",
            ),
        );
        let (mut env, mut paths) = (BTreeMap::new(), BTreeMap::new());
        env.insert("NROS_EXECUTOR_ARENA_SIZE".to_string(), "8192".to_string());
        let carried = authored_leaf_env(td.path(), &mut env, &mut paths).unwrap();
        assert_eq!(carried.len(), 2);
        assert_eq!(
            env["NROS_EXECUTOR_ARENA_SIZE"], "16384",
            "the leaf's own row wins over a derived one, as its `include` order did"
        );
        assert_eq!(paths["SRC"], td.path().join("../../src"));
    }

    #[test]
    fn a_forced_authored_row_is_refused_not_weakened() {
        let td = leaf("", Some("[env]\nX = { value = \"1\", force = true }\n"));
        let e = authored_leaf_env(td.path(), &mut BTreeMap::new(), &mut BTreeMap::new())
            .unwrap_err()
            .to_string();
        assert!(e.contains("force"), "{e}");
    }

    #[test]
    fn a_leaf_without_system_toml_is_not_this_road() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        assert!(resolve(td.path(), td.path()).unwrap().is_none());
    }

    #[test]
    fn the_retypable_command_runs_from_above_the_leaf() {
        let img = LeafImage {
            leaf: PathBuf::from("/w/examples/talker"),
            package: "demo".into(),
            decl: LeafSystem {
                origin: leaf_system::Origin::SystemToml("/w/examples/talker/system.toml".into()),
                image: Some("native".into()),
                board: Some("native".into()),
                board_from: None,
                rmw: None,
                network: Default::default(),
                components: Vec::new(),
            },
            image_id: "native".into(),
            board: "native".into(),
            platform: "posix".into(),
            target: None,
            cargo_config: None,
            config_path: settings_path(Path::new("/w/examples/talker"), "native"),
        };
        let (cwd, args) = build_command(&img);
        assert_eq!(cwd, PathBuf::from("/w/examples"));
        assert_eq!(
            args,
            [
                "build",
                "--manifest-path",
                "talker/Cargo.toml",
                "--config",
                "talker/build/native/nros-cargo.toml"
            ]
        );
    }
}
