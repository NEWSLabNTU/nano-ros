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
//!    and its `[knobs]`, reached through `NROS_BOARD_TOML`;
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

/// The `[package] name` of `leaf` when it is a single-package leaf candidate —
/// a `system.toml` beside a `[package]` manifest — else `None`. The cheap half
/// of [`resolve`], for a caller that must know it is looking at a leaf before
/// it can say which image it means.
pub fn leaf_package(leaf: &Path) -> Option<String> {
    if !leaf.join(leaf_system::SYSTEM_TOML).is_file() {
        return None;
    }
    package_name(leaf)
}

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
    resolve_for(leaf, nano_ros_root, Driver::Cargo, None)
}

/// [`resolve`] for ONE named image of the leaf (issue 1305). A cargo leaf may
/// declare several images — one per RMW and ROS edition, `examples/native/
/// rust/talker` has nine — and `nros sync --image <id>` / `nros build <id>`
/// build the one named. `None` keeps the leaf's own rule (one image, or
/// `[system] default_images`); naming an image the file does not declare is
/// an error listing the ones it does.
pub fn resolve_image(
    leaf: &Path,
    nano_ros_root: &Path,
    image: Option<&str>,
) -> Result<Option<LeafImage>> {
    resolve_for(leaf, nano_ros_root, Driver::Cargo, image)
}

/// Issue 1662 -- the same resolution for a leaf whose board's road is WEST: a
/// standalone Zephyr Rust leaf (`examples/zephyr/rust/*`), a `[package]`
/// manifest with a `system.toml` naming a Zephyr board. Its image is built by
/// west, not by this module's settings file, but it sizes from the same
/// inventory and the same descriptor writer, so the leaf is resolved once,
/// here. `None` for anything else, exactly as [`resolve`].
pub fn resolve_west(leaf: &Path, nano_ros_root: &Path) -> Result<Option<LeafImage>> {
    resolve_for(leaf, nano_ros_root, Driver::West, None)
}

/// [`resolve_west`] for ONE named image of the leaf (phase-481 W3). A Zephyr
/// leaf states one image per RMW and the configure builds the one
/// `-DNROS_IMAGE=<id>` names, so a reader that sizes that build must read THAT
/// image -- its `rmw` decides which backend the descriptor speaks for. `None`
/// keeps the leaf's own rule (one image, or `[system] default_images`).
pub fn resolve_west_image(
    leaf: &Path,
    nano_ros_root: &Path,
    image: Option<&str>,
) -> Result<Option<LeafImage>> {
    resolve_for(leaf, nano_ros_root, Driver::West, image)
}

fn resolve_for(
    leaf: &Path,
    nano_ros_root: &Path,
    want: Driver,
    image: Option<&str>,
) -> Result<Option<LeafImage>> {
    if !leaf.join(leaf_system::SYSTEM_TOML).is_file() {
        return Ok(None);
    }
    let Some(package) = package_name(leaf) else {
        return Ok(None);
    };
    let decl = match image {
        Some(id) => leaf_system::read_image(leaf, id),
        None => leaf_system::read(leaf),
    };
    let Some(decl) = decl.map_err(|e| eyre!(e))? else {
        return Ok(None);
    };
    leaf_image_from_decl(leaf, nano_ros_root, want, package, decl)
}

/// Issue 1305 — a workspace ENTRY package with no `system.toml` of its own,
/// whose image is the one a bringup CLAIMS for it (`[image.<id>] entry =
/// "<pkg>"`, or the image the builder would generate that entry for —
/// [`leaf_system::for_entry`], the reader `nros::main!` uses for the same
/// entry). `None` when `entry` is not a package, states its own deployment
/// (that is [`resolve`]'s), or no image of `bringup` claims it.
pub fn resolve_claimed_entry(
    entry: &Path,
    bringup: &Path,
    nano_ros_root: &Path,
) -> Result<Option<LeafImage>> {
    if entry.join(leaf_system::SYSTEM_TOML).is_file() {
        return Ok(None);
    }
    let Some(package) = package_name(entry) else {
        return Ok(None);
    };
    let Some(decl) = leaf_system::for_entry(entry, &package, bringup).map_err(|e| eyre!(e))? else {
        return Ok(None);
    };
    leaf_image_from_decl(entry, nano_ros_root, Driver::Cargo, package, decl)
}

/// The tail [`resolve_for`] and [`resolve_claimed_entry`] share: the image's
/// board through the catalog, and the road check.
fn leaf_image_from_decl(
    leaf: &Path,
    nano_ros_root: &Path,
    want: Driver,
    package: String,
    decl: LeafSystem,
) -> Result<Option<LeafImage>> {
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
            "{}: {} under {}",
            decl.origin_path().display(),
            crate::cmd::board_facts::unresolved_board(&board),
            nano_ros_root.display()
        );
    };
    let platform = d.platform.kebab().to_string();
    // A board with no road is not a leaf image either, and the workspace road
    // is where its refusal gets reported with the image named.
    if plan::driver_for_board(&platform, d.entry_kind, false) != Ok(want) {
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

/// Layer 3 — the board facts for `board`, resolved from the `system.toml` in
/// `dir` by the one resolver every road uses (`nros ws board-facts`), split
/// into plain rows and path rows.
///
/// Both cargo roads call this: the single-package leaf road here, and the
/// workspace road in `cmd::build` with the bringup's directory. Issue 1811's
/// last FreeRTOS red was the workspace road skipping it: with no
/// `NROS_BOARD_TOML`, `nros-board-freertos` fell back to the platform network
/// default (`192.0.3.10`) while QEMU now runs SLIRP (`10.0.2.x`), so the
/// image booted and never reached its router.
pub(crate) fn board_fact_layer(
    dir: &Path,
    nano_ros_root: &Path,
    board: &str,
    env: &mut BTreeMap<String, String>,
    path_env: &mut BTreeMap<String, PathBuf>,
) -> Result<()> {
    let facts = crate::cmd::board_facts::resolve(dir, nano_ros_root, None, Some(board), &|k| {
        std::env::var(k).ok()
    })
    .map_err(|e| eyre!("{}: board facts for `{board}`: {e}", dir.display()))?;
    for (k, v) in facts {
        if is_path_value(&v) {
            env.remove(&k);
            path_env.insert(k, PathBuf::from(v));
        } else {
            path_env.remove(&k);
            env.insert(k, v);
        }
    }
    Ok(())
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
pub(crate) fn transport_implications(decl: &LeafSystem, env: &mut BTreeMap<String, String>) {
    transport_implications_of(decl.network.transport.as_deref(), env);
}

/// The rule [`transport_implications`] applies, keyed on the transport alone,
/// so a WORKSPACE image (`[image.<id>]` in a bringup, which has no
/// [`LeafSystem`]) reads it through the same spelling (phase-481 W4). The
/// cargo workspace road carried a second hand-written copy of it until then.
pub(crate) fn transport_implications_of(
    transport: Option<&str>,
    env: &mut BTreeMap<String, String>,
) {
    if transport != Some("serial") {
        return;
    }
    env.insert("ZPICO_NO_SMOLTCP".to_string(), "1".to_string());
    env.insert("NROS_LINK_IP".to_string(), "0".to_string());
}

/// Issue 1712 — the two layers an IMAGE states about itself, in ladder order:
/// what its declared transport implies, then its `[image.<id>] env` (the
/// RFC-0049 APP rung), which outranks the implication key by key.
///
/// [`write`] applies the same two layers to this road's settings file, with
/// the sizing descriptor's rows between them; `nros ws leaf-system
/// --image-env-out` hands exactly these rows to the CMAKE road, which has no
/// settings file of its own. One composition, so a C leaf and a Rust leaf
/// cannot read one `system.toml` two ways.
pub fn image_layers(decl: &LeafSystem) -> Result<BTreeMap<String, String>> {
    let mut env = BTreeMap::new();
    transport_implications(decl, &mut env);
    env.extend(leaf_app_rung(decl)?);
    Ok(env)
}

/// The APP rung of a leaf's image alone — its `[image.<id>.knobs]` rendered,
/// beside its `[image.<id>] env` rows ([`app_rung`]).
pub fn leaf_app_rung(decl: &LeafSystem) -> Result<BTreeMap<String, String>> {
    app_rung(
        Some(decl.image.as_deref().unwrap_or("<id>")),
        &decl.knobs,
        &decl.env,
    )
}

/// phase-484 W8 (RFC-0103 D6) — the APP rung's rows: `[image.<id>.knobs]`
/// rendered to each knob's env spelling, beside the `[image.<id>] env` rows.
/// ONE carrier — the rows land wherever env rows always have (the cargo
/// `[env]`, the cmake image-env file, the Zephyr `Kconfig.from-system-toml`),
/// so a knob stated either way reaches the build identically.
///
/// Refused: a key the registry does not hold, a value its row's type or
/// range does not admit, a retired env spelling, and an env row naming a
/// registry knob — release 0.6.0 warned about that last one and named the
/// knobs spelling; the error now does (`image` is `None` for a coordinate
/// digest, which re-reads rows another call writes, and is spelled `<id>`).
pub fn app_rung(
    image: Option<&str>,
    knobs: &BTreeMap<String, nros_orchestration_ir::leaf_system::KnobValue>,
    env: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    refuse_retired_env_keys(env)?;
    let id = image.unwrap_or("<id>");
    let mut rows = BTreeMap::new();
    let mut errors = Vec::new();
    for (key, value) in knobs {
        match knob_env_row(id, key, value) {
            Ok((row, v)) => {
                rows.insert(row.env.to_string(), v);
            }
            Err(e) => errors.push(e),
        }
    }
    if !errors.is_empty() {
        return Err(eyre!("[image.{id}.knobs]:\n  {}", errors.join("\n  ")));
    }
    let refused: Vec<String> = env
        .keys()
        .filter_map(|k| {
            registry_knob_of_env_row(k).map(|row| row.image_env_refusal(id).replace(row.env, k))
        })
        .collect();
    if !refused.is_empty() {
        return Err(eyre!("{}", refused.join("\n")));
    }
    rows.extend(env.iter().map(|(k, v)| (k.clone(), v.clone())));
    Ok(rows)
}

/// The registry knob an `[image.<id>] env` row's KEY names: its canonical env
/// spelling, or the Kconfig symbol a west build renders the row to
/// (`NROS_MAX_PUBLISHERS` -> `CONFIG_NROS_MAX_PUBLISHERS`, the zenoh pool's
/// symbol). `None` for an application's own variable. A retired spelling is
/// refused before this is asked.
pub fn registry_knob_of_env_row(key: &str) -> Option<&'static nros_zephyr_build::KnobRow> {
    let symbol = format!("CONFIG_{key}");
    nros_zephyr_build::KNOBS
        .iter()
        .find(|r| r.env == key || r.kconfig == symbol)
}

/// One `[image.<id>.knobs]` key → its registry row and the env value it
/// renders to: an int as decimal, a bool as `1`/`0`, a string (a `string`
/// knob, or one of an `int` knob's `names`) verbatim — the spellings the
/// env rows it replaces were written in, so a migrated row is byte-identical.
fn knob_env_row(
    id: &str,
    key: &str,
    value: &nros_orchestration_ir::leaf_system::KnobValue,
) -> std::result::Result<(&'static nros_zephyr_build::KnobRow, String), String> {
    use nros_orchestration_ir::leaf_system::KnobValue;
    let (group, name) = key.rsplit_once('.').unwrap_or(("", key));
    let Some(row) = nros_zephyr_build::knob_by_image_key(key) else {
        let hint = nros_zephyr_build::KNOBS
            .iter()
            .find(|r| r.image_key.rsplit_once('.').map(|(_, n)| n) == Some(name))
            .map(|r| {
                let (g, n) = r.image_group_name();
                format!(" (did you mean `[image.{id}.knobs.{g}] {n}`?)")
            })
            .unwrap_or_default();
        return Err(format!(
            "`[image.{id}.knobs.{group}] {name}` names no knob in the registry \
             (packages/tooling/nros-zephyr-build/knobs.toml){hint}"
        ));
    };
    let bad = |want: &str| {
        format!(
            "`[image.{id}.knobs.{group}] {name} = {value}`: {} is {want}",
            row.env
        )
    };
    let rendered = match (row.ty, value) {
        (Some("int" | "hex") | None, KnobValue::Int(n)) => {
            if let Some((lo, hi)) = row.range
                && !(lo..=hi).contains(n)
            {
                return Err(bad(&format!("in {lo}..={hi}")));
            }
            n.to_string()
        }
        (Some("int"), KnobValue::Str(s)) if !row.names.is_empty() => {
            if !row.names.iter().any(|n| n.eq_ignore_ascii_case(s)) {
                return Err(bad(&format!(
                    "an integer or one of {}",
                    row.names.join("/")
                )));
            }
            s.clone()
        }
        (Some("int"), _) => return Err(bad("an integer")),
        (Some("hex"), _) => return Err(bad("an integer")),
        (Some("bool") | None, KnobValue::Bool(b)) => if *b { "1" } else { "0" }.to_string(),
        (Some("bool"), _) => return Err(bad("a bool (true / false)")),
        (Some("string") | None, KnobValue::Str(s)) => s.clone(),
        (Some(_), _) => return Err(bad("a string")),
    };
    Ok((row, rendered))
}

/// phase-484 W8 (RFC-0103 D2b) — an `[image.<id>] env` row keyed by a RETIRED
/// knob spelling (`ZPICO_MAX_QUERYABLES`) is refused, naming the canonical one
/// (`NROS_ZENOH_MAX_QUERYABLES`), in the registry's words. Release 0.5.0-nros2
/// carried such a row under its canonical key with a deprecation; carrying it
/// now would keep the retired name working through `system.toml` alone.
///
/// Every key that names a retired spelling is listed, so one edit fixes them
/// all. A key the registry does not hold is carried as written.
pub fn refuse_retired_env_keys(env: &BTreeMap<String, String>) -> Result<()> {
    let refusals: Vec<String> = env
        .keys()
        .filter_map(|k| nros_zephyr_build::retired_spelling_refusal(k))
        .collect();
    if refusals.is_empty() {
        return Ok(());
    }
    Err(eyre::eyre!(
        "[image.<id>] env:\n  {}",
        refusals.join("\n  ")
    ))
}

/// phase-481 W4 (RFC-0098 D12) — [`image_layers`] for a WORKSPACE image: what
/// its declared transport implies, then its `[image.<id>] env`. Both workspace
/// roads read it — the cargo road into the image's `nros-cargo.toml`, the
/// cmake road into its configure's image-env file and, through
/// `cmake_coordinate`, into WHICH configure it shares.
///
/// `image_id` names the image in a deprecation warning and an error; `None`
/// reads the rows quietly (a coordinate digest).
pub fn image_block_layers(
    image_id: Option<&str>,
    image: &crate::orchestration::image::ImageBlock,
) -> Result<BTreeMap<String, String>> {
    let mut env = BTreeMap::new();
    transport_implications_of(image.transport.as_deref(), &mut env);
    env.extend(app_rung(image_id, &image.knobs.0, &image.env)?);
    Ok(env)
}

/// Issue 1305 / RFC-0098 D6 — does the leaf manifest depend on its generated
/// selection package (`<pkg>_nros_selection`, in any dependency table)?
///
/// That dependency is what makes a leaf MIGRATED: its image-only dependencies
/// live in the generated package, not in its own `Cargo.toml`.
#[must_use]
pub fn depends_on_selection(leaf: &Path, package: &str) -> bool {
    let want = leaf_system::selection_crate_name(package);
    let Some(doc) = std::fs::read_to_string(leaf.join("Cargo.toml"))
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok())
    else {
        return false;
    };
    std::iter::once(&doc)
        .chain(
            doc.get("target")
                .and_then(|t| t.as_table())
                .into_iter()
                .flat_map(|t| t.values().filter_map(|v| v.as_table())),
        )
        .filter_map(|scope| scope.get("dependencies").and_then(|d| d.as_table()))
        .any(|deps| deps.contains_key(&want))
}

/// Issue 1305 / RFC-0098 D6 — write the leaf's generated selection package
/// (`<leaf>/build/nros/selection`) for its image, when the leaf depends on it.
///
/// Returns `Ok(None)` for a leaf that is not on this road ([`resolve`]) or not
/// migrated (it still names its board crate by hand, and
/// [`check_board_crate_dependency`] guards it instead). Both `nros sync`, BEFORE
/// its first cargo invocation (the metadata probe loads the leaf manifest, and
/// cargo refuses a manifest whose path dependency is missing), and `nros build`
/// ([`write`]) call it, so a board switch followed by either one regenerates it.
pub fn write_selection(
    leaf: &Path,
    nano_ros_root: &Path,
    image: Option<&str>,
) -> Result<Option<crate::orchestration::facade::Facade>> {
    let Some(img) = resolve_image(leaf, nano_ros_root, image)? else {
        return Ok(None);
    };
    if !depends_on_selection(&img.leaf, &img.package) {
        return Ok(None);
    }
    write_selection_for(&img, nano_ros_root).map(Some)
}

/// [`write_selection`] for a workspace ENTRY whose image a bringup claims
/// ([`resolve_claimed_entry`]): `nros sync`'s workspace arm asks it of every
/// package with no `system.toml` of its own, so an entry the bringup names
/// (`[image.freertos] entry = "firmware"`) gets its board crate from that
/// image exactly as a single-package leaf gets it from its own — and the
/// image's network identity stays in the ONE place `nros::main!` reads it.
pub fn write_entry_selection(
    entry: &Path,
    bringup: &Path,
    nano_ros_root: &Path,
) -> Result<Option<crate::orchestration::facade::Facade>> {
    let Some(img) = resolve_claimed_entry(entry, bringup, nano_ros_root)? else {
        return Ok(None);
    };
    if !depends_on_selection(&img.leaf, &img.package) {
        return Ok(None);
    }
    write_selection_for(&img, nano_ros_root).map(Some)
}

fn write_selection_for(
    img: &LeafImage,
    nano_ros_root: &Path,
) -> Result<crate::orchestration::facade::Facade> {
    let catalog = BoardCatalog::load(nano_ros_root)
        .map_err(|e| eyre!("board catalog under {}: {e}", nano_ros_root.display()))?;
    let Some(d) = crate::cmd::board_facts::resolve_board(&catalog, &img.board) else {
        bail!("board `{}` is claimed by no board descriptor", img.board);
    };
    let candidates = [
        img.image_id.as_str(),
        img.board.as_str(),
        img.platform.as_str(),
    ];
    let facts = crate::builder::entry::BoardFacts::from_descriptor_for(d, &candidates);
    let sys = system_toml(img)?;
    let spec = crate::orchestration::facade::LeafSelectionSpec {
        package: &img.package,
        leaf: &img.leaf,
        image_id: &img.image_id,
        rmw: img.decl.rmw.as_deref(),
        transport: img.decl.network.transport.as_deref(),
        board: &facts,
        candidates: &candidates,
        nano_ros_root,
    };
    crate::orchestration::facade::write_leaf_selection(&spec, &sys)
}

/// The leaf's `system.toml` through the typed bringup schema, so every typo
/// guard `nros sync` applies to a bringup applies to a leaf too.
fn system_toml(img: &LeafImage) -> Result<crate::orchestration::cargo_metadata_schema::SystemToml> {
    let origin = img.decl.origin_path();
    let mut table: toml::Table = std::fs::read_to_string(origin)
        .wrap_err_with(|| format!("reading {}", origin.display()))?
        .parse()
        .wrap_err_with(|| format!("parsing {}", origin.display()))?;
    // The bringup schema requires `[system] domain_id`; a leaf may leave it to
    // the runtime (`leaf_system` does), and nothing selected here reads it.
    if let Some(system) = table.get_mut("system").and_then(|s| s.as_table_mut()) {
        system.entry("domain_id").or_insert(toml::Value::Integer(0));
    }
    table
        .try_into()
        .wrap_err_with(|| format!("parsing {}", origin.display()))
}

/// The ROS edition `img` builds for — `[image.<id>]` > `[image_defaults]` >
/// `[system]` > humble ([`SystemToml::image_ros_edition`], issue 1305).
///
/// [`SystemToml::image_ros_edition`]: crate::orchestration::cargo_metadata_schema::SystemToml::image_ros_edition
pub fn image_ros_edition(img: &LeafImage) -> Result<rosidl_codegen::RosEdition> {
    system_toml(img)?.image_ros_edition(Some(img.image_id.as_str()))
}

/// The board crates `packages/boards/board-support.toml` marks
/// `tier = "scaffold"` — explicitly not supported (the dormant ESP32, issue
/// 1525). Issue 1305's walk derives its "unsupported board" exemption from
/// here rather than naming a path.
#[cfg(test)]
fn scaffold_board_crates(root: &Path) -> Vec<String> {
    let path = root.join("packages/boards/board-support.toml");
    let doc: toml::Table = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .parse()
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    doc.get("board")
        .and_then(|b| b.as_array())
        .into_iter()
        .flatten()
        .filter(|b| b.get("tier").and_then(|t| t.as_str()) == Some("scaffold"))
        .filter_map(|b| b.get("crate").and_then(|c| c.as_str()).map(str::to_string))
        .collect()
}

/// The `nros-board-*` crates a manifest names, in any dependency table
/// (target-scoped ones included), by crate (`package = "…"` when renamed).
/// The in-tree walk's question about a manifest with no image (issue 1305).
#[cfg(test)]
fn named_board_crates(doc: &toml::Table) -> Vec<String> {
    let mut out = Vec::new();
    for scope in std::iter::once(doc).chain(
        doc.get("target")
            .and_then(|t| t.as_table())
            .into_iter()
            .flat_map(|t| t.values().filter_map(|v| v.as_table())),
    ) {
        for key in ["dependencies", "build-dependencies", "dev-dependencies"] {
            if let Some(deps) = scope.get(key).and_then(|d| d.as_table()) {
                for (k, v) in deps {
                    let krate = v
                        .get("package")
                        .and_then(|p| p.as_str())
                        .unwrap_or(k.as_str())
                        .replace('_', "-");
                    if krate.starts_with("nros-board-") && !out.contains(&krate) {
                        out.push(krate);
                    }
                }
            }
        }
    }
    out
}

/// Issue 1305 — a single-package Rust leaf's image-only dependencies must
/// agree with its image's board.
///
/// Two shapes, one check, run where `nros sync` and `nros build` both pass:
///
/// * **Migrated** — the leaf depends on its generated `<pkg>_nros_selection`
///   package ([`write_selection`]), which names the image's board crate, RMW,
///   platform and runtime, so a board switch is one line in `system.toml`.
///   Such a leaf must name NO board crate itself: a hand-written one beside
///   the selection is the old manifest coming back, and it is wrong the moment
///   the image's board changes (two boards, two `#[global_allocator]`s, or a
///   backend whose `platform-*` feature clashes). Refused, naming the crate.
/// * **Not migrated** — the leaf still names its board crate by hand (the
///   native leaves, whose RMW / edition is a cargo FEATURE axis; the
///   board-specific test fixtures). `nros::main!` asks for the crate the
///   IMAGE's board maps to while `Cargo.toml` names whichever crate it was
///   written with, so after a board switch sync used to succeed and the build
///   died in the leaf: "cannot find `nros_board_<new>` in the crate root".
///   Refused, naming both crates. The crate is the MACRO's
///   (`entry::macro_board_crate`), because the macro's expansion is what has
///   to compile.
///
/// A leaf that names no board crate and no selection (issue 1815) takes its
/// platform another way and is accepted.
fn check_board_crate_dependency(img: &LeafImage) -> Result<()> {
    let Some(want) = crate::builder::entry::macro_board_crate(&[
        img.image_id.as_str(),
        img.board.as_str(),
        img.platform.as_str(),
    ]) else {
        return Ok(());
    };
    let manifest = img.leaf.join("Cargo.toml");
    let Ok(doc) = std::fs::read_to_string(&manifest)
        .map_err(|e| e.to_string())
        .and_then(|t| t.parse::<toml::Table>().map_err(|e| e.to_string()))
    else {
        return Ok(());
    };
    // A dependency's crate is its `package = "…"` when renamed, else its key.
    // Target-scoped tables count too: an image-only board crate lives under
    // `[target.'cfg(target_os = "none")'.dependencies]` so the node still
    // builds for the host probe (issue 1265).
    let mut tables: Vec<&toml::Table> = Vec::new();
    for scope in std::iter::once(&doc).chain(
        doc.get("target")
            .and_then(|t| t.as_table())
            .into_iter()
            .flat_map(|t| t.values().filter_map(|v| v.as_table())),
    ) {
        for key in ["dependencies", "build-dependencies"] {
            if let Some(deps) = scope.get(key).and_then(|d| d.as_table()) {
                tables.push(deps);
            }
        }
    }
    let mut named: Vec<String> = Vec::new();
    for deps in tables {
        for (k, v) in deps {
            let krate = v
                .get("package")
                .and_then(|p| p.as_str())
                .unwrap_or(k.as_str());
            named.push(krate.replace('_', "-"));
        }
    }
    let boards: Vec<&str> = named
        .iter()
        .map(String::as_str)
        .filter(|n| n.starts_with("nros-board-"))
        .collect();
    // A MIGRATED leaf depends on its generated selection package, which
    // [`write_selection`] writes for the image's board — and names no board
    // crate of its own.
    let selection = leaf_system::selection_crate_name(&img.package).replace('_', "-");
    if named.iter().any(|n| n == &selection) {
        if boards.is_empty() {
            return Ok(());
        }
        bail!(
            "{}: depends on its generated selection package `{}` AND names the board \
             crate `{}`.\n  The selection package carries the image's board crate (and its \
             RMW, platform and runtime), so a migrated leaf names none of them: delete \
             the board dependency from {}. A hand-written board crate beside the \
             selection is wrong as soon as `[image.{}] board` changes (issue 1305, \
             RFC-0098 D6).",
            img.leaf.display(),
            leaf_system::selection_crate_name(&img.package),
            boards.join("`, `"),
            manifest.display(),
            img.image_id,
        );
    }
    if named.iter().any(|n| n == &want) {
        return Ok(());
    }
    // Issue 1815 — a leaf that names NO board crate was not written for some
    // other board, so it is not the stale state this refuses. Such a leaf
    // takes its platform another way (`log-arena-probe` links the POSIX
    // `nros_platform_*` symbols through `nros-platform-cffi` and has its own
    // `main`); refusing it stopped every native fixture build at sync.
    if boards.is_empty() {
        return Ok(());
    }
    let has = format!("`{}`", boards.join("`, `"));
    bail!(
        "{}: image `{}` builds for board `{}`, whose entry needs the crate `{want}`, \
         but {} depends on {has}.\n  Replace the board dependency in {} with `{want}`, \
         using the features that board crate has. Then compare the rest of the leaf's \
         image-only dependencies with an in-tree example for board `{}`: a crate the old \
         board needed can clash with the new one (an RMW backend carrying a `platform-*` \
         feature stops the build with \"Only one platform backend can be selected\").\n  \
         (issue 1305: this leaf still names its image dependencies by hand; a leaf \
         that depends on its generated `<pkg>_nros_selection` package instead gets \
         them from `system.toml`, RFC-0098 D6).",
        img.leaf.display(),
        img.image_id,
        img.board,
        manifest.display(),
        manifest.display(),
        img.board
    )
}

/// Write `<leaf>/build/<image>/nros-cargo.toml`, or return `None` when `leaf`
/// is not a leaf this road builds ([`resolve`]).
///
/// `who` prefixes diagnostics (`sync`, `nros build`).
///
/// `image` names one of the leaf's images (`None`: its own rule — one image,
/// or `[system] default_images`).
pub fn write(
    leaf: &Path,
    nano_ros_root: &Path,
    who: &str,
    image: Option<&str>,
) -> Result<Option<LeafImage>> {
    Ok(write_shared(leaf, nano_ros_root, who, image, &[])?.map(|(img, _)| img))
}

/// [`write`] for a caller that may name ONE cargo target dir for several
/// images (`native_args`: the cargo arguments after `--`).
///
/// Issue 1869 (RFC-0100 D13) gave the WORKSPACE road a build dir per sizing
/// configuration inside such a target dir; the single-package road did not, so
/// every leaf of a fixture group shared one `nros-node` build-script `out/`
/// and the last leaf to build overwrote the others' generated config (issue
/// 1885). With a caller target dir this keys the image's configuration, points
/// the descriptor at its class copy, and returns the class so the caller can
/// pass `build.build-dir`. No caller target dir: nothing changes.
pub fn write_shared(
    leaf: &Path,
    nano_ros_root: &Path,
    who: &str,
    image: Option<&str>,
    native_args: &[String],
) -> Result<Option<(LeafImage, Option<crate::builder::shared_target::Class>)>> {
    let Some(img) = resolve_image(leaf, nano_ros_root, image)? else {
        return Ok(None);
    };
    let leaf = img.leaf.as_path();
    check_board_crate_dependency(&img)?;
    // Issue 1305 — a migrated leaf's selection package, for THIS image's
    // board. Sync wrote it already (before its probe); `nros build` writes it
    // here, so a board switch followed by a build alone also lands.
    write_selection(leaf, nano_ros_root, Some(img.image_id.as_str()))?;

    // Layer 2 — derived pools + facts.
    let mut env = crate::leaf_entity_env::leaf_env(leaf, who).env;
    let mut path_env: BTreeMap<String, PathBuf> = BTreeMap::new();

    // Layer 3 — the board facts, by the one resolver the other lanes use.
    board_fact_layer(leaf, nano_ros_root, &img.board, &mut env, &mut path_env)?;

    // Layer 3.5 — what the image's declared transport implies.
    transport_implications(&img.decl, &mut env);

    // phase-454 W4 (RFC-0100 D4) — the sizing descriptor, written HERE because
    // this is the one place that has all three of its inputs at once: the
    // leaf's inventories, the board facts just resolved above (which carry
    // `NROS_BOARD_TOML`, and therefore the `[knobs.memory]` rung), and
    // the image's declared backend.
    //
    // The variable below names a PATH and nothing watches the VARIABLE — issue
    // 0491. The rebuild edge is on the file's CONTENT, in
    // `nros_sizing_descriptor::load_for_build_script`.
    let descriptor =
        crate::sizing_descriptor::write_for_leaf(&img, &path_env, &leaf.join("build"), who)
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

    // Issue 1512 — the image's connect identity, for a C/C++ app compiled by
    // `cc` inside a cargo-rooted image. `<nros/entry_config.h>` lets an entry
    // bake its locator/domain by defining the macro first; the cmake road does
    // that with `-DNROS_ENTRY_LOCATOR`, and these rows are the cargo road's
    // carrier (`nros_cc_flags::entry_config::bake` reads them). A Rust leaf
    // takes the same facts through `nros::main!`, so nothing reads these there.
    // Absent ⇒ no row ⇒ the header's own ladder, unchanged.
    if let Some(loc) = &img.decl.network.locator {
        env.insert("NROS_ENTRY_LOCATOR".to_string(), loc.clone());
    }
    if let Some(id) = img.decl.network.domain_id {
        env.insert("NROS_ENTRY_DOMAIN_ID".to_string(), id.to_string());
    }
    // ...and the board's boot `Config` for the same image
    // (`nros_board_mps2_an385::Config::from_image`), which a Rust leaf gets
    // from `nros::main!`'s deploy overlay instead.
    for (key, v) in [
        ("NROS_ENTRY_IP", &img.decl.network.ip),
        ("NROS_ENTRY_GATEWAY", &img.decl.network.gateway),
        ("NROS_ENTRY_NETMASK", &img.decl.network.netmask),
    ] {
        if let Some(v) = v {
            env.insert(key.to_string(), v.clone());
        }
    }

    // Issue 1305 — WHICH image this file builds, for `nros::main!`: a leaf
    // with several images reads `system.toml` for this one (its board,
    // locator, RMW overlay), not for the leaf's default. Each image has its
    // own file and its own target dir, so the value never changes under a
    // built artifact.
    env.insert(leaf_system::IMAGE_ENV.to_string(), img.image_id.clone());

    // Layer 4 — the APP rung: what this IMAGE states (`[image.<id>] env`).
    // Last, so it outranks the board and the implications above and is
    // outranked only by the calling environment.
    for (k, v) in leaf_app_rung(&img.decl)? {
        path_env.remove(&k);
        env.insert(k, v);
    }

    // Issue 1885 — a caller-named target dir gets a build class (see above).
    let shared = {
        let getenv = |k: &str| std::env::var(k).ok();
        let (cwd, _) = build_command(&img);
        match crate::builder::shared_target::caller_target_dir(native_args, &getenv, &cwd) {
            Some(tdir) => {
                let extra = [
                    ("cargo_config", img.cargo_config.as_deref().unwrap_or("")),
                    ("target", img.target.as_deref().unwrap_or("")),
                ];
                Some(
                    crate::builder::shared_target::apply(&tdir, &env, &mut path_env, &extra)
                        .map_err(|e| eyre!("`{}`'s shared build class: {e}", img.image_id))?,
                )
            }
            None => None,
        }
    };

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
    Ok(Some((img, shared)))
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
            knobs: BTreeMap::new(),
            // Issue 1142's `[system] features`. ABSENT MEANS NONE, and no test
            // in here asks about the runtime service families, so the helper
            // states the empty claim once instead of per test.
            features: Vec::new(),
            conf: Vec::new(),
        }
    }

    #[test]
    fn the_settings_file_sits_under_the_leafs_build_dir_per_image() {
        assert_eq!(
            settings_path(Path::new("/w/talker"), "native"),
            PathBuf::from("/w/talker/build/native/nros-cargo.toml")
        );
    }

    fn knobs(rows: &[(&str, leaf_system::KnobValue)]) -> BTreeMap<String, leaf_system::KnobValue> {
        rows.iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    fn env_rows(rows: &[(&str, &str)]) -> BTreeMap<String, String> {
        rows.iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    /// phase-484 W8 (RFC-0103 D6) — a knob stated in `[image.<id>.knobs]`
    /// renders to EXACTLY the row an `[image.<id>] env` row states, so moving a
    /// row from one to the other changes no byte of any carrier.
    #[test]
    fn a_knobs_row_renders_to_the_env_row_it_replaces() {
        use leaf_system::KnobValue::{Bool, Int, Str};
        let from_knobs = app_rung(
            None,
            &knobs(&[
                ("zephyr.heap_size", Int(98304)),
                ("executor.max_cbs", Int(16)),
                ("log.max_level", Str("warn".into())),
                ("zenoh.tx.batch", Bool(true)),
                // A derived count's zero is a stated answer, not "unset"
                // (issue 1033); it renders like any other value.
                ("xrce.max_subscribers", Int(0)),
                ("zenoh.socket_timeout_ms", Int(5)),
                // The derive sentinel is a stated value too: -1 renders as -1.
                ("xrce.subscriber_buffer_size", Int(-1)),
                ("xrce.subscriber_ring_depth", Int(1)),
                ("xrce.transport_mtu", Int(512)),
            ]),
            &env_rows(&[("NROS_BOOT_REPORT", "1")]),
        )
        .unwrap();
        // The rows the retired `[image.<id>] env` spelling of these knobs
        // stated, before W8 refused it.
        let from_env = env_rows(&[
            ("NROS_ZEPHYR_HEAP_SIZE", "98304"),
            ("NROS_EXECUTOR_MAX_CBS", "16"),
            ("NROS_LOG_MAX_LEVEL", "warn"),
            ("NROS_ZENOH_TX_BATCH", "1"),
            ("NROS_XRCE_MAX_SUBSCRIBERS", "0"),
            ("NROS_ZENOH_SOCKET_TIMEOUT_MS", "5"),
            ("NROS_XRCE_SUBSCRIBER_BUFFER_SIZE", "-1"),
            ("NROS_XRCE_SUBSCRIBER_RING_DEPTH", "1"),
            ("NROS_XRCE_TRANSPORT_MTU", "512"),
            ("NROS_BOOT_REPORT", "1"),
        ]);
        assert_eq!(from_knobs, from_env);
    }

    #[test]
    fn a_knob_the_registry_does_not_admit_is_refused_naming_it() {
        use leaf_system::KnobValue::{Bool, Int, Str};
        let refused = |key: &str, v: leaf_system::KnobValue| {
            app_rung(Some("talker"), &knobs(&[(key, v)]), &BTreeMap::new())
                .unwrap_err()
                .to_string()
        };
        let e = refused("zephyr.heap_sise", Int(1));
        assert!(
            e.contains("`[image.talker.knobs.zephyr] heap_sise` names no knob"),
            "{e}"
        );
        let e = refused("zenoh.max_queryable", Int(1));
        assert!(e.contains("names no knob"), "{e}");
        let e = refused("executor.heap_size", Int(1));
        assert!(
            e.contains("did you mean `[image.talker.knobs.zephyr] heap_size`"),
            "{e}"
        );
        let e = refused("log.max_level", Int(7));
        assert!(e.contains("NROS_LOG_MAX_LEVEL is in 0..=6"), "{e}");
        let e = refused("log.max_level", Str("loud".into()));
        assert!(e.contains("an integer or one of trace/"), "{e}");
        let e = refused("zephyr.heap_size", Str("98304".into()));
        assert!(e.contains("is an integer"), "{e}");
        let e = refused("zephyr.heap_size", Bool(true));
        assert!(e.contains("is an integer"), "{e}");
        let e = refused("zenoh.graph_discovery", Int(1));
        assert!(e.contains("is a bool"), "{e}");
    }

    /// A registry knob has ONE spelling, `[image.<id>.knobs]`: an env row naming
    /// it is refused whether or not the knobs table states it too (release
    /// 0.6.0 warned; phase-484 W8 refuses), naming the spelling to move to.
    #[test]
    fn an_env_row_naming_a_registry_knob_is_refused_naming_its_knobs_key() {
        let e = app_rung(
            Some("talker"),
            &BTreeMap::new(),
            &env_rows(&[("NROS_ZEPHYR_HEAP_SIZE", "2")]),
        )
        .unwrap_err()
        .to_string();
        assert!(
            e.contains("[image.talker] env NROS_ZEPHYR_HEAP_SIZE")
                && e.contains("[image.talker.knobs.zephyr] heap_size"),
            "{e}"
        );
        // Stated both ways is the same refusal.
        let e = app_rung(
            Some("talker"),
            &knobs(&[("zephyr.heap_size", leaf_system::KnobValue::Int(1))]),
            &env_rows(&[("NROS_ZEPHYR_HEAP_SIZE", "2")]),
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("[image.talker.knobs.zephyr] heap_size"), "{e}");
        // The Kconfig spelling a west build renders an env row to is the same
        // knob too (`NROS_MAX_PUBLISHERS` is `CONFIG_NROS_MAX_PUBLISHERS`),
        // and the error quotes the row as written.
        let e = app_rung(
            None,
            &BTreeMap::new(),
            &env_rows(&[("NROS_MAX_PUBLISHERS", "2")]),
        )
        .unwrap_err()
        .to_string();
        assert!(
            e.contains("[image.<id>] env NROS_MAX_PUBLISHERS")
                && e.contains("knobs.zenoh] max_publishers"),
            "{e}"
        );
        assert_eq!(
            registry_knob_of_env_row("NROS_MAX_PUBLISHERS").map(|r| r.env),
            Some("NROS_ZENOH_MAX_PUBLISHERS")
        );
        assert!(registry_knob_of_env_row("NROS_BOOT_REPORT").is_none());
    }

    /// An env row that is NOT a registry knob is an app's own variable: carried
    /// as written. A retired spelling is still refused.
    #[test]
    fn a_non_knob_env_row_is_carried_and_a_retired_one_refused() {
        let rows = app_rung(
            Some("t"),
            &BTreeMap::new(),
            &env_rows(&[("MY_APP_FLAG", "x")]),
        )
        .unwrap();
        assert_eq!(rows["MY_APP_FLAG"], "x");
        let e = app_rung(
            Some("t"),
            &BTreeMap::new(),
            &env_rows(&[("ZPICO_MAX_QUERYABLES", "4")]), // a retired spelling
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("NROS_ZENOH_MAX_QUERYABLES"), "{e}");
    }

    /// The GATE (phase-484 W8): no tracked `system.toml` states a REGISTRY knob
    /// as an `[image.<id>] env` / `[image_defaults] env` row — that spelling is
    /// refused at build time, and this finds it on every PR instead. Read raw, so it
    /// covers every file whatever road builds it.
    #[test]
    fn no_tracked_system_toml_states_a_registry_knob_as_an_env_row() {
        let repo = || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .canonicalize()
                .unwrap()
        };
        let out = crate::source_stamp::nros_git_command("git")
            .args(["ls-files", "-z", "--", "*system.toml"])
            .current_dir(repo())
            .output()
            .expect("git ls-files");
        let mut offenders = Vec::new();
        let mut files = 0;
        for rel in String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|s| !s.is_empty())
        {
            let text = std::fs::read_to_string(repo().join(rel)).unwrap();
            let doc: toml::Table = text.parse().unwrap_or_else(|e| panic!("{rel}: {e}"));
            files += 1;
            let mut blocks: Vec<(String, &toml::Table)> = Vec::new();
            if let Some(d) = doc.get("image_defaults").and_then(|v| v.as_table()) {
                blocks.push(("image_defaults".into(), d));
            }
            if let Some(imgs) = doc.get("image").and_then(|v| v.as_table()) {
                for (id, b) in imgs {
                    if let Some(b) = b.as_table() {
                        blocks.push((format!("image.{id}"), b));
                    }
                }
            }
            for (table, b) in blocks {
                let Some(env) = b.get("env").and_then(|v| v.as_table()) else {
                    continue;
                };
                for k in env.keys() {
                    if let Some(row) = registry_knob_of_env_row(k) {
                        let (g, n) = row.image_group_name();
                        offenders.push(format!(
                            "{rel}: [{table}] env {k} -> [{table}.knobs.{g}] {n}"
                        ));
                    }
                }
            }
        }
        assert!(
            files >= 30,
            "the walk reached only {files} system.toml files"
        );
        assert!(
            offenders.is_empty(),
            "a registry knob is stated as an env row; move each to its knobs spelling:\n  {}",
            offenders.join("\n  ")
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
            board: "linux".into(),
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

#[cfg(test)]
mod board_crate_tests {
    //! Issue 1305 — the leaf's board crate and the image's board must agree.
    use super::*;

    fn repo() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap()
    }

    /// Every in-tree single-package Rust leaf on a cargo road is accepted:
    /// it names the crate its image needs, or no board crate at all. A leaf
    /// that fails here was never buildable for its own image.
    ///
    /// Issue 1815 — the walk covers EVERY tracked `system.toml`, not only
    /// `examples/`: the refusal runs in the fixture build's row-codegen pass
    /// too, and the leaf it broke (`log-arena-probe`) lives under
    /// `packages/testing/`, where the first version of this sweep never looked.
    #[test]
    fn every_in_tree_rust_leaf_names_the_board_crate_its_image_needs() {
        let out = crate::source_stamp::nros_git_command("git")
            .args(["ls-files", "-z", "--", "*system.toml"])
            .current_dir(repo())
            .output()
            .expect("git ls-files");
        let mut checked = 0;
        for rel in String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|s| !s.is_empty())
        {
            let leaf = repo().join(rel).parent().unwrap().to_path_buf();
            if !leaf.join("Cargo.toml").is_file() {
                continue;
            }
            let Some(img) = resolve(&leaf, &repo()).unwrap_or_else(|e| panic!("{rel}: {e}")) else {
                continue;
            };
            check_board_crate_dependency(&img).unwrap_or_else(|e| panic!("{e}"));
            checked += 1;
        }
        assert!(
            checked >= 10,
            "the walk reached only {checked} cargo leaves"
        );
    }

    /// Issue 1305 — the leaves NOT on the selection package, each with the
    /// reason it is not, every one of them out of scope BY SHAPE. Every other in-tree cargo-road leaf on a supported board —
    /// the examples, the copy-out templates' Rust packages, the mps2 C/C++ apps
    /// on a cargo link root, and (phase F) every `packages/testing/` program,
    /// bench and harness-templated fixture entry (issue 1340's witness
    /// included, with `[image.native] producers = false`) — depends on its
    /// `<pkg>_nros_selection` package and names no board crate (enforced by
    /// [`check_board_crate_dependency`]). A leaf for a `tier = "scaffold"`
    /// board is skipped by that DERIVED property, never listed here. A prefix
    /// matches a leaf path relative to the checkout.
    const NOT_ON_THE_SELECTION: &[(&str, &str)] = &[
        (
            "examples/rv-virt-threadx/rust/",
            "lib-only staticlibs a CMake image links (no `nros::main!`): the CMake \
             road picks the RMW (`-DNROS_RMW`) and links the backend itself, so the \
             cargo-road selection package — one `[image]`'s board, RMW and runtime — \
             has nothing to select; the board crate is target-scoped so the node \
             still builds for the host probe",
        ),
        (
            "packages/testing/nros-tests/bins/log-arena-probe",
            "names no board crate at all (issue 1815)",
        ),
    ];

    /// Issue 1305 phase C — the gate the sweep leaves behind: every in-tree
    /// cargo-road leaf outside [`NOT_ON_THE_SELECTION`] depends on its
    /// selection package, so a NEW leaf in a migrated family that names its
    /// board crate by hand fails here (and its own `nros sync` refuses it if it
    /// names both). An exemption that no longer matches a leaf fails too, so
    /// the list shrinks as families move.
    #[test]
    fn every_cargo_leaf_outside_the_open_classes_uses_its_selection_package() {
        let out = crate::source_stamp::nros_git_command("git")
            .args(["ls-files", "-z", "--", "*system.toml"])
            .current_dir(repo())
            .output()
            .expect("git ls-files");
        let mut on_selection = 0;
        let mut used = vec![false; NOT_ON_THE_SELECTION.len()];
        let mut stray = Vec::new();
        // A leaf for an UNSUPPORTED board is out of scope by a DERIVED
        // property, never by naming its path: the board registry's
        // `tier = "scaffold"` (the dormant ESP32, issue 1525 — code kept,
        // nothing built or migrated).
        let scaffold = scaffold_board_crates(&repo());
        let mut unsupported = 0;
        for rel in String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|s| !s.is_empty())
        {
            let leaf = repo().join(rel).parent().unwrap().to_path_buf();
            if !leaf.join("Cargo.toml").is_file() {
                continue;
            }
            let Some(img) = resolve(&leaf, &repo()).unwrap_or_else(|e| panic!("{rel}: {e}")) else {
                continue;
            };
            let leaf_rel = Path::new(rel)
                .parent()
                .unwrap()
                .to_string_lossy()
                .to_string();
            if let Some(i) = NOT_ON_THE_SELECTION
                .iter()
                .position(|(p, _)| leaf_rel.starts_with(p))
            {
                used[i] = true;
                continue;
            }
            if crate::builder::entry::macro_board_crate(&[
                img.image_id.as_str(),
                img.board.as_str(),
                img.platform.as_str(),
            ])
            .is_some_and(|k| scaffold.contains(&k))
            {
                unsupported += 1;
                continue;
            }
            if depends_on_selection(&img.leaf, &img.package) {
                // ...and names no board crate beside it — the refusal sync
                // applies, asked of every migrated leaf here too.
                if let Err(e) = check_board_crate_dependency(&img) {
                    stray.push(format!("{leaf_rel}: {e}"));
                }
                on_selection += 1;
            } else {
                stray.push(leaf_rel);
            }
        }
        // The walk above only reaches a leaf that HAS a `system.toml`, so a cargo
        // package that names a board crate and declares no image at all was
        // invisible to it — the gate's reach was narrower than its rule (issue
        // 0196's shape; 15 `examples/native/rust/*` leaves sat there, issue 1305
        // phase E). Every tracked `[package]` manifest under `examples/` with no
        // `system.toml` beside it is asked here: naming a board crate makes it a
        // stray, because a leaf with no image cannot get a selection package.
        // Phase F widened it to `packages/testing/` — the test programs, benches
        // and harness fixtures, 22 of which sat outside the walk the same way.
        let out = crate::source_stamp::nros_git_command("git")
            .args([
                "ls-files",
                "-z",
                "--",
                "examples/**/Cargo.toml",
                "packages/testing/**/Cargo.toml",
            ])
            .current_dir(repo())
            .output()
            .expect("git ls-files");
        let mut imageless = 0;
        for rel in String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|s| !s.is_empty())
        {
            let manifest = repo().join(rel);
            let leaf = manifest.parent().unwrap();
            if leaf.join("system.toml").is_file() {
                continue;
            }
            let Some(doc) = std::fs::read_to_string(&manifest)
                .ok()
                .and_then(|t| t.parse::<toml::Table>().ok())
            else {
                continue;
            };
            if !doc.contains_key("package") {
                continue;
            }
            imageless += 1;
            let leaf_rel = Path::new(rel)
                .parent()
                .unwrap()
                .to_string_lossy()
                .to_string();
            if let Some(i) = NOT_ON_THE_SELECTION
                .iter()
                .position(|(p, _)| leaf_rel.starts_with(p))
            {
                used[i] = true;
                continue;
            }
            let boards = named_board_crates(&doc);
            if !boards.is_empty() && boards.iter().all(|b| scaffold.contains(b)) {
                unsupported += 1;
                continue;
            }
            // A workspace ENTRY whose image a bringup claims has no
            // `system.toml` of its own and is on its selection package all the
            // same (`write_entry_selection`, phase F).
            if boards.is_empty()
                && doc
                    .get("package")
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                    .is_some_and(|name| depends_on_selection(leaf, name))
            {
                on_selection += 1;
            }
            if !boards.is_empty() {
                stray.push(format!(
                    "{leaf_rel}: names `{}` and has no `system.toml` (no image, so no \
                     selection package)",
                    boards.join("`, `")
                ));
            }
        }
        assert!(
            imageless >= 1,
            "the image-less arm reached no `[package]` manifest under examples/ or \
             packages/testing/"
        );
        assert!(
            stray.is_empty(),
            "these cargo leaves name their image dependencies by hand; depend on the \
             generated `<pkg>_nros_selection = {{ path = \"build/nros/selection\" }}` \
             instead (issue 1305), or add an exemption with its reason:\n  {}",
            stray.join("\n  ")
        );
        let stale: Vec<&str> = NOT_ON_THE_SELECTION
            .iter()
            .zip(&used)
            .filter(|(_, u)| !**u)
            .map(|((p, _), _)| *p)
            .collect();
        assert!(stale.is_empty(), "exemptions matching no leaf: {stale:?}");
        // The scaffold rule is only as good as its reach: it must find the
        // dormant leaves it exists for, or the registry spelling drifted.
        assert!(
            unsupported >= 1,
            "no leaf matched a `tier = \"scaffold\"` board ({scaffold:?})"
        );
        assert!(
            on_selection >= 98,
            "only {on_selection} leaves on the selection package"
        );
    }

    /// A migrated leaf that names a board crate again is refused.
    #[test]
    fn a_migrated_leaf_naming_a_board_crate_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = migrated_leaf(tmp.path(), "qemu-mps2-an385", "");
        let m = read(&leaf.join("Cargo.toml")) + "nros-board-mps2-an385 = { version = \"*\" }\n";
        std::fs::write(leaf.join("Cargo.toml"), m).unwrap();
        let img = resolve(&leaf, &repo()).unwrap().unwrap();
        let e = check_board_crate_dependency(&img)
            .expect_err("selection + board crate")
            .to_string();
        assert!(
            e.contains("my_talker_nros_selection") && e.contains("`nros-board-mps2-an385`"),
            "{e}"
        );
    }

    #[test]
    fn a_switched_board_is_refused_naming_both_crates() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = tmp.path().join("talker");
        std::fs::create_dir_all(leaf.join("src")).unwrap();
        std::fs::write(
            leaf.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [dependencies]\nnros-board-mps2-an385 = { version = \"*\" }\n",
        )
        .unwrap();
        std::fs::write(
            leaf.join("system.toml"),
            "[system]\nname = \"t\"\nrmw = \"zenoh\"\n\n[image.native]\nboard = \"linux\"\n",
        )
        .unwrap();
        let img = resolve(&leaf, &repo()).unwrap().expect("a cargo leaf");
        let e = check_board_crate_dependency(&img)
            .expect_err("the image needs nros-board-linux")
            .to_string();
        assert!(
            e.contains("`nros-board-linux`") && e.contains("`nros-board-mps2-an385`"),
            "{e}"
        );
        assert!(e.contains("issue 1305"), "{e}");
    }

    /// A migrated leaf: names no board crate, depends on its selection package.
    fn migrated_leaf(dir: &Path, board: &str, image_extra: &str) -> PathBuf {
        let leaf = dir.join("talker");
        std::fs::create_dir_all(leaf.join("src")).unwrap();
        std::fs::write(
            leaf.join("Cargo.toml"),
            "[package]\nname = \"my-talker\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [target.'cfg(target_os = \"none\")'.dependencies]\n\
             my_talker_nros_selection = { path = \"build/nros/selection\" }\n",
        )
        .unwrap();
        std::fs::write(
            leaf.join("system.toml"),
            format!(
                "[system]\nname = \"t\"\nrmw = \"zenoh\"\n\n[image.img]\nboard = \"{board}\"\n{image_extra}"
            ),
        )
        .unwrap();
        leaf
    }

    fn read(p: &Path) -> String {
        std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    }

    /// Issue 1305 / RFC-0098 D6 — the selection package carries every
    /// image-only dependency, from the board descriptor and the system: the
    /// board crate with its entry surface, its non-RMW defaults and the RMW,
    /// and the crate-root panic provider; and re-exports the board crate for
    /// `nros::main!`.
    #[test]
    fn a_migrated_leaf_gets_the_image_dependencies_generated() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = migrated_leaf(tmp.path(), "qemu-mps2-an385", "");
        let sel = write_selection(&leaf, &repo(), None)
            .unwrap()
            .expect("a migrated leaf gets a selection");
        assert_eq!(sel.crate_name, "my_talker_nros_selection");
        assert_eq!(
            sel.dir,
            leaf.canonicalize().unwrap().join("build/nros/selection")
        );
        let m = read(&sel.dir.join("Cargo.toml"));
        assert!(m.contains("name = \"my_talker_nros_selection\""), "{m}");
        let board = m
            .lines()
            .find(|l| l.starts_with("nros-board-mps2-an385 "))
            .unwrap_or_else(|| panic!("no board crate:\n{m}"));
        for f in [
            "\"board-entry\"",
            "\"ethernet\"",
            "\"rmw-zenoh\"",
            "default-features = false",
        ] {
            assert!(board.contains(f), "`{f}` missing: {board}");
        }
        assert!(m.contains("panic-semihosting"), "{m}");
        // Bare metal: the umbrella's marker is never named on nros-platform.
        assert!(!m.contains("nros-platform ="), "{m}");
        let lib = read(&sel.dir.join("src/lib.rs"));
        assert!(lib.contains("pub use nros_board_mps2_an385;"), "{lib}");
        assert!(lib.contains("use panic_semihosting as _;"), "{lib}");
        // The leaf is migrated, so the 1305 refusal has nothing to refuse.
        let img = resolve(&leaf, &repo()).unwrap().unwrap();
        check_board_crate_dependency(&img).expect("a migrated leaf is accepted");
    }

    /// Issue 1305 / RFC-0056 — one leaf, two images on two editions: each
    /// image's selection names its own edition on the umbrella AND on the
    /// board crate (which forwards it to the backend's keyexpr), and the two
    /// selections differ. The settings file names the image it builds, which
    /// is how `nros::main!` reads that image and not the leaf's default.
    #[test]
    fn two_images_on_two_editions_get_two_selections() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = tmp.path().join("talker");
        std::fs::create_dir_all(leaf.join("src")).unwrap();
        std::fs::write(
            leaf.join("Cargo.toml"),
            "[package]\nname = \"my-talker\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [dependencies]\nmy_talker_nros_selection = { path = \"build/nros/selection\" }\n",
        )
        .unwrap();
        std::fs::write(
            leaf.join("system.toml"),
            "[system]\nname = \"t\"\nrmw = \"zenoh\"\ndefault_images = [\"native\"]\n\n\
             [image_defaults]\nboard = \"linux\"\n\n\
             [image.native]\n\n\
             [image.native_xrce_jazzy]\nrmw = \"xrce\"\nros_edition = \"jazzy\"\n",
        )
        .unwrap();
        let humble = write_selection(&leaf, &repo(), None).unwrap().unwrap();
        let humble_m = read(&humble.dir.join("Cargo.toml"));
        let jazzy = write_selection(&leaf, &repo(), Some("native_xrce_jazzy"))
            .unwrap()
            .unwrap();
        let jazzy_m = read(&jazzy.dir.join("Cargo.toml"));
        assert_ne!(humble_m, jazzy_m, "two editions, one selection");
        let board = |m: &str| {
            m.lines()
                .find(|l| l.starts_with("nros-board-linux "))
                .unwrap_or_else(|| panic!("no board crate:\n{m}"))
                .to_string()
        };
        let umbrella = |m: &str| {
            m.lines()
                .find(|l| l.starts_with("nros = "))
                .unwrap_or_else(|| panic!("no nros:\n{m}"))
                .to_string()
        };
        assert!(umbrella(&humble_m).contains("\"ros-humble\""), "{humble_m}");
        assert!(board(&humble_m).contains("\"ros-humble\""), "{humble_m}");
        assert!(board(&humble_m).contains("\"rmw-zenoh\""), "{humble_m}");
        assert!(umbrella(&jazzy_m).contains("\"ros-jazzy\""), "{jazzy_m}");
        assert!(board(&jazzy_m).contains("\"ros-jazzy\""), "{jazzy_m}");
        assert!(board(&jazzy_m).contains("\"rmw-xrce\""), "{jazzy_m}");
        // What the image SELECTS — the dependency table, not the board's own
        // features the package passes through for a leaf to name (phase F).
        let selected = jazzy_m.split("\n[features]\n").next().unwrap();
        assert!(!selected.contains("ros-humble"), "{jazzy_m}");
        assert!(!selected.contains("rmw-zenoh"), "{jazzy_m}");
        // An image the file does not declare is refused, naming the ones it does.
        let e = write_selection(&leaf, &repo(), Some("native_iron"))
            .expect_err("undeclared image")
            .to_string();
        assert!(e.contains("native_xrce_jazzy"), "{e}");
    }

    /// The point of the package: a board switch in `system.toml` is the ONE
    /// edit. The same manifest gets the other board's crate, without the
    /// first board's entry feature (which the other crate does not declare).
    #[test]
    fn switching_board_rewrites_the_selection_and_nothing_else() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = migrated_leaf(tmp.path(), "qemu-mps2-an385", "");
        let manifest = read(&leaf.join("Cargo.toml"));
        write_selection(&leaf, &repo(), None).unwrap().unwrap();
        let again = write_selection(&leaf, &repo(), None).unwrap().unwrap();
        assert!(!again.changed, "an unchanged image rewrote its selection");

        let sys = read(&leaf.join("system.toml")).replace("qemu-mps2-an385", "freertos");
        std::fs::write(leaf.join("system.toml"), sys).unwrap();
        let sel = write_selection(&leaf, &repo(), None).unwrap().unwrap();
        assert!(sel.changed);
        let m = read(&sel.dir.join("Cargo.toml"));
        assert!(
            !m.contains("nros-board-mps2-an385 "),
            "old board left:\n{m}"
        );
        let board = m
            .lines()
            .find(|l| l.starts_with("nros-board-mps2-an385-freertos "))
            .unwrap_or_else(|| panic!("no FreeRTOS board crate:\n{m}"));
        assert!(board.contains("\"rmw-zenoh\""), "{board}");
        assert!(!board.contains("board-entry"), "{board}");
        assert!(m.contains("features = [\"platform-freertos\"]"), "{m}");
        assert!(
            read(&sel.dir.join("src/lib.rs")).contains("pub use nros_board_mps2_an385_freertos;")
        );
        assert_eq!(
            read(&leaf.join("Cargo.toml")),
            manifest,
            "the leaf manifest moved"
        );
    }

    /// An image that names a `panic` policy other than `own` gets its handler
    /// from `nros::main!`, so the board's crate-root provider would be a
    /// second `#[panic_handler]`.
    #[test]
    fn a_platform_panic_image_carries_no_crate_root_provider() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = migrated_leaf(tmp.path(), "freertos", "panic = \"platform\"\n");
        let sel = write_selection(&leaf, &repo(), None).unwrap().unwrap();
        assert!(!read(&sel.dir.join("Cargo.toml")).contains("panic-semihosting"));
        assert!(!read(&sel.dir.join("src/lib.rs")).contains("panic_semihosting"));
    }

    /// A leaf that still names its board crate is not migrated: no selection,
    /// and the refusal still guards it.
    #[test]
    fn an_unmigrated_leaf_gets_no_selection() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = tmp.path().join("talker");
        std::fs::create_dir_all(&leaf).unwrap();
        std::fs::write(
            leaf.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [dependencies]\nnros-board-linux = { version = \"*\" }\n",
        )
        .unwrap();
        std::fs::write(
            leaf.join("system.toml"),
            "[system]\nname = \"t\"\nrmw = \"zenoh\"\n\n[image.native]\nboard = \"linux\"\n",
        )
        .unwrap();
        assert!(write_selection(&leaf, &repo(), None).unwrap().is_none());
        assert!(!leaf.join("build").exists());
    }

    /// Issue 1305 phase F — a workspace ENTRY with no `system.toml` of its own
    /// gets its selection from the bringup image that claims it (`entry =
    /// "<pkg>"`), the board's own features passed through for a
    /// board-specific leaf; an entry no image claims gets nothing.
    #[test]
    fn a_claimed_entry_gets_its_selection_from_the_bringup_image() {
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path().join("src/demo_bringup");
        let entry = tmp.path().join("src/firmware");
        std::fs::create_dir_all(&bringup).unwrap();
        std::fs::create_dir_all(entry.join("src")).unwrap();
        std::fs::write(
            bringup.join("system.toml"),
            "[system]\nname = \"demo\"\nrmw = \"zenoh\"\n\n\
             [image.freertos]\nboard = \"freertos\"\nentry = \"firmware\"\n\
             locator = \"tcp/10.0.2.2:7891\"\n",
        )
        .unwrap();
        std::fs::write(
            entry.join("Cargo.toml"),
            "[package]\nname = \"firmware\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [dependencies]\nfirmware_nros_selection = { path = \"build/nros/selection\" }\n",
        )
        .unwrap();
        let sel = write_entry_selection(&entry, &bringup, &repo())
            .unwrap()
            .expect("the bringup claims `firmware`");
        let manifest = read(&sel.dir.join("Cargo.toml"));
        assert!(
            manifest.contains("nros-board-mps2-an385-freertos = {"),
            "{manifest}"
        );
        assert!(
            manifest.contains("\n[features]\n")
                && manifest.contains("[\"nros-board-mps2-an385-freertos/rmw-zenoh\"]"),
            "{manifest}"
        );
        assert!(
            read(&sel.dir.join("src/lib.rs")).contains("pub use nros_board_mps2_an385_freertos;")
        );

        let other = tmp.path().join("src/other");
        std::fs::create_dir_all(other.join("src")).unwrap();
        std::fs::write(
            other.join("Cargo.toml"),
            "[package]\nname = \"other\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [dependencies]\nother_nros_selection = { path = \"build/nros/selection\" }\n",
        )
        .unwrap();
        assert!(
            write_entry_selection(&other, &bringup, &repo())
                .unwrap()
                .is_none()
        );
    }

    /// Issue 1340 — the host board's census + profile producers are ON for a
    /// host image's selection by default, and `[image.<id>] producers = false`
    /// drops exactly those two features (nothing else in the line moves).
    #[test]
    fn producers_false_drops_the_host_census_and_profile_features() {
        let line = |extra: &str| {
            let tmp = tempfile::tempdir().unwrap();
            let leaf = tmp.path().join("probe");
            std::fs::create_dir_all(leaf.join("src")).unwrap();
            std::fs::write(
                leaf.join("Cargo.toml"),
                "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
                 [dependencies]\nprobe_nros_selection = { path = \"build/nros/selection\" }\n",
            )
            .unwrap();
            std::fs::write(
                leaf.join("system.toml"),
                format!(
                    "[system]\nname = \"p\"\nrmw = \"zenoh\"\n\n[image.native]\nboard = \"linux\"\n{extra}"
                ),
            )
            .unwrap();
            let sel = write_selection(&leaf, &repo(), None).unwrap().unwrap();
            read(&sel.dir.join("Cargo.toml"))
                .lines()
                .find(|l| l.starts_with("nros-board-linux "))
                .expect("board line")
                .to_string()
        };
        let on = line("");
        assert!(
            on.contains("\"census\"") && on.contains("\"profile\""),
            "{on}"
        );
        let off = line("producers = false\n");
        // Quoted, as features: the line also holds the board crate's PATH, and a
        // checkout whose directory name contains `census` matched the bare word.
        assert!(
            !off.contains("\"census\"") && !off.contains("\"profile\""),
            "{off}"
        );
        assert_eq!(
            on.replace(", \"census\", \"profile\"", ""),
            off,
            "only the two producer features move"
        );
        assert_eq!(line("producers = true\n"), on, "`true` is the default");
    }

    /// Issue 1815 — no board crate is not a stale board crate.
    #[test]
    fn a_leaf_naming_no_board_crate_is_accepted() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = tmp.path().join("probe");
        std::fs::create_dir_all(leaf.join("src")).unwrap();
        std::fs::write(
            leaf.join("Cargo.toml"),
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [dependencies]\nnros-platform-cffi = { version = \"*\" }\n",
        )
        .unwrap();
        std::fs::write(
            leaf.join("system.toml"),
            "[system]\nname = \"p\"\nrmw = \"zenoh\"\n\n[image.native]\nboard = \"linux\"\n",
        )
        .unwrap();
        let img = resolve(&leaf, &repo()).unwrap().expect("a cargo leaf");
        check_board_crate_dependency(&img).expect("no board crate is not a stale one");
    }
}
