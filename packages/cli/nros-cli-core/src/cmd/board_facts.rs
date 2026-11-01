//! phase-351 W5 — resolve one deploy's board FACTS + SITE config, as env lines.
//!
//! [RFC-0072](../../../../../docs/design/0072-rtos-integration-nano-ros-is-a-guest.md)
//! §5 splits board information into A (board facts, in the board package), B
//! (site config, in the user's `[board_config.<board>]`) and C (test harness).
//! W1–W4 gave both halves a home and a validity domain. This is DELIVERY: the
//! resolved pair, printed in one shape, for whoever is about to invoke cargo.
//!
//! **Why the invoker and not the leaf.** Cargo config is discovered from the
//! invocation CWD upward, and corrosion runs cargo from `workspace_toml_dir` —
//! so a leaf's own `.cargo/config.toml` is never read for a workspace member
//! (phase-349 W2.0 measured exactly this, and it is why the `NROS_BOARD_TOML`
//! row that wave added could not reach the members it was written for). The
//! process environment is the only carrier that crosses that boundary, and the
//! thing that owns it is whoever spawns cargo: cmake, a `just` recipe, or west.
//!
//! **One board per emission.** Exactly one board is active per configure — the
//! cmake side selects it with `if/elseif` on `NROS_BOARD` and the toolchain
//! file must precede `project()` — so this prints ONE deploy's facts, never a
//! table the caller has to index.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use clap::Args as ClapArgs;
use eyre::{Result, eyre};

use crate::orchestration::{
    board_descriptor::{BoardCatalog, BoardDescriptor},
    site_config::SiteConfig,
};
use nros_board_common::platform_config::PLATFORM_CONFIG_FILENAME;

#[derive(Debug, ClapArgs)]
pub struct BoardFactsArgs {
    /// Workspace (or bringup) dir to read `system.toml` from. Defaults to cwd.
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Deploy name to resolve. Defaults to the only one, or the one whose
    /// `board` matches `--board`.
    #[arg(long)]
    pub deploy: Option<String>,

    /// Board name, when the deploy is identified by its board rather than its
    /// key (what the cmake lane knows — `NROS_BOARD`).
    #[arg(long)]
    pub board: Option<String>,

    /// nano-ros SDK root holding `packages/boards`. Defaults to an upward
    /// search, then this `nros` binary's own SDK root.
    #[arg(long)]
    pub nano_ros_path: Option<PathBuf>,
}

/// The resolved facts, in emission order.
///
/// `BTreeMap` rather than a struct with named fields: the set is open (each
/// `sdk.*` and `config_files.*` key becomes its own variable) and every
/// consumer wants the same "export these" shape.
pub fn resolve(
    ws: &Path,
    nano_ros_root: &Path,
    deploy: Option<&str>,
    board: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<BTreeMap<String, String>> {
    // Several blocks may be in scope when the caller named neither a deploy
    // nor a board. Two naming the SAME board can no longer disagree — the site
    // table is keyed by board (issue 0951) — so what survives here is the
    // genuinely ambiguous case: blocks on DIFFERENT boards, where answering
    // with either one would hand the build another board's SDK roots.
    let (candidates, site, origin) = pick_deploys(ws, nano_ros_root, deploy, board)?;
    let mut resolved: Vec<(String, BTreeMap<String, String>)> = Vec::new();
    for (name, board) in &candidates {
        // Issue 2199 — a refusal about the BOARD belongs to every candidate on
        // that board, not to whichever one sorts first: naming one deploy read
        // as "this is the image being built" when nothing chose it.
        let label = candidates
            .iter()
            .filter(|(_, b)| b == board)
            .map(|(n, _)| format!("`{n}`"))
            .collect::<Vec<_>>();
        let label = match label.as_slice() {
            [one] => format!("deploy {one}"),
            many => format!("deploys {}", many.join(", ")),
        };
        let facts = resolve_one(&origin, nano_ros_root, &label, board, &site, env)?;
        resolved.push((name.clone(), facts));
    }
    if let Some((first_name, first)) = resolved.first()
        && let Some((other_name, other)) = resolved.iter().find(|(_, f)| f != first)
    {
        let differing: Vec<String> = first
            .keys()
            .chain(other.keys())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter(|k| first.get(*k) != other.get(*k))
            .cloned()
            .collect();
        return Err(eyre!(
            "deploys `{first_name}` and `{other_name}` resolve DIFFERENTLY ({}); \
             pass --deploy or --board to say which one this build is",
            differing.join(", ")
        ));
    }
    let mut out = resolved
        .into_iter()
        .next()
        .map(|(_, f)| f)
        .ok_or_else(|| eyre!("{}: no [image.*] blocks", ws.display()))?;
    // phase-400 W1 — platform packages in the USER's workspace.
    //
    // W1 says resolution is by name over a search path and that
    // `$NROS_PLATFORMS_DIR` "stops being the only way in". For an in-tree
    // platform it did: `default_search_path` carries `packages/platform`. For a
    // platform package in the user's own workspace it did NOT — nothing derived
    // the search path from the workspace, so such a package resolved only if a
    // human exported the variable by hand, and otherwise fell through to the
    // builtins with a warning.
    //
    // This is the seam that knows the workspace, so it is where the answer
    // belongs — the same way `NROS_BOARD_TOML` and `NROS_PLATFORM_NAME` are
    // handed down rather than rediscovered by every build script.
    //
    // A caller's own value comes FIRST: an explicit `--platforms-dir` is an
    // override and must keep outranking discovery.
    let mut roots: Vec<String> = env("NROS_PLATFORMS_DIR")
        .map(|v| {
            v.split(':')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    for dir in workspace_platform_roots(ws) {
        let d = dir.display().to_string();
        if !roots.contains(&d) {
            roots.push(d);
        }
    }
    if !roots.is_empty() {
        out.insert("NROS_PLATFORMS_DIR".into(), roots.join(":"));
    }
    Ok(out)
}

/// Do two spellings name the SAME board?
///
/// Compared by `names`, which is per-ENTRY, and NOT by `source`, which is the
/// descriptor FILE. `packages/boards/nros-board-nuttx/nros-board.toml` declares
/// two distinct boards — `qemu-armv7a-nuttx` and `rv-virt-nuttx` — so a
/// file-level compare answers "yes" for two boards that differ in ISA, and
/// `--board rv-virt-nuttx` silently resolves the arm board's facts. That is
/// the same collapse `check-site-config.py`'s alias map has to avoid, and it is
/// worth stating in both places: a directory is not a board.
fn same_board(a: &BoardDescriptor, b: &BoardDescriptor) -> bool {
    a.names == b.names
}

/// Find the `[board_config.<key>]` block that describes `descriptor`.
///
/// The key is matched by RESOLUTION, not by text: `resolve_board` is the same
/// rule `[deploy.*].board` and `[image.*].board` go through (issue 0606), so
/// every legal spelling of one board finds the same block. Two keys resolving
/// to one descriptor is an authoring mistake — it is the duplicate-fact shape
/// this table exists to remove — so it is refused rather than silently
/// order-dependent.
fn site_for_board<'a>(
    table: &'a BTreeMap<String, toml::Value>,
    catalog: &BoardCatalog,
    descriptor: &BoardDescriptor,
) -> Option<(&'a String, &'a toml::Value)> {
    table
        .iter()
        .find(|(key, _)| resolve_board(catalog, key).is_some_and(|d| same_board(d, descriptor)))
}

fn resolve_one(
    origin: &str,
    nano_ros_root: &Path,
    deploy_label: &str,
    board_name: &str,
    site_table: &BTreeMap<String, toml::Value>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<BTreeMap<String, String>> {
    let board_name = board_name.to_string();

    let catalog = BoardCatalog::load(nano_ros_root)
        .map_err(|e| eyre!("board catalog under {}: {e}", nano_ros_root.display()))?;
    let descriptor = resolve_board(&catalog, &board_name).ok_or_else(|| {
        eyre!(
            "no board descriptor claims `{board_name}` ({deploy_label}). \
             Descriptors are matched by their `names` and by their directory \
             (`packages/boards/nros-board-<name>`); an out-of-tree board package is \
             reached through `$NROS_EXTRA_BOARD_PATH`."
        )
    })?;

    // The site block is keyed by BOARD (issue 0951), and a board has several
    // legal spellings — the descriptor's `names`, its directory, its
    // downstream framework id. Matching the key TEXTUALLY would make
    // `[board_config."qemu-armv7a-nsh"]` invisible to a build that spelled the
    // same board `qemu-armv7a-nuttx`, which is issue 0606 one table over. So
    // resolve both sides through the catalog and compare DESCRIPTORS.
    let (site_section, site) = match site_for_board(site_table, &catalog, descriptor) {
        Some((key, value)) => {
            let section = format!("board_config.{key}");
            (
                section.clone(),
                SiteConfig::from_value(value, &section, origin)?,
            )
        }
        None => (format!("board_config.{board_name}"), SiteConfig::default()),
    };

    let mut out = BTreeMap::new();

    // The board rung itself (RFC-0049): the descriptor's own path, which is
    // what `nros-zpico-build` reads for per-board knob deltas. Delivered HERE
    // rather than from a leaf `[env]` row, which is what phase-351 W6 retires.
    if let Some(src) = descriptor.source.as_deref() {
        out.insert(
            "NROS_BOARD_TOML".into(),
            nano_ros_root.join(src).display().to_string(),
        );
    }
    out.insert("NROS_BOARD".into(), board_name.clone());

    // phase-484 W2b (RFC-0103 D1) — the board's OWN `[env]` facts from its
    // `cargo_config` (RTOS port, config dirs): the generated cargo config
    // carries them on the `nros build` road, and this carries the SAME rows on
    // the cmake road, so a board states them once and both roads deliver them.
    // Before, the cmake road had only `just/sdk-env.just`'s one repo-wide
    // export, which was the right answer for exactly one board.
    for (key, value) in descriptor_env_rows(descriptor, nano_ros_root) {
        out.entry(key).or_insert(value);
    }

    // phase-400 W6 — the board's PLATFORM, by name.
    //
    // `nros-node` deliberately has no `platform-*` cargo feature (phase-248 C2:
    // the core executor is platform-agnostic and reaches the platform through
    // the vtable), so its build script cannot know which platform it is
    // compiling for — and without that it cannot resolve the platform rung of
    // the RFC-0049 knob ladder. The descriptor knows; this is the seam that has
    // already resolved it.
    //
    // A NAME, not a path: `NROS_PLATFORM_NAME`, not `NROS_PLATFORM`, because
    // cmake's `-DNROS_PLATFORM=cffi` names the platform LAYER, a different
    // axis, and one env var meaning two things is how they start disagreeing.
    out.insert(
        "NROS_PLATFORM_NAME".into(),
        descriptor.platform.kebab().to_string(),
    );

    // W4 — the netstack, validated against the board's declared domain. An
    // unsupported pair fails HERE, at the seam that knows both halves, rather
    // than as a link error inside a stack nobody selected. The FATAL_ERROR a
    // caller sees comes from `NanoRosBoardFacts.cmake`, which matches on this
    // error's text — so the call below is load-bearing even though its Ok value
    // is discarded.
    //
    // Issue 1063 — the RESOLVED VALUE is deliberately NOT emitted. It used to
    // become `NROS_NETSTACK`, which nothing ever read: the only plausible
    // consumer is a per-board cmake file, and those pick their stack directly
    // (`cmake/board/nano-ros-board-rv-virt-threadx.cmake` calls
    // `nros_threadx_build_netstack_netxduo` unconditionally) because a file
    // that is already per-board knows its own netstack. Emitting a value with
    // no reader is the declared-but-unread shape phase-349 named; the
    // validation is the half that does work, so it is the half that stays.
    descriptor
        .resolve_netstack(site.netstack.as_deref())
        .map_err(|e| eyre!("{e}"))?;

    for (name, raw) in &site.sdk {
        let r = site.interpolate(raw, &site_section, origin, env)?;
        out.insert(format!("NROS_SDK_{}", env_key(name)), r.value);
    }
    // phase-484 W3 (RFC-0103 D4) — every source tree the board needs, DERIVED:
    // the index's `[board.<name>]` lists them, and each is located through the
    // one ladder (`nros locate`). A project no longer restates
    // `sdk = { freertos = "{env:FREERTOS_DIR}" }` to get this — 81 such rows
    // said nothing the environment did not, and read it raw (no re-root). A
    // source not provisioned anywhere is emitted EMPTY, which is how a
    // consumer (`check-template-copy-out`) tells "absent" from "present".
    // A project's own `sdk` entry (a vendor tree under its own name) is kept.
    for (name, located) in board_sources(descriptor, nano_ros_root) {
        out.entry(format!("NROS_SDK_{}", env_key(&name)))
            .or_insert_with(|| located.unwrap_or_default());
    }
    for (role, raw) in &site.config_files {
        let r = site.interpolate(raw, &site_section, origin, env)?;
        out.insert(format!("NROS_CONFIG_FILE_{}", env_key(role)), r.value);
    }
    if !site.include_dirs.is_empty() {
        let mut dirs = Vec::new();
        for raw in &site.include_dirs {
            dirs.push(site.interpolate(raw, &site_section, origin, env)?.value);
        }
        out.insert("NROS_INCLUDE_DIRS".into(), dirs.join(";"));
    }
    if !site.defines.is_empty() {
        out.insert("NROS_DEFINES".into(), site.defines.join(";"));
    }
    for (key, raw) in &site.upload {
        let r = site.interpolate(raw, &site_section, origin, env)?;
        out.insert(format!("NROS_UPLOAD_{}", env_key(key)), r.value);
    }
    Ok(out)
}

/// `freertos_plus_tcp` / `nuttx-apps` → FREERTOS_PLUS_TCP / NUTTX_APPS.
fn env_key(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .to_ascii_uppercase()
}

/// The index's source trees for `descriptor`'s board, each located —
/// `Err` when the ladder found it nowhere. Matched by the descriptor's names
/// and its directory (`nros-board-<key>`), like every other board lookup.
fn board_sources(
    descriptor: &BoardDescriptor,
    nano_ros_root: &Path,
) -> Vec<(String, std::result::Result<String, String>)> {
    let index_path = nano_ros_root.join("nros-sdk-index.toml");
    let Ok(index) = crate::orchestration::sdk_index::SdkIndex::load(&index_path) else {
        return Vec::new();
    };
    let dir_key = descriptor
        .source
        .as_deref()
        .and_then(|s| Path::new(s).parent())
        .and_then(|d| d.file_name())
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix("nros-board-"))
        .map(str::to_string);
    let entry = descriptor
        .names
        .iter()
        .cloned()
        .chain(dir_key)
        .find_map(|k| index.board.get(&k));
    let Some(entry) = entry else {
        return Vec::new();
    };
    let mut names: Vec<&String> = entry.packages.iter().chain(&entry.build_sources).collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .filter(|n| index.source.contains_key(n.as_str()))
        .map(|n| {
            let r = crate::cmd::locate::locate_source(&index, nano_ros_root, n)
                .map(|p| p.display().to_string());
            (n.clone(), r)
        })
        .collect()
}

/// The `[env]` rows of a board descriptor's `cargo_config`, `${workspace}`
/// resolved. A row is a string or `{ value = "…", … }`; `relative` is a
/// cargo-config spelling and means nothing to a process environment.
///
/// `force` does mean something, and the same thing on both roads (phase-517
/// W10): cargo applies a non-forced `[env]` row only when the variable is not
/// already in the environment, so a row this carries onto the cmake road —
/// where Corrosion's `cmake -E env` would otherwise OVERWRITE the variable — is
/// dropped when the environment already states it. That is what lets a board
/// name its default cross compiler (`CC_<triple>`) while a lane that resolved
/// a different one (`nros_riscv64_export_cc`) keeps its own on either road.
fn descriptor_env_rows(
    descriptor: &BoardDescriptor,
    nano_ros_root: &Path,
) -> Vec<(String, String)> {
    descriptor_env_rows_with(descriptor, nano_ros_root, |k| {
        std::env::var_os(k).is_some_and(|v| !v.is_empty())
    })
}

fn descriptor_env_rows_with(
    descriptor: &BoardDescriptor,
    nano_ros_root: &Path,
    env_has: impl Fn(&str) -> bool,
) -> Vec<(String, String)> {
    let Some(text) = descriptor.cargo_config_rendered(nano_ros_root) else {
        return Vec::new();
    };
    let Ok(doc) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let Some(env) = doc.get("env").and_then(toml::Value::as_table) else {
        return Vec::new();
    };
    env.iter()
        .filter_map(|(k, v)| {
            let (value, force) = match v {
                toml::Value::String(s) => (s.clone(), false),
                toml::Value::Table(t) => (
                    t.get("value")?.as_str()?.to_string(),
                    t.get("force")
                        .and_then(toml::Value::as_bool)
                        .unwrap_or(false),
                ),
                _ => return None,
            };
            if !force && env_has(k) {
                return None;
            }
            Some((k.clone(), value))
        })
        .collect()
}

/// Descriptor for a `[deploy.*].board` value — the catalog's ONE rule.
///
/// issue 0606: this used to carry its own `names`-then-directory fallback,
/// one of three such opinions in the tree. `BoardCatalog::resolve_deploy` owns
/// the rule now (names, then the directory alias, then the platform), so this
/// is a thin adapter that keeps the error shape callers expect.
pub fn resolve_board<'a>(catalog: &'a BoardCatalog, board: &str) -> Option<&'a BoardDescriptor> {
    match catalog.resolve_deploy(board) {
        crate::orchestration::board_descriptor::DeployResolution::Board(d) => Some(d),
        _ => None,
    }
}

/// Why `board` resolved to no descriptor: a retired key's replacement
/// (phase-500 W1 — `native` is `linux`), else the generic refusal. The callers
/// of [`resolve_board`] that refuse a miss share this text.
pub fn unresolved_board(board: &str) -> String {
    nros_entry_lower::retired_board_key_hint(board)
        .unwrap_or_else(|| format!("board `{board}` is claimed by no board descriptor"))
}

/// One candidate: the name it was selected by, and the board it resolves to.
///
/// A BOARD, not the whole block: since the site table became board-keyed
/// (issue 0951) the board is the only thing `resolve_one` needs, and carrying
/// the deploy block instead is what kept images invisible here — the board can
/// come from `[image.<id>]` just as well, and in a migrated workspace that is
/// the ONLY place it comes from.
type PickedDeploy = (String, String);

/// The picked deploys, the site table to resolve them against, and the file
/// that supplied both (for error text).
type Picked = (Vec<PickedDeploy>, BTreeMap<String, toml::Value>, String);

fn pick_deploys(
    ws: &Path,
    nano_ros_root: &Path,
    deploy: Option<&str>,
    board: Option<&str>,
) -> Result<Picked> {
    // A standalone leaf states its deployment in the `system.toml` beside its
    // manifest (RFC-0098 D3) — the same file a bringup uses, so one read serves
    // both. The retired manifest keys this used to map (phase-445 W5) are
    // refused by the reader, naming the file to write.
    //
    // Issue 2199 — and the read's ANSWER is the leaf's image. A leaf builds ONE
    // image (exactly one `[image.*]`, or `[system] default_images`), and
    // `nros ws leaf-system` — which the Zephyr hook renders the image from —
    // applies that rule. This verb used to discard it and resolve EVERY image
    // the file declares, so a leaf with `xiao_serial` (its default) and
    // `xiao_cdc` was refused naming `xiao_cdc`, the first key in order, for a
    // build of `xiao_serial`. A configure that names a deploy or a board still
    // chooses for itself.
    let own = nros_orchestration_ir::leaf_system::read(ws).map_err(|e| eyre!(e))?;
    // A workspace ENTRY states no board of its own: the bringup image that
    // claims it does (`leaf_system::for_entry`, phase-445 W5). This is the
    // Zephyr lane's case — `NanoRosBoardFacts.cmake` points this verb at the
    // west APPLICATION dir, which is the entry package, not the bringup. Its
    // `[package.metadata.nros.entry] deploy` used to answer here.
    let claimed = if ws
        .join(nros_orchestration_ir::leaf_system::SYSTEM_TOML)
        .is_file()
    {
        None
    } else {
        match crate::cmd::ws::cargo_package_name_of(&ws.join("Cargo.toml")) {
            Some(pkg) => crate::cmd::ws::workspace_entry_system(ws, &pkg)?,
            None => None,
        }
    };
    if let Some(leaf) = &claimed {
        let p = leaf.origin_path().to_path_buf();
        let raw = std::fs::read_to_string(&p).map_err(|e| eyre!("reading {}: {e}", p.display()))?;
        let sys = toml::from_str::<crate::orchestration::cargo_metadata_schema::SystemToml>(&raw)
            .map_err(|e| eyre!("parsing {}: {e}", p.display()))?;
        return pick_from(&p, &sys, Some(leaf), nano_ros_root, deploy, board);
    }
    let mut found = load_system_tomls(ws)?;
    if found.len() == 1 {
        let (p, sys) = found.remove(0);
        let own = own.as_ref().filter(|_| deploy.is_none() && board.is_none());
        return pick_from(&p, &sys, own, nano_ros_root, deploy, board);
    }
    let listed = found
        .iter()
        .map(|(p, _)| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    if deploy.is_none() && board.is_none() {
        return Err(eyre!(
            "{}: {} bringups carry a system.toml; point --path at one: {listed}",
            ws.display(),
            found.len(),
        ));
    }
    // Issue 1811's sibling: a workspace with several bringups (realtime-c has
    // `demo_bringup` and `smp_bringup`) was refused even when the configure
    // named the image, so no board fact reached cargo and `zpico-sys`
    // panicked on `FREERTOS_PORT`. The named deploy or board picks the
    // bringup; it is still refused when it picks none or more than one.
    let mut hits = Vec::new();
    let mut misses = Vec::new();
    for (p, sys) in &found {
        match pick_from(p, sys, None, nano_ros_root, deploy, board) {
            Ok(picked) => hits.push(picked),
            Err(e) => misses.push(e.to_string()),
        }
    }
    let wanted = deploy
        .map(|d| format!("deploy `{d}`"))
        .or_else(|| board.map(|b| format!("board `{b}`")))
        .unwrap_or_default();
    match hits.len() {
        1 => Ok(hits.remove(0)),
        0 => Err(eyre!(
            "{}: none of {} bringups has {wanted}: {}",
            ws.display(),
            found.len(),
            misses.join("; ")
        )),
        n => Err(eyre!(
            "{}: {n} bringups have {wanted}; point --path at one: {}",
            ws.display(),
            hits.iter()
                .map(|(_, _, o)| o.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// `pick_deploys` for one bringup's `system.toml`.
fn pick_from(
    path: &Path,
    system: &crate::orchestration::cargo_metadata_schema::SystemToml,
    claimed: Option<&nros_orchestration_ir::leaf_system::LeafSystem>,
    nano_ros_root: &Path,
    deploy: Option<&str>,
    board: Option<&str>,
) -> Result<Picked> {
    let origin = path.display().to_string();
    // RFC-0103 D3 rung 3 — a `[sources]` key names a `[source.*]` row, and a
    // misspelt one would be ignored by every reader (the ladder asks by NAME),
    // so it is refused here, on every configure, naming the rows that exist.
    if !system.sources.is_empty() || !system.tools.is_empty() {
        let index_path = nano_ros_root.join("nros-sdk-index.toml");
        if let Ok(index) = crate::orchestration::sdk_index::SdkIndex::load(&index_path) {
            let tables = [
                (
                    "sources",
                    "source",
                    &system.sources,
                    index.source.keys().collect::<Vec<_>>(),
                ),
                (
                    "tools",
                    "tool",
                    &system.tools,
                    index.tool.keys().collect::<Vec<_>>(),
                ),
            ];
            for (table, row, keys, known) in tables {
                for key in keys.keys() {
                    if !known.contains(&key) {
                        let known: Vec<&str> = known.iter().map(|s| s.as_str()).collect();
                        return Err(eyre!(
                            "{origin}: [{table}] names `{key}`, which is no [{row}.*] row in {} \
                             — known: {}",
                            index_path.display(),
                            known.join(", ")
                        ));
                    }
                }
            }
        }
    }
    let site = system.board_config.clone();
    // Candidates come from `[image.*]`, the only table that names a board
    // (`[deploy.*]` retired, phase-486 W8).
    let mut boards: BTreeMap<String, String> = BTreeMap::new();
    for name in system.image.keys() {
        if let Some(b) = system.image_for(name).and_then(|i| i.board) {
            boards.insert(name.clone(), b);
        }
    }
    // An entry's answer is its OWN image, not every image its bringup builds.
    if let Some(id) = claimed.and_then(|l| l.image.as_deref()) {
        boards.retain(|k, _| k == id);
    }

    let mut candidates: Vec<PickedDeploy> = boards.into_iter().collect();
    if let Some(want) = deploy {
        candidates.retain(|(k, _)| k == want);
        if candidates.is_empty() {
            return Err(eyre!(
                "{}: no [image.{want}] naming a board",
                path.display()
            ));
        }
    } else if let Some(want) = board {
        // Match by RESOLUTION, not by text. A board has several legal
        // spellings, and which one an author used is not a fact about the
        // build: `examples/workspaces/realtime-rust` writes `board =
        // "freertos"` on its image while cmake passes the directory spelling
        // `mps2-an385-freertos`, and `check-site-config` canonicalises both to
        // the same block. A textual compare here makes the gate and the
        // resolver disagree about which boards a workspace builds for — issue
        // 0606's failure, one table over.
        let catalog = BoardCatalog::load(nano_ros_root)
            .map_err(|e| eyre!("board catalog under {}: {e}", nano_ros_root.display()))?;
        let wanted = resolve_board(&catalog, want).map(|d| d.names.clone());
        candidates.retain(|(_, b)| {
            b == want
                || (wanted.is_some()
                    && resolve_board(&catalog, b).map(|d| d.names.clone()) == wanted)
        });
        if candidates.is_empty() {
            return Err(eyre!(
                "{}: no [image.*] with board = \"{want}\" \
                 (matched through the board catalog, so every spelling of one \
                 board is tried)",
                path.display()
            ));
        }
    }
    if candidates.is_empty() {
        return Err(eyre!("{}: no [image.*] names a board", path.display()));
    }
    Ok((candidates, site, origin))
}

/// Every `system.toml` in scope: `<ws>/system.toml`, else `<ws>/src/*/system.toml`.
fn load_system_tomls(
    ws: &Path,
) -> Result<
    Vec<(
        PathBuf,
        crate::orchestration::cargo_metadata_schema::SystemToml,
    )>,
> {
    let mut found: Vec<PathBuf> = Vec::new();
    let direct = ws.join("system.toml");
    if direct.is_file() {
        found.push(direct);
    } else if let Ok(entries) = std::fs::read_dir(ws.join("src")) {
        for e in entries.flatten() {
            let p = e.path().join("system.toml");
            if p.is_file() {
                found.push(p);
            }
        }
    }
    found.sort();
    if found.is_empty() {
        return Err(eyre!(
            "no system.toml at {} or {}/src/*/",
            ws.display(),
            ws.display()
        ));
    }
    found
        .into_iter()
        .map(|path| {
            let raw = std::fs::read_to_string(&path)
                .map_err(|e| eyre!("read {}: {e}", path.display()))?;
            let system = toml::from_str(&raw).map_err(|e| eyre!("{}: {e}", path.display()))?;
            Ok((path, system))
        })
        .collect()
}

pub fn run(args: BoardFactsArgs) -> Result<()> {
    let ws = args
        .path
        .canonicalize()
        .map_err(|e| eyre!("{}: {e}", args.path.display()))?;
    // phase-447 A2 — the shared four-rung ladder (RFC-0099 D3).
    let root = crate::orchestration::nano_ros_root::resolve(args.nano_ros_path.clone(), &ws)
        .ok_or_else(|| eyre!("{}", crate::orchestration::nano_ros_root::not_found_help()))?;

    let has_system = ws.join("system.toml").is_file()
        || std::fs::read_dir(ws.join("src"))
            .is_ok_and(|d| d.flatten().any(|e| e.path().join("system.toml").is_file()));
    // Issue 0941 — "this configure has no board facts to carry" is an ANSWER,
    // owned here and stated: exit 0, nothing on stdout, the reason on stderr.
    // Every other way this verb can fail is a nonzero exit, and the cmake arm
    // stops the configure on it. That is what lets the cmake side stop
    // pattern-matching the error TEXT, which is how both earlier hardenings
    // went wrong (`21be9c0684`, `5d9abbd376`).
    if let Some(why) = nothing_to_carry(&ws, &root, has_system, &args)? {
        eprintln!("board-facts: nothing to carry -- {why}");
        return Ok(());
    }
    let facts = if has_system {
        resolve(
            &ws,
            &root,
            args.deploy.as_deref(),
            args.board.as_deref(),
            &|k| std::env::var(k).ok(),
        )?
    } else {
        // phase-484 W2b (RFC-0103 D1) — a leaf with no `system.toml` (a test
        // bin, a bench) still names its board: it depends on exactly one board
        // crate, and every descriptor records its `board_crate`. Derive the
        // board from that rather than ask the leaf to author it twice, and
        // answer with the descriptor's facts alone (there is no site block).
        let board = match args.board.clone() {
            Some(b) => b,
            None => board_of_cargo_leaf(&ws, &root)?
                .expect("nothing_to_carry answered the no-board-crate case"),
        };
        resolve_one(
            &ws.join("Cargo.toml").display().to_string(),
            &root,
            "the leaf's board crate",
            &board,
            &BTreeMap::new(),
            &|k| std::env::var(k).ok(),
        )?
    };
    for (k, v) in facts {
        println!("{k}={v}");
    }
    Ok(())
}

/// Issue 0941 — the configures that LEGITIMATELY carry no board facts, as a
/// stated reason; `None` means this one must resolve (and any failure to is a
/// configure error). Enumerated from every caller of `nros_resolve_board_facts`
/// (2026-10-14), and deliberately short:
///
/// * nothing names a board and nothing states a deployment — a guest project
///   (`find_package(nano_ros)`, no `system.toml`, no `Cargo.toml`), or a cargo
///   dir that depends on no board crate (a host tool, a bench);
/// * a bringup that declares no `[image.*]` at all, asked for no image or
///   board — it builds nothing for a board (the orchestration test fixtures).
///
/// Anything that names a board or an image and cannot resolve it is NOT here.
fn nothing_to_carry(
    ws: &Path,
    root: &Path,
    has_system: bool,
    args: &BoardFactsArgs,
) -> Result<Option<String>> {
    if args.board.is_some() || args.deploy.is_some() {
        return Ok(None);
    }
    if has_system {
        let Ok(found) = load_system_tomls(ws) else {
            return Ok(None);
        };
        return Ok(found.iter().all(|(_, sys)| sys.image.is_empty()).then(|| {
            format!(
                "{}: no system.toml in scope declares an [image.*]",
                ws.display()
            )
        }));
    }
    if !ws.join("Cargo.toml").is_file() {
        return Ok(Some(format!(
            "{}: no system.toml and no Cargo.toml, so nothing names a board",
            ws.display()
        )));
    }
    Ok(board_of_cargo_leaf(ws, root)?
        .is_none()
        .then(|| format!("{}: its Cargo.toml depends on no board crate", ws.display())))
}

/// The board a `system.toml`-less cargo leaf builds for: the one board
/// descriptor whose `board_crate` the leaf's `Cargo.toml` depends on. `None`
/// when it depends on none — a host build, which has no board facts.
fn board_of_cargo_leaf(ws: &Path, root: &Path) -> Result<Option<String>> {
    let manifest = ws.join("Cargo.toml");
    let raw = std::fs::read_to_string(&manifest)
        .map_err(|e| eyre!("reading {}: {e}", manifest.display()))?;
    let doc: toml::Table =
        toml::from_str(&raw).map_err(|e| eyre!("{}: {e}", manifest.display()))?;
    let mut deps: Vec<String> = ["dependencies", "build-dependencies"]
        .iter()
        .filter_map(|t| doc.get(*t).and_then(toml::Value::as_table))
        .flat_map(|t| t.keys().cloned())
        .collect();
    // Issue 1305 — a leaf on its generated selection package names its board
    // crate THROUGH it (`<pkg>_nros_selection = { path = "build/nros/selection" }`,
    // written by `nros sync`), so the board crate is one of the selection's
    // dependencies. An entry whose image a bringup claims has no `system.toml`
    // of its own and is answered here.
    let selections: Vec<PathBuf> = doc
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
        .filter(|(k, _)| k.ends_with("_nros_selection"))
        .filter_map(|(_, v)| v.get("path").and_then(toml::Value::as_str))
        .map(|p| ws.join(p).join("Cargo.toml"))
        .collect();
    for sel in selections {
        if let Some(t) = std::fs::read_to_string(&sel)
            .ok()
            .and_then(|raw| toml::from_str::<toml::Table>(&raw).ok())
            .and_then(|d| {
                d.get("dependencies")
                    .and_then(toml::Value::as_table)
                    .cloned()
            })
        {
            deps.extend(t.keys().cloned());
        }
    }
    let deps: Vec<&str> = deps.iter().map(String::as_str).collect();
    let catalog = BoardCatalog::load(root)
        .map_err(|e| eyre!("board catalog under {}: {e}", root.display()))?;
    let hits: Vec<&BoardDescriptor> = catalog
        .descriptors()
        .iter()
        .filter(|d| d.board_crate.as_deref().is_some_and(|c| deps.contains(&c)))
        .collect();
    match hits.as_slice() {
        // The first of its names that resolves back to THIS descriptor — a
        // short alias (`threadx`) can be claimed by another board too.
        [one] => one
            .names
            .iter()
            .find(|n| resolve_board(&catalog, n).is_some_and(|d| same_board(d, one)))
            .cloned()
            .map(Some)
            .ok_or_else(|| {
                eyre!(
                    "no name of the board descriptor for `{:?}` resolves to it",
                    one.board_crate
                )
            }),
        [] => Ok(None),
        many => Err(eyre!(
            "{}: depends on {} board crates ({}) — pass --board",
            manifest.display(),
            many.len(),
            many.iter()
                .filter_map(|d| d.board_crate.as_deref())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Directories in `ws` that CONTAIN platform packages.
///
/// A platform descriptor lives at `<root>/<pkg>/nros-platform.toml`, so what
/// the search path wants is `<root>` — the same shape `packages/platform` has
/// in the tree.
///
/// Bounded and source-time (RFC-0071 D5): three levels is enough for
/// `<ws>/src/<pkg>/` and `<ws>/<pkg>/`, and the build outputs are skipped by
/// name because scanning them is pure cost — a workspace's `build/` and
/// `install/` hold copies of the very files this is looking for, and a copy
/// resolving as a platform is worse than not finding one.
fn workspace_platform_roots(ws: &Path) -> Vec<std::path::PathBuf> {
    const SKIP: &[&str] = &["build", "install", "log", "target", ".git"];
    let mut out = Vec::new();
    let mut stack = vec![(ws.to_path_buf(), 0u32)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > 3 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut holds_platform = false;
        for e in entries.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if SKIP.contains(&name) || name.starts_with('.') {
                continue;
            }
            if p.join(PLATFORM_CONFIG_FILENAME).is_file() {
                holds_platform = true;
            }
            stack.push((p, depth + 1));
        }
        if holds_platform && !out.contains(&dir) {
            out.push(dir);
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// phase-484 W2b — a board's own `[env]` facts reach every road through
    /// this verb, and a `system.toml`-less cargo leaf is attributed to the one
    /// board crate it depends on.
    #[test]
    fn an_environment_value_beats_a_non_forced_descriptor_row_only() {
        // phase-517 W10 — cargo's own `[env]` rule, on the cmake road too.
        let root = repo_root();
        let catalog = crate::orchestration::board_descriptor::BoardCatalog::load(&root)
            .expect("in-tree catalog");
        let d = resolve_board(&catalog, "rv-virt-threadx").expect("rv-virt-threadx");
        let cc = "CC_riscv64gc_unknown_none_elf";
        let all = descriptor_env_rows_with(d, &root, |_| false);
        assert!(all.iter().any(|(k, _)| k == cc), "{all:?}");
        let set = descriptor_env_rows_with(d, &root, |_| true);
        assert!(
            !set.iter().any(|(k, _)| k == cc),
            "a lane's CC wins: {set:?}"
        );
        assert!(
            set.iter().any(|(k, _)| k == "THREADX_PORT"),
            "a forced row is delivered whatever the environment says: {set:?}"
        );
    }

    #[test]
    fn a_cargo_leaf_gets_its_boards_descriptor_env_rows() {
        let root = repo_root();
        let leaf = tempfile::tempdir().unwrap();
        std::fs::write(
            leaf.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\n[dependencies]\nnros-board-mps2-an385-freertos = \"*\"\n",
        )
        .unwrap();
        let board = board_of_cargo_leaf(leaf.path(), &root)
            .expect("attributed")
            .expect("one board crate");
        let facts = resolve_one("t", &root, "test", &board, &BTreeMap::new(), &|_| None).unwrap();
        assert_eq!(
            facts.get("FREERTOS_PORT").map(String::as_str),
            Some("GCC_ARM_CM3")
        );
        assert!(
            facts
                .get("FREERTOS_CONFIG_DIR")
                .is_some_and(|d| d.ends_with("nros-board-mps2-an385-freertos/config")),
            "{facts:?}"
        );
    }

    /// Issue 0941 — the configures with no board facts to carry are an
    /// ANSWER (`Some(reason)`), never a guess and never an error; anything that
    /// names a board or an image is not among them.
    #[test]
    fn only_a_configure_that_names_no_board_has_nothing_to_carry() {
        let root = repo_root();
        let args = |board: Option<&str>| BoardFactsArgs {
            path: PathBuf::new(),
            nano_ros_path: None,
            deploy: None,
            board: board.map(str::to_string),
        };
        // A guest project: no system.toml, no Cargo.toml.
        let guest = tempfile::tempdir().unwrap();
        let why = nothing_to_carry(guest.path(), &root, false, &args(None)).unwrap();
        assert!(why.is_some_and(|w| w.contains("nothing names a board")));
        // A cargo dir on no board crate (a host tool): nothing, not an error.
        let host = tempfile::tempdir().unwrap();
        std::fs::write(host.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        assert_eq!(board_of_cargo_leaf(host.path(), &root).unwrap(), None);
        assert!(
            nothing_to_carry(host.path(), &root, false, &args(None))
                .unwrap()
                .is_some()
        );
        // A bringup that declares no image builds nothing for a board.
        let bringup = tempfile::tempdir().unwrap();
        std::fs::write(
            bringup.path().join("system.toml"),
            "[system]\nname = \"b\"\nrmw = \"zenoh\"\ndomain_id = 0\n",
        )
        .unwrap();
        assert!(
            nothing_to_carry(bringup.path(), &root, true, &args(None))
                .unwrap()
                .is_some_and(|w| w.contains("[image.*]"))
        );
        // Naming a board is never "nothing": it must resolve or fail.
        assert_eq!(
            nothing_to_carry(guest.path(), &root, false, &args(Some("linux"))).unwrap(),
            None
        );
        // An image that exists must resolve, too.
        std::fs::write(
            bringup.path().join("system.toml"),
            "[system]\nname = \"b\"\nrmw = \"zenoh\"\ndomain_id = 0\n\n[image.x]\nboard = \"no-such-board\"\n",
        )
        .unwrap();
        assert_eq!(
            nothing_to_carry(bringup.path(), &root, true, &args(None)).unwrap(),
            None
        );
    }

    fn ws_with(system: &str) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("system.toml"), system).unwrap();
        d
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            // packages/cli/nros-cli-core -> cli -> packages -> repo root
            .ancestors()
            .nth(3)
            .expect("repo root")
            .to_path_buf()
    }

    const FREERTOS_WS: &str = r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[image.freertos]
board = "mps2-an385-freertos"

[board_config."mps2-an385-freertos"]
netstack = "lwip"
sdk = { freertos = "{env:FREERTOS_DIR}", lwip = "{env:LWIP_DIR}" }
"#;

    /// phase-400 W1 — a platform package in the USER's workspace is found
    /// WITHOUT anyone exporting `NROS_PLATFORMS_DIR`.
    ///
    /// The assertion is on the emitted search path rather than on a built
    /// artifact because this seam's job ends at handing the path down; that the
    /// resolver then honours it is `platform_config`'s own coverage.
    #[test]
    fn workspace_platform_package_joins_the_search_path() {
        let ws = ws_with(FREERTOS_WS);
        let pkg = ws.path().join("src/nros-platform-acme");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join(PLATFORM_CONFIG_FILENAME), "names = [\"acme\"]\n").unwrap();

        let env = |k: &str| match k {
            "FREERTOS_DIR" => Some("/opt/freertos".to_string()),
            "LWIP_DIR" => Some("/opt/lwip".to_string()),
            _ => None,
        };
        let facts = resolve(ws.path(), &repo_root(), None, None, &env).expect("resolves");
        let path = facts
            .get("NROS_PLATFORMS_DIR")
            .expect("workspace platform packages put a root on the search path");
        let want = ws.path().join("src").display().to_string();
        assert!(
            path.split(':').any(|p| p == want),
            "search path {path} is missing the workspace root {want}"
        );
    }

    /// A caller's own value stays AHEAD of what discovery found — the override
    /// is still an override.
    #[test]
    fn explicit_platforms_dir_outranks_discovery() {
        let ws = ws_with(FREERTOS_WS);
        let pkg = ws.path().join("src/nros-platform-acme");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join(PLATFORM_CONFIG_FILENAME), "names = [\"acme\"]\n").unwrap();

        let env = |k: &str| match k {
            "FREERTOS_DIR" => Some("/opt/freertos".to_string()),
            "LWIP_DIR" => Some("/opt/lwip".to_string()),
            "NROS_PLATFORMS_DIR" => Some("/opt/mine".to_string()),
            _ => None,
        };
        let facts = resolve(ws.path(), &repo_root(), None, None, &env).expect("resolves");
        let path = facts.get("NROS_PLATFORMS_DIR").expect("set");
        assert!(
            path.starts_with("/opt/mine:"),
            "explicit root must come first, got {path}"
        );
    }

    /// A workspace with no platform package of its own emits nothing — the
    /// in-tree default search path is not something this seam should restate.
    #[test]
    fn plain_workspace_adds_no_search_path() {
        let ws = ws_with(FREERTOS_WS);
        let env = |k: &str| match k {
            "FREERTOS_DIR" => Some("/opt/freertos".to_string()),
            "LWIP_DIR" => Some("/opt/lwip".to_string()),
            _ => None,
        };
        let facts = resolve(ws.path(), &repo_root(), None, None, &env).expect("resolves");
        assert_eq!(facts.get("NROS_PLATFORMS_DIR"), None);
    }

    /// The whole point of the wave: one call, both halves, in the shape a
    /// cargo invoker can export.
    #[test]
    fn resolves_board_rung_and_site_config_together() {
        let ws = ws_with(FREERTOS_WS);
        let env = |k: &str| match k {
            "FREERTOS_DIR" => Some("/opt/freertos".to_string()),
            "LWIP_DIR" => Some("/opt/lwip".to_string()),
            _ => None,
        };
        let facts = resolve(ws.path(), &repo_root(), None, None, &env).expect("resolves");

        assert_eq!(
            facts.get("NROS_BOARD").map(String::as_str),
            Some("mps2-an385-freertos")
        );
        assert_eq!(
            facts.get("NROS_NETSTACK"),
            None,
            "issue 1063 — the netstack is VALIDATED, never emitted: nothing read \
             it, and a per-board cmake file already knows its own stack"
        );
        assert_eq!(
            facts.get("NROS_SDK_FREERTOS").map(String::as_str),
            Some("/opt/freertos")
        );
        assert_eq!(
            facts.get("NROS_SDK_LWIP").map(String::as_str),
            Some("/opt/lwip")
        );
        // The board rung's own carrier — an ABSOLUTE path, because the consumer
        // is a build script whose CWD is its own crate, not the leaf.
        let toml_path = facts.get("NROS_BOARD_TOML").expect("board rung delivered");
        assert!(toml_path.ends_with("nros-board.toml"), "{toml_path}");
        assert!(Path::new(toml_path).is_absolute(), "{toml_path}");
        assert!(Path::new(toml_path).is_file(), "{toml_path} does not exist");
    }

    /// W4's domain is enforced at THIS seam — the one place that holds the
    /// board descriptor and the deploy's request at the same time.
    #[test]
    /// Issue 1063 made this test load-bearing in a way it was not before.
    ///
    /// The `NROS_NETSTACK` emission is gone, so `resolve_netstack`'s Ok value is
    /// now unused — which means a future edit could delete the CALL and every
    /// other test in this module would still pass. This one would not. It is
    /// what keeps the validation alive.
    ///
    /// The message wording is also a contract, not a nicety:
    /// `NanoRosBoardFacts.cmake` matches on "does not support netstack" /
    /// "supported_netstacks" to turn this into a configure FATAL_ERROR.
    fn an_unsupported_netstack_fails_the_resolution() {
        let ws = ws_with(&FREERTOS_WS.replace("\"lwip\"", "\"netxduo\""));
        let err = resolve(ws.path(), &repo_root(), None, None, &|_| None)
            .expect_err("netxduo is not in the freertos board's domain");
        let msg = format!("{err:#}");
        assert!(msg.contains("netxduo") && msg.contains("lwip"), "{msg}");
    }

    /// An unset `{env:VAR}` is an error, not an empty string: a silently blank
    /// SDK root becomes a compile failure inside a vendor tree.
    #[test]
    fn an_unset_env_reference_is_refused() {
        let ws = ws_with(FREERTOS_WS);
        let err = resolve(ws.path(), &repo_root(), None, None, &|_| None).expect_err("no env set");
        assert!(format!("{err:#}").contains("FREERTOS_DIR"), "{err:#}");
    }

    /// Two deploy blocks on the same board are ONE answer — `examples/workspaces/mixed`
    /// ships exactly that pair, and refusing it would block every cmake build of
    /// that workspace over a distinction with no consequence.
    ///
    /// Under a board-keyed site table they cannot even differ: both reach the
    /// same `[board_config.*]` block. This used to need a per-block site table
    /// on each deploy, which is the duplication issue 0951 removed.
    #[test]
    fn duplicate_deploys_that_agree_are_not_ambiguous() {
        let ws = ws_with(&format!(
            "{FREERTOS_WS}\n[image.mps2-an385-freertos]\nboard = \"mps2-an385-freertos\"\n\
             rmw = \"zenoh\"\n"
        ));
        let env = |k: &str| match k {
            "FREERTOS_DIR" => Some("/opt/freertos".to_string()),
            "LWIP_DIR" => Some("/opt/lwip".to_string()),
            _ => None,
        };
        let facts = resolve(
            ws.path(),
            &repo_root(),
            None,
            Some("mps2-an385-freertos"),
            &env,
        )
        .expect("agreeing duplicates resolve");
        assert_eq!(
            facts.get("NROS_NETSTACK"),
            None,
            "issue 1063 — the netstack is VALIDATED, never emitted: nothing read \
             it, and a per-board cmake file already knows its own stack"
        );
    }

    /// …and DISAGREEING duplicates are no longer REFUSED — they are
    /// UNREPRESENTABLE.
    ///
    /// Two deploy blocks naming one board used to carry a site table each, so
    /// they could state different SDK roots for the same hardware; `resolve`
    /// compared the resolutions and refused a conflict. Keying the table by
    /// board deletes the shape instead of detecting it, which is why the
    /// agree/disagree comparison in `resolve` now has nothing to catch. This
    /// test is what says that is deliberate rather than a lost check.
    #[test]
    fn two_deploys_on_one_board_cannot_disagree() {
        let ws = ws_with(&format!(
            "{FREERTOS_WS}\n[image.other]\nboard = \"mps2-an385-freertos\"\nrmw = \"zenoh\"\n"
        ));
        let env = |k: &str| match k {
            "FREERTOS_DIR" => Some("/opt/freertos".to_string()),
            "LWIP_DIR" => Some("/opt/lwip".to_string()),
            _ => None,
        };
        let by_board = resolve(
            ws.path(),
            &repo_root(),
            None,
            Some("mps2-an385-freertos"),
            &env,
        )
        .expect("one board, one answer");
        let by_name =
            resolve(ws.path(), &repo_root(), Some("other"), None, &env).expect("named deploy");
        assert_eq!(by_board, by_name, "the board decides, not the deploy key");
        assert_eq!(
            by_board.get("NROS_SDK_FREERTOS").map(String::as_str),
            Some("/opt/freertos")
        );
    }

    /// issue 0755 — `--deploy` still selects, but among DIFFERENT BOARDS.
    ///
    /// That is what threading it from the cmake wrapper buys: one bringup with
    /// deploys for posix + fvp + hardware turns board facts into a silent skip
    /// without it. What it no longer arbitrates is two spellings of ONE board —
    /// see `two_deploys_on_one_board_cannot_disagree`.
    #[test]
    fn naming_the_deploy_picks_among_boards() {
        let ws = ws_with(&format!(
            "{FREERTOS_WS}\n[image.nuttx]\nboard = \"qemu-armv7a-nuttx\"\nrmw = \"zenoh\"\n\n\
             [board_config.\"qemu-armv7a-nuttx\"]\n\
             sdk = {{ nuttx = \"{{env:NUTTX_DIR}}\" }}\n"
        ));
        let env = |k: &str| match k {
            "FREERTOS_DIR" => Some("/opt/freertos".to_string()),
            "LWIP_DIR" => Some("/opt/lwip".to_string()),
            "NUTTX_DIR" => Some("/opt/nuttx".to_string()),
            _ => None,
        };
        let nuttx = resolve(ws.path(), &repo_root(), Some("nuttx"), None, &env)
            .expect("the named deploy resolves");
        assert_eq!(
            nuttx.get("NROS_SDK_NUTTX").map(String::as_str),
            Some("/opt/nuttx")
        );
        assert!(
            !nuttx.contains_key("NROS_SDK_FREERTOS"),
            "the other board's site block must not leak in: {nuttx:?}"
        );
    }

    /// A workspace with several bringups (`examples/workspaces/realtime-c`
    /// has two) is resolved by the deploy or board the configure names; with
    /// neither, or with a name no bringup has, it is still refused.
    #[test]
    fn the_named_deploy_picks_the_bringup() {
        let ws = tempfile::tempdir().unwrap();
        for (dir, body) in [
            (
                "demo_bringup",
                "[system]\nname = \"d\"\nrmw = \"zenoh\"\ndomain_id = 0\n[image.freertos]\nboard = \"mps2-an385-freertos\"\n",
            ),
            (
                "smp_bringup",
                "[system]\nname = \"s\"\nrmw = \"zenoh\"\ndomain_id = 0\n[image.zephyr_smp]\nboard = \"qemu-cortex-a53\"\n",
            ),
        ] {
            let d = ws.path().join("src").join(dir);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("system.toml"), body).unwrap();
        }
        let root = repo_root();
        let facts = resolve(ws.path(), &root, Some("freertos"), None, &|_| None)
            .expect("the named image picks demo_bringup");
        assert_eq!(
            facts.get("FREERTOS_PORT").map(String::as_str),
            Some("GCC_ARM_CM3")
        );
        let by_board = resolve(ws.path(), &root, None, Some("qemu-cortex-a53"), &|_| None)
            .expect("the named board picks smp_bringup");
        assert_eq!(
            by_board.get("NROS_BOARD").map(String::as_str),
            Some("qemu-cortex-a53")
        );
        let none = resolve(ws.path(), &root, None, None, &|_| None).unwrap_err();
        assert!(none.to_string().contains("2 bringups"), "{none}");
        let miss = resolve(ws.path(), &root, Some("nope"), None, &|_| None).unwrap_err();
        assert!(miss.to_string().contains("none of 2 bringups"), "{miss}");
    }

    /// Issue 2199 — a single-package leaf builds the image its own rule picks
    /// (`[system] default_images`), so a configure that names none resolves
    /// THAT image's board — the one `nros ws leaf-system` hands the Zephyr hook
    /// — rather than every image the file declares.
    #[test]
    fn a_leaf_resolves_its_default_image_when_none_is_named() {
        let ws = ws_with(
            "[system]\nname = \"l\"\nrmw = \"zenoh\"\ndomain_id = 0\n\
             default_images = [\"smp\"]\n\
             [image.a_freertos]\nboard = \"mps2-an385-freertos\"\n\
             [image.smp]\nboard = \"qemu-cortex-a53\"\n",
        );
        std::fs::write(ws.path().join("CMakeLists.txt"), "").unwrap();
        let facts = resolve(ws.path(), &repo_root(), None, None, &|_| None)
            .expect("the leaf's default image resolves");
        assert_eq!(
            facts.get("NROS_BOARD").map(String::as_str),
            Some("qemu-cortex-a53")
        );
        // A named deploy still chooses for itself, default or not.
        let named = resolve(ws.path(), &repo_root(), Some("a_freertos"), None, &|_| None)
            .expect("the named image resolves");
        assert_eq!(
            named.get("NROS_BOARD").map(String::as_str),
            Some("mps2-an385-freertos")
        );
    }

    /// Issue 2199 — the board refusal names the image that is being built, and
    /// when nothing chose one it names every image on that board, never the
    /// first in key order alone.
    #[test]
    fn an_unknown_board_refusal_names_the_right_images() {
        let body = "[system]\nname = \"l\"\nrmw = \"zenoh\"\ndomain_id = 0\n\
                    default_images = [\"xiao_serial\"]\n\
                    [image.xiao_serial]\nboard = \"no-such-board\"\n\
                    [image.xiao_cdc]\nboard = \"no-such-board\"\n";
        let leaf = ws_with(body);
        std::fs::write(leaf.path().join("CMakeLists.txt"), "").unwrap();
        let e = resolve(leaf.path(), &repo_root(), None, None, &|_| None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("(deploy `xiao_serial`)"), "{e}");
        assert!(!e.contains("xiao_cdc"), "{e}");

        // No manifest: a bringup builds every image, so both are named.
        let bringup = ws_with(body);
        let e = resolve(bringup.path(), &repo_root(), None, None, &|_| None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("(deploys `xiao_cdc`, `xiao_serial`)"), "{e}");
    }

    /// The regression this file's own gate demanded and this resolver could
    /// not serve: `examples/workspaces/realtime-rust` has SEVEN images and ZERO
    /// deploy blocks, so selecting candidates from `[deploy.*]` alone (retired since) made its
    /// site table — which `check-site-config` requires — unreachable.
    #[test]
    fn an_image_only_workspace_resolves() {
        let ws = ws_with(
            r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[image.fw]
board = "mps2-an385-freertos"

[board_config."mps2-an385-freertos"]
netstack = "lwip"
sdk = { freertos = "/opt/freertos", lwip = "/opt/lwip" }
"#,
        );
        let facts = resolve(ws.path(), &repo_root(), None, None, &|_| None)
            .expect("an image names the board");
        assert_eq!(
            facts.get("NROS_BOARD").map(String::as_str),
            Some("mps2-an385-freertos")
        );
        assert_eq!(
            facts.get("NROS_NETSTACK"),
            None,
            "issue 1063 — the netstack is VALIDATED, never emitted: nothing read \
             it, and a per-board cmake file already knows its own stack"
        );
        // And both selectors reach it.
        assert!(resolve(ws.path(), &repo_root(), Some("fw"), None, &|_| None).is_ok());
        assert!(
            resolve(
                ws.path(),
                &repo_root(),
                None,
                Some("mps2-an385-freertos"),
                &|_| None
            )
            .is_ok()
        );
    }

    /// Two boards share `packages/boards/nros-board-nuttx/nros-board.toml`, so
    /// a descriptor compared by its FILE answers "same board" for an arm and a
    /// riscv target. `--board rv-virt-nuttx` would then hand the build the
    /// arm board's facts — a wrong answer that reports success.
    #[test]
    fn two_boards_in_one_descriptor_file_stay_distinct() {
        let ws = ws_with(
            r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[image.arm]
board = "qemu-armv7a-nuttx"

[image.riscv]
board = "nuttx-riscv"

[board_config."qemu-armv7a-nuttx"]
sdk = { nuttx = "/opt/arm", nuttx_apps = "/opt/arm-apps" }

[board_config."rv-virt-nuttx"]
sdk = { nuttx = "/opt/riscv", nuttx_apps = "/opt/riscv-apps" }
"#,
        );
        let arm = resolve(
            ws.path(),
            &repo_root(),
            None,
            Some("qemu-armv7a-nuttx"),
            &|_| None,
        )
        .expect("arm resolves");
        assert_eq!(
            arm.get("NROS_SDK_NUTTX").map(String::as_str),
            Some("/opt/arm")
        );
        // `nuttx-riscv` is the descriptor's declared name; `rv-virt-nuttx`
        // is another spelling of the SAME entry, so both must find the riscv
        // block and neither may find the arm one.
        for spelling in ["nuttx-riscv", "rv-virt-nuttx"] {
            let r = resolve(ws.path(), &repo_root(), None, Some(spelling), &|_| None)
                .unwrap_or_else(|e| panic!("{spelling} resolves: {e}"));
            assert_eq!(
                r.get("NROS_SDK_NUTTX").map(String::as_str),
                Some("/opt/riscv"),
                "{spelling} must not resolve the arm board"
            );
        }
    }

    /// The site key is matched by RESOLUTION, not by text — a board has several
    /// legal spellings and every one of them must find the same block. Keying
    /// on the string is issue 0606 one table over: a block authored under the
    /// descriptor's declared name would be invisible to a build spelling the
    /// board by its directory.
    #[test]
    fn a_site_block_under_an_alias_spelling_still_resolves() {
        let ws = ws_with(
            r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[image.freertos]
board = "mps2-an385-freertos"

[board_config.freertos]
netstack = "lwip"
sdk = { freertos = "/opt/freertos" }
"#,
        );
        let facts = resolve(ws.path(), &repo_root(), None, None, &|_| None)
            .expect("the `freertos` key names the same board as the directory spelling");
        assert_eq!(
            facts.get("NROS_NETSTACK"),
            None,
            "issue 1063 — the netstack is VALIDATED, never emitted: nothing read \
             it, and a per-board cmake file already knows its own stack"
        );
        assert_eq!(
            facts.get("NROS_SDK_FREERTOS").map(String::as_str),
            Some("/opt/freertos")
        );
    }

    /// `board = "mps2-an385-freertos"` is the DIRECTORY spelling, which that
    /// descriptor does not carry in `names`. Every in-tree site block uses it.
    #[test]
    fn a_board_named_by_its_directory_still_resolves() {
        let catalog = BoardCatalog::load(&repo_root()).expect("in-tree catalog");
        assert!(resolve_board(&catalog, "mps2-an385-freertos").is_some());
        assert!(
            resolve_board(&catalog, "freertos").is_some(),
            "declared name"
        );
        assert!(resolve_board(&catalog, "no-such-board").is_none());
    }
}
