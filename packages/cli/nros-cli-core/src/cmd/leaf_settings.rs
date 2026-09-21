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
//! DISCOVERS**, and the nano-ros `--profile nros-*` presets used to come only
//! from the checkout's root one, i.e. only when cargo ran inside the checkout.
//! The writer carries them now, so the working directory carries no build fact.
//!
//! This road still runs cargo from the directory ABOVE the leaf. That was
//! REQUIRED while the leaf had its own AUTHORED `.cargo/config.toml`: its
//! `include` of the board projection repeated the board's
//! `[target.<triple>] rustflags`, and cargo JOINS arrays across config files,
//! so reading both doubled the link script — measured on the mps2 bare-metal
//! talker, `rust-lld: error: memory.x:19: region 'FLASH' already defined`.
//! phase-445 W6 deleted every `examples/**/.cargo/`, so running from the leaf
//! is now equivalent; the invocation stays where it is because the fixture lane
//! and its staleness probe share it, and a second spelling is a permanent
//! false-STALE.
//!
//! **Issue 1381 — and a plain `cargo build` INSIDE the leaf works too.**
//! [`wire_settings_into_leaf_config`] puts an `include` of this file into the
//! leaf's own (gitignored) `.cargo/config.toml`, the one sync already writes
//! for the central patch. Cargo discovers that file from the working directory,
//! so the board's triple and link group finally reach an invocation that names
//! no flags — which is what "standalone copy-out project" has to mean. It costs
//! the lane nothing: the lane's working directory is ABOVE the leaf, where that
//! file is not on cargo's discovery path, so the doubling above cannot come
//! back through it. Combining the two BY HAND still doubles, and the generated
//! file's own header says so.
//!
//! ## What the file carries
//!
//! Lowest precedence first, each later layer overriding a key the earlier one
//! also sets:
//!
//! 1. the board descriptor's `cargo_config` (triple, link group, build-std)
//!    and its `[board.knobs]`, reached through `NROS_BOARD_TOML`;
//! 2. the derived pool knobs and the image's entity FACTS
//!    ([`crate::leaf_entity_env::leaf_env`]);
//! 3. the board facts (`nros ws board-facts`: `NROS_BOARD_TOML`,
//!    `NROS_PLATFORM_NAME`, site config), which the fixture lane used to export
//!    per invocation;
//! 4. what this image's declared TRANSPORT implies
//!    ([`transport_implications`]);
//! 5. the APP rung — `[image.<id>] env` in the leaf's `system.toml`
//!    (RFC-0049's `app` level, RFC-0098 D4, phase-445 W6). This is what
//!    replaced the `[env]` block a leaf used to hand-write in its own
//!    `.cargo/config.toml`.
//!
//! Plus the in-repo `[patch.crates-io]` rows the graph names registry-style.
//!
//! No row is written with `force`, so the ladder's top rung — a value the
//! calling lane exports — still outranks all five.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use eyre::{Result, WrapErr, bail, eyre};
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

/// Layer 3.5 — what an image's declared TRANSPORT implies (phase-445 W6).
///
/// `[image.<id>] transport = "serial"` is a link-set choice, and two build
/// scripts already ask for it in their own spelling:
///
/// * `ZPICO_NO_SMOLTCP=1` — `nros-zpico-build`'s runner skips the smoltcp glue
///   and defines `ZPICO_SERIAL` instead; without it a bare-metal serial link
///   does not resolve (`smoltcp_init` / `smoltcp_cleanup`).
/// * `NROS_LINK_IP=0` — `nros-zpico-build` and `nros-rmw-xrce-cffi` drop the
///   vendor TCP/UDP link C, which `--gc-sections` then removes entirely.
///
/// Three mps2 leaves hand-wrote BOTH into their `.cargo/config.toml` `[env]`,
/// which is two restatements of one fact — and the same rule already exists one
/// layer up, as `PlanBuildOptions::drops_ip_link` ("every declared transport is
/// serial or CAN ⇒ no IP link"). Stating the transport once and deriving the
/// two knobs is that rule, applied to a leaf.
///
/// IMPLIES, never forces: these go in the `[env]` table with everything else,
/// so a lane that exports `NROS_LINK_IP=1` still wins (RFC-0086 D2's `imply`
/// strength, and the reason an image may still name either knob explicitly in
/// its `[image.<id>] env`, which is applied AFTER this).
fn transport_implications(decl: &LeafSystem, env: &mut BTreeMap<String, String>) {
    if decl.network.transport.as_deref() != Some("serial") {
        return;
    }
    env.insert("ZPICO_NO_SMOLTCP".to_string(), "1".to_string());
    env.insert("NROS_LINK_IP".to_string(), "0".to_string());
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

    // Layer 3.5 — what the image's declared transport implies.
    transport_implications(&img.decl, &mut env);

    // phase-454 W4 (RFC-0100 D4) — the sizing descriptor, written HERE because
    // this is the one place that has all three of its inputs at once: the
    // leaf's inventories, the board facts just resolved above (which carry
    // `NROS_BOARD_TOML`, and therefore the `[board.knobs.memory]` rung), and
    // the image's declared backend.
    //
    // The variable below names a PATH and nothing watches the VARIABLE — issue
    // 0491. The rebuild edge is on the file's CONTENT, in
    // `nros_sizing_descriptor::load_for_build_script`.
    let descriptor = crate::sizing_descriptor::write_for_leaf(&img, &path_env, who)
        .map_err(|e| eyre!("{}: sizing descriptor: {e}", leaf.display()))?;
    // phase-454 W6.c — the four CycloneDDS facts that cannot be read from the
    // file where they are needed. A `cc::Build` compiling `descriptors.cpp` has
    // only a compile line, so the STATED ones ride the `[env]` table as well; a
    // refused or absent one emits no row and the consumer keeps its default
    // (RFC-0100 D6). Inserted BEFORE layer 4, so an image that states one of
    // these in `[image.<id>] env` still outranks the derivation.
    for (k, v) in descriptor.cyclonedds_env() {
        path_env.remove(&k);
        env.insert(k, v);
    }
    path_env.insert(
        nros_sizing_descriptor::DESCRIPTOR_ENV.to_string(),
        descriptor.path,
    );

    // Layer 4 — the APP rung: what this IMAGE states (`[image.<id>] env`).
    // Last, so it outranks the board and the implications above and is
    // outranked only by the calling environment.
    for (k, v) in &img.decl.env {
        path_env.remove(k);
        env.insert(k.clone(), v.clone());
    }

    // No absolute path in the hint: the file is otherwise checkout-independent,
    // and the command is written relative to the directory it names.
    let (_cwd, args) = build_command(&img);
    let hint = format!(
        "(from the directory ABOVE the package, so the package's own\n\
         `.cargo/config.toml` is not read a second time)\n\
         cargo {}\n\
         \n\
         or, from INSIDE the package, with no flags at all:\n\
         cargo build --release\n\
         because `nros sync` wires this file into the leaf's own (gitignored)\n\
         `.cargo/config.toml` as an `include` — issue 1381. Do NOT combine the\n\
         two: cargo JOINS `rustflags` arrays across config files, so reading\n\
         this file twice links `link.x` twice (`region 'FLASH' already\n\
         defined`).",
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
    wire_settings_into_leaf_config(leaf, &img.image_id, who)?;
    Ok(Some(img))
}

/// The `include` a leaf's own `.cargo/config.toml` carries so that a plain
/// `cargo build` RUN INSIDE THE LEAF reads the generated settings.
///
/// Relative to the config file's directory (`<leaf>/.cargo/`), which is how
/// cargo resolves an `include` entry.
fn settings_include_rel(image_id: &str) -> String {
    format!("../build/{image_id}/{}", cargo_config::FILE_NAME)
}

/// Issue 1381 — make the leaf STANDALONE: `nros sync` + `cargo build` inside it.
///
/// The generated settings file is a `--config` file, and cargo does not
/// discover one; it discovers `.cargo/config.toml` upward from the WORKING
/// DIRECTORY. So a leaf that states its board in `system.toml` and carries no
/// `[build] target` of its own built for the HOST under a bare `cargo build` —
/// which for a `#![no_std]` firmware leaf cannot work at all (measured: the
/// `nros::main!` hosted arm, then `unwinding panics are not supported without
/// std`, then no `main` symbol to link). The fixture lane never saw it because
/// it builds from the directory above with `--config` NAMED.
///
/// One `include` line closes that, and it costs nothing elsewhere: the lane's
/// working directory is ABOVE the leaf, so the file this writes is not on
/// cargo's discovery path there and cannot double the board's `rustflags`
/// (phase-445 W6's `region 'FLASH' already defined`). The file is gitignored
/// (`**/.cargo/config.toml`), sync already writes it for the central patch
/// `include`, and `render_patch_config` preserves entries it does not own — so
/// this line survives every later sync without being re-added.
///
/// SKIPPED for a leaf whose config already states `[build] target` or any
/// `[target.*] rustflags`: there the authored file and the generated one would
/// both be read from inside the leaf, and cargo JOINS `rustflags`. No in-tree
/// leaf is in that shape (zero of the 23 tracked `.cargo/config.toml` sit
/// beside a `system.toml`), but an out-of-tree consumer may be, and silently
/// doubling its link script would be worse than leaving it on the `--config`
/// road it is already using.
fn wire_settings_into_leaf_config(leaf: &Path, image_id: &str, who: &str) -> Result<()> {
    use toml_edit::{DocumentMut, Value, value};

    let cfg_dir = leaf.join(".cargo");
    let cfg = cfg_dir.join("config.toml");
    let existing = std::fs::read_to_string(&cfg).unwrap_or_default();
    let mut doc: DocumentMut = existing
        .parse()
        .wrap_err_with(|| format!("{who}: parse {}", cfg.display()))?;

    if states_its_own_build_flags(&doc) {
        println!(
            "{who}: {} states its own `[build] target` / `rustflags`, so the generated \
             settings stay on the `--config` road (issue 1381)",
            cfg.display()
        );
        return Ok(());
    }

    let want = settings_include_rel(image_id);
    let current: Vec<String> = doc
        .as_table()
        .get("include")
        .and_then(|i| i.as_value())
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    // A leaf that switches `[image.<id>]` would otherwise keep an include of the
    // PREVIOUS image's settings file, which sync no longer writes — and a
    // missing `include` target is a HARD cargo error during manifest parse, not
    // a silent drop (issue 0463). So the whole family is re-decided, never just
    // appended to.
    let is_ours = |s: &str| s.starts_with("../build/") && s.ends_with(cargo_config::FILE_NAME);
    let mut desired: Vec<String> = current.iter().filter(|s| !is_ours(s)).cloned().collect();
    desired.push(want);
    if current == desired {
        // Never rewrite an identical file: the mtime alone re-stales every
        // fixture keyed on this leaf (the rule `render_patch_config` records).
        return Ok(());
    }

    let item = doc
        .as_table_mut()
        .entry("include")
        .or_insert_with(|| value(toml_edit::Array::new()));
    let arr = item
        .as_value_mut()
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| eyre!("{who}: `include` in {} is not an array", cfg.display()))?;
    arr.retain(|v| v.as_str().is_none_or(|s| !is_ours(s)));
    arr.push(&desired[desired.len() - 1]);

    std::fs::create_dir_all(&cfg_dir)
        .wrap_err_with(|| format!("{who}: mkdir {}", cfg_dir.display()))?;
    crate::atomic_file::atomic_write(&cfg, &doc.to_string())?;
    Ok(())
}

/// Does this `.cargo/config.toml` already carry build flags the generated
/// settings file would duplicate? See [`wire_settings_into_leaf_config`].
fn states_its_own_build_flags(doc: &toml_edit::DocumentMut) -> bool {
    if doc
        .as_table()
        .get("build")
        .and_then(|b| b.as_table_like())
        .is_some_and(|b| b.get("target").is_some())
    {
        return true;
    }
    doc.as_table()
        .get("target")
        .and_then(|t| t.as_table_like())
        .is_some_and(|t| {
            t.iter().any(|(_, v)| {
                v.as_table_like()
                    .is_some_and(|tt| tt.get("rustflags").is_some())
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `LeafSystem` with only the fields a test cares about set.
    fn decl(transport: Option<&str>, env: &[(&str, &str)]) -> LeafSystem {
        LeafSystem {
            origin: leaf_system::Origin::SystemToml("/w/talker/system.toml".into()),
            image: Some("native".into()),
            board: Some("native".into()),
            rmw: None,
            network: leaf_system::Network {
                transport: transport.map(str::to_string),
                ..Default::default()
            },
            components: Vec::new(),
            env: env
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            // Issue 1142's `[system] features`. ABSENT MEANS NONE, and no test
            // in here asks about the runtime service families, so the helper
            // states the empty claim once instead of per test.
            features: Vec::new(),
        }
    }

    #[test]
    fn the_settings_file_sits_under_the_leafs_build_dir_per_image() {
        assert_eq!(
            settings_path(Path::new("/w/talker"), "native"),
            PathBuf::from("/w/talker/build/native/nros-cargo.toml")
        );
    }

    /// phase-445 W6 — one declared transport, both knobs the build scripts ask
    /// for. Three mps2 leaves used to hand-write the pair.
    #[test]
    fn a_serial_transport_implies_the_two_link_knobs() {
        let mut env = BTreeMap::new();
        transport_implications(&decl(Some("serial"), &[]), &mut env);
        assert_eq!(env["ZPICO_NO_SMOLTCP"], "1");
        assert_eq!(env["NROS_LINK_IP"], "0");
    }

    /// IMPLIES, not selects: an image that names either knob itself still wins,
    /// because the app rung is applied after the implication (RFC-0086 D2).
    #[test]
    fn an_image_that_states_a_link_knob_overrides_the_implication() {
        let d = decl(Some("serial"), &[("NROS_LINK_IP", "1")]);
        let mut env = BTreeMap::new();
        transport_implications(&d, &mut env);
        for (k, v) in &d.env {
            env.insert(k.clone(), v.clone());
        }
        assert_eq!(env["NROS_LINK_IP"], "1", "the image's own row wins");
        assert_eq!(env["ZPICO_NO_SMOLTCP"], "1", "the other half still applies");
    }

    /// Absent or IP-bearing transport implies nothing — a leaf that says
    /// nothing keeps the board's default link set.
    #[test]
    fn a_non_serial_transport_implies_nothing() {
        for t in [None, Some("udp"), Some("tcp")] {
            let mut env = BTreeMap::new();
            transport_implications(&decl(t, &[]), &mut env);
            assert!(env.is_empty(), "{t:?} implied {env:?}");
        }
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
            decl: decl(None, &[]),
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

    /// Issue 1381 — the leaf's own `.cargo/config.toml` gains the `include`, so
    /// `cargo build` INSIDE the leaf reads the generated settings.
    ///
    /// This is what makes a single-package leaf standalone. Without it the
    /// board's triple lives only in a `--config` file, which cargo does not
    /// discover, so a bare `cargo build` in a bare-metal leaf compiled for the
    /// HOST and could not succeed on any path.
    #[test]
    fn the_leaf_config_gains_an_include_of_the_generated_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = tmp.path();
        wire_settings_into_leaf_config(leaf, "mps2-an385-baremetal", "sync").unwrap();
        let text = std::fs::read_to_string(leaf.join(".cargo/config.toml")).unwrap();
        assert!(
            text.contains("../build/mps2-an385-baremetal/nros-cargo.toml"),
            "{text}"
        );
    }

    /// Idempotent, and the second run does not TOUCH the file: an identical
    /// rewrite still moves the mtime, which re-stales every fixture keyed on
    /// this leaf (the rule `render_patch_config` records for the same file).
    #[test]
    fn a_second_sync_neither_duplicates_the_include_nor_touches_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = tmp.path();
        wire_settings_into_leaf_config(leaf, "native", "sync").unwrap();
        let cfg = leaf.join(".cargo/config.toml");
        let first = std::fs::read_to_string(&cfg).unwrap();
        let mtime = std::fs::metadata(&cfg).unwrap().modified().unwrap();

        wire_settings_into_leaf_config(leaf, "native", "sync").unwrap();
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), first);
        assert_eq!(std::fs::metadata(&cfg).unwrap().modified().unwrap(), mtime);
        assert_eq!(first.matches("nros-cargo.toml").count(), 1, "{first}");
    }

    /// An `include` a user or an earlier sync put there survives — sync's own
    /// central-patch entry among them.
    #[test]
    fn an_existing_include_entry_is_preserved() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = tmp.path();
        std::fs::create_dir_all(leaf.join(".cargo")).unwrap();
        std::fs::write(
            leaf.join(".cargo/config.toml"),
            "include = [\"../../nros-patch.toml\"]\n",
        )
        .unwrap();
        wire_settings_into_leaf_config(leaf, "native", "sync").unwrap();
        let text = std::fs::read_to_string(leaf.join(".cargo/config.toml")).unwrap();
        assert!(text.contains("../../nros-patch.toml"), "{text}");
        assert!(text.contains("../build/native/nros-cargo.toml"), "{text}");
    }

    /// A leaf that states its OWN `[build] target` / `rustflags` keeps the
    /// `--config` road: cargo JOINS `rustflags` arrays across config files, so
    /// reading the board's link group twice is `region 'FLASH' already defined`
    /// (measured by phase-445 W6, which is why every `examples/**/.cargo/` went).
    #[test]
    fn a_leaf_with_authored_build_flags_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = tmp.path();
        std::fs::create_dir_all(leaf.join(".cargo")).unwrap();
        let authored = "[build]\ntarget = \"thumbv7m-none-eabi\"\n";
        std::fs::write(leaf.join(".cargo/config.toml"), authored).unwrap();
        wire_settings_into_leaf_config(leaf, "native", "sync").unwrap();
        assert_eq!(
            std::fs::read_to_string(leaf.join(".cargo/config.toml")).unwrap(),
            authored
        );
    }

    /// The same refusal for the other half of the hazard — a leaf that carries
    /// only `[target.<triple>] rustflags`.
    #[test]
    fn authored_rustflags_alone_also_keep_the_config_road() {
        let doc: toml_edit::DocumentMut =
            "[target.thumbv7m-none-eabi]\nrustflags = [\"-C\", \"link-arg=-Tlink.x\"]\n"
                .parse()
                .unwrap();
        assert!(states_its_own_build_flags(&doc));
        let clean: toml_edit::DocumentMut =
            "[target.thumbv7m-none-eabi]\nrunner = \"qemu-system-arm\"\n"
                .parse()
                .unwrap();
        assert!(!states_its_own_build_flags(&clean));
    }

    /// A leaf that changes `[image.<id>]` loses the PREVIOUS image's include.
    ///
    /// Leaving it would point at a file sync no longer writes, and a missing
    /// `include` target is a HARD cargo error during manifest parse — the leaf
    /// stops being READABLE, not merely buildable (issue 0463).
    #[test]
    fn switching_image_replaces_the_stale_settings_include() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = tmp.path();
        wire_settings_into_leaf_config(leaf, "native", "sync").unwrap();
        wire_settings_into_leaf_config(leaf, "mps2-an385-baremetal", "sync").unwrap();
        let text = std::fs::read_to_string(leaf.join(".cargo/config.toml")).unwrap();
        assert!(
            !text.contains("../build/native/nros-cargo.toml"),
            "the stale include survived: {text}"
        );
        assert!(
            text.contains("../build/mps2-an385-baremetal/nros-cargo.toml"),
            "{text}"
        );
        assert_eq!(text.matches("nros-cargo.toml").count(), 1, "{text}");
    }
}
