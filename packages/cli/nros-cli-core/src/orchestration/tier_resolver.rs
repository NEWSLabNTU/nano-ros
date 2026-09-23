//! Tier resolver — re-export shim (Phase 228.E).
//!
//! The resolver + its result types moved to the shared
//! [`nros_orchestration_ir`] crate so the `nros::main!()` proc-macro can run
//! the exact same resolution at compile time (the CLI can't be a proc-macro
//! dep). This module re-exports them so existing `orchestration::tier_resolver::*`
//! references stay valid.
//!
//! The one shape change: [`resolve_tiers`] now takes the decomposed
//! `system.toml` pieces (`tiers`, `node_overrides`, `component_names`,
//! `callback_groups`) instead of a whole `SystemToml`, so the leaf crate stays
//! free of the full CLI config type. See [`crate::cmd::codegen_system`] for the
//! call site that adapts a `SystemToml` to it.

pub use nros_orchestration_ir::{
    DEFAULT_TIER, ResolvedTier, ResolvedTierTable, TierResolveError, resolve_tiers,
};

use super::{
    cargo_metadata_schema::{CallbackGroupDecl, SystemComponentEntry, SystemToml},
    nros_config::NrosConfig,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

/// Phase 228 / 256 W4.2 / 273 W2 — collect each system component's declared
/// callback-group → tier bindings, keyed by the system `[[component]].name`
/// (the instance name `ResolvedTier.members` use).
/// Shared by the C bake (`codegen_system`) and the Rust codegen (`generate`).
///
/// Phase 273 (RFC-0047 W2): `[[component]].group_tiers` is the new source of
/// truth for the group → tier binding. When present it takes priority over the
/// package manifest's `callback_groups` tier field. When absent the manifest is
/// honoured for one release (deprecated path — emit a warning so workspaces can
/// migrate to `system.toml group_tiers`).
///
/// phase-459 W1 (issue 1426): a THIRD source below those two - the cmake
/// keyword `CALLBACK_GROUPS`, read from the `nros-metadata.json` a configure
/// wrote. See [`metadata_callback_groups`] for why a cmake workspace reached
/// neither of the first two, and what that cost.
pub fn collect_callback_groups(
    cfg: &NrosConfig,
    components: &[SystemComponentEntry],
) -> BTreeMap<String, Vec<CallbackGroupDecl>> {
    let from_metadata = metadata_callback_groups(&cfg.workspace_root);
    let mut map = BTreeMap::new();
    for c in components {
        // Phase 273 (W2): prefer system.toml [[component]].group_tiers (RFC-0047).
        if !c.group_tiers.is_empty() {
            let groups: Vec<CallbackGroupDecl> = c
                .group_tiers
                .iter()
                .map(|(id, tier)| CallbackGroupDecl {
                    id: id.clone(),
                    r#type: "MutuallyExclusive".to_string(),
                    tier: tier.clone(),
                })
                .collect();
            map.insert(c.name.clone(), groups);
            continue;
        }

        // Fallback: package-manifest `callback_groups` tier (deprecated — move to
        // [[component]].group_tiers in system.toml).
        //
        // A cmake workspace has no root `Cargo.toml`, so `NrosConfig` gives it
        // `component_packages: BTreeMap::new()` and this rung answers for
        // nothing. That is why the third source below is reached by an `else`
        // and not by an `if`: it is the ONLY rung a C/C++ component has.
        let groups = cfg
            .component_packages
            .get(&c.pkg)
            .map(|pkg| {
                // Single-node pkg -> node_or_component; multi-node pkg -> match by name/class.
                pkg.nros
                    .node_or_component()
                    .filter(|m| !m.callback_groups.is_empty())
                    .map(|m| m.callback_groups.clone())
                    .or_else(|| {
                        pkg.nros
                            .nodes_or_components()
                            .values()
                            .find(|m| {
                                m.name.as_deref() == Some(c.name.as_str())
                                    || m.class.as_deref() == Some(c.class.as_str())
                            })
                            .map(|m| m.callback_groups.clone())
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let has_pkg_tiers = groups.iter().any(|g| g.tier != DEFAULT_TIER);
        if has_pkg_tiers {
            eprintln!(
                "[WARN] nros: package `{}` (component `{}`) has `callback_groups` with a tier \
                 assignment in Cargo.toml. Move it to `system.toml [[component]].group_tiers` \
                 (Phase 273 / RFC-0047). Package-level tier binding is deprecated.",
                c.pkg, c.name
            );
        }
        if !groups.is_empty() {
            map.insert(c.name.clone(), groups);
            continue;
        }

        // phase-459 W1 (issue 1426) - the cmake keyword, the third and last
        // source. Every group binds to `DEFAULT_TIER`: the keyword states WHICH
        // groups the code has, never where they run. That is exactly the input
        // shape `derive_tiers_from_contracts` keys on - a node with groups and
        // no authored binding is the one route on which the rate-monotonic
        // derivation runs (RFC-0052), and a bound group would instead have to
        // name a `[tiers.*]` that this workspace deliberately does not author.
        if let Some(ids) = from_metadata.get(&c.name) {
            map.insert(
                c.name.clone(),
                ids.iter()
                    .map(|id| CallbackGroupDecl {
                        id: id.clone(),
                        r#type: "MutuallyExclusive".to_string(),
                        tier: DEFAULT_TIER.to_string(),
                    })
                    .collect(),
            );
        }
    }
    map
}

/// phase-459 W1 (issue 1426) - `CALLBACK_GROUPS` as a `codegen-system` input.
///
/// `nros_components_register_node(... CALLBACK_GROUPS main)` and
/// `nano_ros_add_node(... CALLBACK_GROUPS ...)` write the group ids into
/// `${CMAKE_BINARY_DIR}/nros-metadata.json` (`NanoRosNodeRegister.cmake`,
/// `_nros_metadata_emit`). Until this wave that file had exactly one consumer,
/// `codegen entry`'s `metadata::enrich_plan`, and `codegen-system` knew only the
/// two sources above - `[[component]].group_tiers`, which a workspace deriving
/// its schedule deliberately does not write, and `cfg.component_packages`, which
/// is empty for every workspace without a root `Cargo.toml`. So a C++ image
/// authored the keyword and every one of its nodes still arrived at
/// `derive_tiers_from_contracts` groupless, went onto the default tier, and the
/// derived schedule was empty. Issue 1426 measured that on the Autoware Safety
/// Island; this is the reader that closes it.
///
/// Returns `component name -> group ids`, where the name is the register call's
/// `EXECUTABLE` / `NAME` - the same string `[[component]].name` carries, which
/// is why the caller can key on it directly.
///
/// # Where it looks, and why not everywhere
///
/// The file is a CONFIGURE output, so it lives in a build tree, not beside the
/// sources: `${CMAKE_BINARY_DIR}/nros-metadata.json`. The search is therefore
/// bounded on purpose rather than a walk of the workspace - a tree that has
/// accumulated build directories will make a recursive scan open hundreds of
/// thousands of them, which is a measured failure mode here and not a
/// hypothetical one. Per root: the root itself, each immediate `build*` child,
/// and each child of those (colcon's `build/<pkg>/`). Three `read_dir` levels,
/// no deeper.
///
/// The roots are the workspace and - when it still looks like a workspace - its
/// parent. The second is not a guess: `nros_system_generate()` passes
/// `--workspace` the PARENT of the bringup package, so a Zephyr image whose
/// bringup is `<ws>/src/<bringup>` bakes with `--workspace <ws>/src` while its
/// `CMAKE_BINARY_DIR` is `<ws>/build-board`. One level up finds it; the walk
/// stops as soon as a candidate carries no workspace marker, so it never
/// wanders into a home directory.
///
/// # Merging several files, and the empty list
///
/// A workspace commonly holds several build trees (the island has four). Group
/// ids are UNIONED across the files that name a component, because an empty
/// `callback_groups` array is "this configure said nothing", not "this
/// component has no groups" - the island's own `build-board/nros-metadata.json`
/// carries `[]` for all four components simply because it never wrote the
/// keyword. Union is also the only monotone choice: a stale tree can add a
/// group that no longer exists (and W6 makes code/keyword disagreement a
/// refusal), but it can never silently delete one and shrink a schedule.
///
/// Unreadable or unparseable files are SKIPPED, silently. This is a
/// best-effort enrichment of a bake that must keep working for every workspace
/// that has no such file at all, and it is the same policy
/// [`load_workspace_metadata`](super::model_ingest::load_workspace_metadata)
/// applies to the source-metadata sidecars.
pub fn metadata_callback_groups(ws_root: &Path) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for path in metadata_doc_paths(ws_root) {
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(doc) = serde_json::from_str::<CmakeMetadataDoc>(&raw) else {
            continue;
        };
        for c in doc.components {
            if c.callback_groups.is_empty() {
                continue;
            }
            out.entry(c.name).or_default().extend(c.callback_groups);
        }
    }
    out.into_iter()
        .map(|(name, ids)| (name, ids.into_iter().collect()))
        .collect()
}

/// The `nros-metadata.json` docs reachable from `ws_root` - see
/// [`metadata_callback_groups`] for the bound and the reason for it.
fn metadata_doc_paths(ws_root: &Path) -> Vec<PathBuf> {
    /// One level above the workspace, and no further: that is the whole of the
    /// `--workspace <ws>/src` + `CMAKE_BINARY_DIR <ws>/build-board` gap.
    const ANCESTORS: usize = 1;
    /// What makes a directory a plausible workspace root rather than whatever
    /// happens to contain one. The generated `Cargo.toml` / `CMakeLists.txt` of
    /// an RFC-0065 workspace are gitignored, so `.colcon_workspace` is the
    /// tracked marker and has to be in the list.
    const MARKERS: [&str; 4] = [
        ".colcon_workspace",
        "Cargo.toml",
        "CMakeLists.txt",
        "package.xml",
    ];

    let mut roots = vec![ws_root.to_path_buf()];
    let mut cur = ws_root.to_path_buf();
    for _ in 0..ANCESTORS {
        let Some(parent) = cur.parent().map(Path::to_path_buf) else {
            break;
        };
        if !MARKERS.iter().any(|m| parent.join(m).exists()) {
            break;
        }
        roots.push(parent.clone());
        cur = parent;
    }

    let mut out = Vec::new();
    let mut push_if_file = |p: PathBuf| {
        if p.is_file() && !out.contains(&p) {
            out.push(p);
        }
    };
    for root in roots {
        push_if_file(root.join("nros-metadata.json"));
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for build_dir in entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| is_build_dir(p))
        {
            push_if_file(build_dir.join("nros-metadata.json"));
            let Ok(inner) = std::fs::read_dir(&build_dir) else {
                continue;
            };
            for pkg_dir in inner.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
                push_if_file(pkg_dir.join("nros-metadata.json"));
            }
        }
    }
    out
}

/// A `build` / `build-board` / `build_native` style directory. Name-based
/// because that is the only thing that distinguishes a build tree before it is
/// opened, and opening every directory is the cost this bound exists to avoid.
fn is_build_dir(p: &Path) -> bool {
    p.is_dir()
        && p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == "build" || n.starts_with("build-") || n.starts_with("build_"))
}

/// The two fields of `nros-metadata.json` this reader needs.
///
/// Not [`crate::codegen::entry::metadata::ComponentIndex`], which parses the
/// same file for the typed entry: that index is keyed by `(pkg, exec)` for
/// CLASS enrichment and REFUSES a row from which no pkg can be derived. A
/// scheduling input must not make a bake fail because a stale build tree holds
/// a row with an unqualified class, so this reads the two fields it needs and
/// ignores the rest - `serde` skips unknown keys here by design.
#[derive(serde::Deserialize)]
struct CmakeMetadataDoc {
    #[serde(default)]
    components: Vec<CmakeComponentMeta>,
}

#[derive(serde::Deserialize)]
struct CmakeComponentMeta {
    name: String,
    #[serde(default)]
    callback_groups: Vec<String>,
}

/// Adapt a full [`SystemToml`] + its components' `callback_groups` to the
/// decomposed [`resolve_tiers`] signature.
pub fn resolve_system_tiers(
    system: &SystemToml,
    callback_groups: &BTreeMap<String, Vec<CallbackGroupDecl>>,
    target_rtos: &str,
) -> Result<ResolvedTierTable, TierResolveError> {
    let component_names: BTreeSet<&str> =
        system.components.iter().map(|c| c.name.as_str()).collect();
    resolve_tiers(
        &system.tiers,
        &system.node_overrides,
        &component_names,
        callback_groups,
        target_rtos,
    )
}

/// The board id the selected target names, and where it was written.
///
/// Issue 0951 — `[image.<t>].board` first, then the DEPRECATED
/// `[deploy.<t>].board`, then `[deploy.<t>].kind`. Images are the buildable
/// unit, so a workspace that has migrated has its board only there. The `kind`
/// rung is last because it is the coarsest: it was only ever a stand-in for a
/// board name, and it cannot separate the two boards it would need to
/// (`qemu-armv7a-nuttx` vs `rv-virt-nuttx`).
///
/// `None` when there is no target, or when the target names no image or
/// deploy block. The second case used to be every bake the Zephyr module made
/// — `nros_system_generate()` passed a `--target zephyr-<rmw>` it had
/// synthesised from Kconfig — so a tiered Zephyr image read the host's tier
/// tables (issue 1312, FIXED). The module names its ENTRY now
/// (`codegen-system --for-entry`) and the image that claims it answers here.
pub fn target_board_id(system: &SystemToml, target: Option<&str>) -> Option<(String, String)> {
    let t = target?;
    if let Some(board) = system.image_for(t).and_then(|img| img.board) {
        return Some((format!("[image.{t}] board"), board));
    }
    let deploy = system.deploy.get(t)?;
    if let Some(board) = deploy.board.clone() {
        return Some((format!("[deploy.{t}] board"), board));
    }
    // `kind` is the DEPLOY-kind vocabulary, not a board id. `self` is its word
    // for "this host", which 22 in-tree `system.toml`s use. It names no board,
    // so it gets the host default, which is the right answer for it and not a
    // guess. Any other kind (`zephyr`) is treated as a stand-in for a board id
    // and resolved strictly through the catalog. So `embedded` with no board is
    // refused, where the substring match quietly called it the host.
    match deploy.kind.as_deref() {
        None | Some(DEPLOY_KIND_SELF) => None,
        Some(kind) => Some((format!("[deploy.{t}] kind"), kind.to_string())),
    }
}

/// The deploy kind meaning "the host this runs on" (`[deploy.<t>] kind`).
const DEPLOY_KIND_SELF: &str = "self";

/// The `[tiers.<name>.<rtos>]` key for the selected target, read from the BOARD
/// CATALOG (issue 1285 follow-up).
///
/// The board id from [`target_board_id`] is an IMAGE board id. That is the
/// catalog's namespace (`native_sim/native/64`, `qemu-armv7a-nsh`,
/// `esp32c3`), not the entry key table. So it resolves through
/// [`resolve_board_id`](super::image::resolve_board_id), the same rule
/// `nros build` uses for the image. The descriptor's `platform` then gives the
/// key through [`PlatformKind::tier_rtos_key`](super::board_descriptor::PlatformKind::tier_rtos_key).
///
/// This used to be a SUBSTRING match on the id with a `posix` fallback. That
/// read `native_sim/native/64`, a Zephyr board, as the host, and so would have
/// baked `[tiers.*.posix]` priorities into a Zephyr image. The same happened to
/// `qemu-cortex-a53`, `s32z270` and `an536`.
///
/// - No board id (no `--target`, or a target naming no image or deploy): the
///   host's `posix`. That is the documented default, and nothing here is a guess
///   about a board.
/// - An id the catalog does not know, or one several descriptors claim: an
///   ERROR naming the known boards. `codegen-system` is a verb and can refuse,
///   and a wrong sub-table is a silent scheduling bug.
/// - A known board with no RTOS (`bare-metal`, `esp32`): `NO_RTOS_TIER_KEY`.
pub fn derive_target_rtos(
    system: &SystemToml,
    target: Option<&str>,
    catalog: &super::board_descriptor::BoardCatalog,
) -> Result<&'static str, String> {
    Ok(derive_target_platform(system, target, catalog)?.tier_rtos_key())
}

/// The PLATFORM the selected target's image runs on, from the BOARD CATALOG.
///
/// Issue 1397 — the resolution [`derive_target_rtos`] is a view of. A bake
/// needs more than the tier key: the executor capacity check needs the
/// platform's NAME (the RFC-0049 platforms-tree vocabulary, for the `max_cbs`
/// ladder) and whether the board honors per-entry sizing. Those used to be
/// read out of the `--target` BLOCK NAME, which is a third vocabulary and
/// answers neither question — it is right for `native`/`posix` only because an
/// image is conventionally named after its platform.
///
/// One resolution, so every consumer agrees about what the image IS. The
/// no-board default is the host, as documented on [`target_board_id`]: a
/// `kind = "self"` deploy genuinely names no board, and a target naming no
/// block at all takes the system-wide default, which for every other question
/// this bake answers is the host too. Naming the image is what replaces the
/// default with an answer — see issue 1312.
pub fn derive_target_platform(
    system: &SystemToml,
    target: Option<&str>,
    catalog: &super::board_descriptor::BoardCatalog,
) -> Result<super::board_descriptor::PlatformKind, String> {
    let Some((origin, board)) = target_board_id(system, target) else {
        return Ok(super::board_descriptor::PlatformKind::Posix);
    };
    let descriptor = super::image::resolve_board_id(catalog, &origin, &board)?;
    Ok(descriptor.platform)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue 0951 — `derive_target_rtos` had NO direct test for two phases.
    /// It decides which `[tiers.<name>.<rtos>]` sub-table is read, so a wrong
    /// answer silently applies another RTOS's scheduling parameters.
    mod target_rtos {
        use super::*;

        fn sys(body: &str) -> SystemToml {
            toml::from_str(&format!(
                "[system]\nname=\"d\"\nrmw=\"zenoh\"\ndomain_id=0\n{body}"
            ))
            .expect("fixture parses")
        }

        /// The in-tree board catalog, with no `$NROS_EXTRA_BOARD_PATH` roots
        /// (env is process-global and racy under a parallel runner).
        fn catalog() -> crate::orchestration::board_descriptor::BoardCatalog {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            crate::orchestration::board_descriptor::BoardCatalog::load_with_extra(&root, &[])
                .expect("in-tree board catalog")
        }

        fn rtos(s: &SystemToml, target: Option<&str>) -> Result<&'static str, String> {
            derive_target_rtos(s, target, &catalog())
        }

        #[test]
        fn an_image_board_decides() {
            let s = sys("[image.fw]\nboard=\"mps2-an385-freertos\"\n");
            assert_eq!(rtos(&s, Some("fw")), Ok("freertos"));
        }

        #[test]
        fn the_image_outranks_a_deploy_naming_another_board() {
            // Mid-migration a workspace carries both. The image is the half
            // that survives, so it decides — reading the deploy would resolve
            // the RTOS of a block that is about to be deleted.
            let s = sys("[image.fw]\nboard=\"qemu-armv7a-nuttx\"\n\
                 [deploy.fw]\nkind=\"embedded\"\nboard=\"mps2-an385-freertos\"\n");
            assert_eq!(rtos(&s, Some("fw")), Ok("nuttx"));
        }

        #[test]
        fn a_deploy_board_still_answers_when_no_image_exists() {
            let s = sys("[deploy.fw]\nkind=\"embedded\"\nboard=\"threadx-linux\"\n");
            assert_eq!(rtos(&s, Some("fw")), Ok("threadx"));
        }

        /// The coarsest rung, and the reason it is last: `kind` was only ever a
        /// stand-in for a board name. It resolves through the catalog like
        /// every other rung (`zephyr` is a descriptor name).
        #[test]
        fn kind_is_the_last_resort() {
            let s = sys("[deploy.fw]\nkind=\"zephyr\"\n");
            assert_eq!(rtos(&s, Some("fw")), Ok("zephyr"));
        }

        /// No board id means the documented host default. It is not a lookup,
        /// so it needs no catalog. `zephyr-zenoh` is what the Zephyr module
        /// USED to pass (issue 1312): it names no block, so it landed here —
        /// which is why the fix was to make the module name its image, not to
        /// teach this function to read a platform out of the string.
        #[test]
        fn no_board_id_is_the_host_default() {
            let s = sys("[image.fw]\nboard=\"native\"\n");
            let empty = crate::orchestration::board_descriptor::BoardCatalog::default();
            assert_eq!(rtos(&s, Some("fw")), Ok("posix"));
            for target in [Some("nope"), Some("zephyr-zenoh"), None] {
                assert_eq!(derive_target_rtos(&s, target, &empty), Ok("posix"));
            }
        }

        /// `[image_defaults]` folds under the block here as everywhere else —
        /// reaching into `system.image` directly would miss it.
        #[test]
        fn the_defaults_table_supplies_a_board_the_block_omits() {
            let s =
                sys("[image_defaults]\nboard=\"rv-virt-nuttx\"\n[image.fw]\nprofile=\"release\"\n");
            assert_eq!(rtos(&s, Some("fw")), Ok("nuttx"));
        }

        /// Issue 1285 follow-up. Every image board id an in-tree `system.toml`
        /// names, with the key its descriptor's platform gives. `""` is a board
        /// with no RTOS (bare-metal, ESP32).
        #[test]
        fn every_in_tree_image_board_reads_its_platform() {
            for (id, want) in [
                ("native", "posix"),
                ("zephyr", "zephyr"),
                ("native_sim/native/64", "zephyr"),
                ("threadx-linux", "threadx"),
                ("rv-virt-threadx", "threadx"),
                ("mps2-an385-freertos", "freertos"),
                ("freertos", "freertos"),
                ("freertos-posix", "freertos"),
                ("s32z270-freertos", "freertos"),
                ("mps3-an536-freertos", "freertos"),
                ("qemu-armv7a-nuttx", "nuttx"),
                ("qemu-armv7a-nsh", "nuttx"),
                ("nuttx", "nuttx"),
                ("rv-virt-nuttx", "nuttx"),
                ("nuttx-riscv", "nuttx"),
                ("rtic-mps2-an385", ""),
                ("qemu-mps2-an385", ""),
                ("esp32-c3-baremetal", ""),
                ("esp32c3", ""),
            ] {
                let s = sys(&format!("[image.fw]\nboard=\"{id}\"\n"));
                assert_eq!(rtos(&s, Some("fw")), Ok(want), "`{id}`");
            }
        }

        /// Ids the substring match answered wrongly. `native_sim/native/64`
        /// is a Zephyr board (`packages/boards/zephyr`), and `native` in its
        /// name is the ROLE (a host process), not the platform. So its tiers
        /// are Zephyr `k_thread`s and read `[tiers.*.zephyr]`.
        #[test]
        fn substring_victims_read_their_platform() {
            for (id, was, now) in [
                ("native_sim/native/64", "posix", "zephyr"),
                ("qemu-cortex-a53", "posix", "zephyr"),
                ("mps2-an385-zephyr", "zephyr", "zephyr"),
                ("s32z270", "posix", "freertos"),
                ("an536", "posix", "freertos"),
                ("rtic-mps2-an385", "posix", ""),
                ("esp32c3", "posix", ""),
            ] {
                let s = sys(&format!("[image.fw]\nboard=\"{id}\"\n"));
                assert_eq!(rtos(&s, Some("fw")), Ok(now), "`{id}` (was `{was}`)");
            }
        }

        /// An id no descriptor claims is refused, naming the line and the known
        /// boards. That includes an id whose NAME contains an RTOS: the old
        /// match read `my-freertos-board` as FreeRTOS.
        #[test]
        fn an_unknown_board_is_refused_naming_the_known_ones() {
            let s = sys("[image.fw]\nboard=\"my-freertos-board\"\n");
            let err = rtos(&s, Some("fw")).expect_err("an unknown board must not resolve");
            assert!(
                err.contains("`[image.fw] board = \"my-freertos-board\"` matches no board"),
                "{err}"
            );
            // Descriptor NAMES. (`mps2-an385-freertos` resolves too, through
            // the directory alias, but the list names only `names` entries.)
            for known in ["freertos", "native_sim/native/64", "native"] {
                assert!(err.contains(known), "message omits `{known}`: {err}");
            }
            let s = sys("[deploy.fw]\nkind=\"embedded\"\n");
            let err = rtos(&s, Some("fw")).expect_err("`embedded` is no board");
            assert!(err.contains("[deploy.fw] kind"), "{err}");
        }

        /// `kind` is the deploy-KIND vocabulary. `self` (22 in-tree
        /// `system.toml`s, and the `codegen_system` test workspaces) means "this
        /// host": it names no board, and it gets the host default with no
        /// catalog. Any other kind is a stand-in for a board id and resolves
        /// strictly.
        #[test]
        fn deploy_kind_self_is_the_host_and_names_no_board() {
            let s = sys("[deploy.native]\nkind=\"self\"\n");
            assert_eq!(target_board_id(&s, Some("native")), None);
            let empty = crate::orchestration::board_descriptor::BoardCatalog::default();
            assert_eq!(derive_target_rtos(&s, Some("native"), &empty), Ok("posix"));
            // A board beside `self` still decides.
            let s = sys("[deploy.native]\nkind=\"self\"\nboard=\"native_sim/native/64\"\n");
            assert_eq!(rtos(&s, Some("native")), Ok("zephyr"));
        }

        /// `threadx` names both the Linux simulation and the riscv64 board. An
        /// id several descriptors claim is refused, as `nros build` refuses it.
        #[test]
        fn a_board_several_descriptors_claim_is_refused() {
            let s = sys("[image.fw]\nboard=\"threadx\"\n");
            let err = rtos(&s, Some("fw")).expect_err("ambiguous");
            assert!(err.contains("ambiguous"), "{err}");
        }
    }
    use crate::orchestration::{
        cargo_metadata_schema::{SystemComponentEntry, SystemToml},
        nros_config::NrosConfig,
    };

    /// Phase 273 (W2) — `collect_callback_groups` prefers `[[component]].group_tiers`
    /// over the package-manifest `callback_groups` tier. An empty `NrosConfig`
    /// (no component packages) is sufficient because the `group_tiers` path never
    /// touches `cfg.component_packages`.
    #[test]
    fn collect_prefers_group_tiers_over_pkg_manifest() {
        let cfg = NrosConfig::default();
        let mut gt = BTreeMap::new();
        gt.insert("ctrl".to_string(), "high".to_string());
        gt.insert("telem".to_string(), "low".to_string());
        let components = vec![SystemComponentEntry {
            pkg: "ctrl_pkg".to_string(),
            class: "ctrl_pkg::Ctrl".to_string(),
            name: "ctrl_node".to_string(),
            group_tiers: gt,
            params: Default::default(),
            params_files: Vec::new(),
            entities: None,
            dispatch: None,
        }];
        let map = collect_callback_groups(&cfg, &components);
        let decls = map.get("ctrl_node").expect("ctrl_node must be in map");
        assert_eq!(decls.len(), 2);
        // Both groups resolved from group_tiers.
        let by_id: BTreeMap<&str, &str> = decls
            .iter()
            .map(|d| (d.id.as_str(), d.tier.as_str()))
            .collect();
        assert_eq!(by_id["ctrl"], "high");
        assert_eq!(by_id["telem"], "low");
    }

    /// Phase 273 (W2) — `resolve_system_tiers` with `group_tiers` produces the
    /// expected `(component, group) → sched_context` mapping without needing
    /// `[[node_overrides]]`.
    #[test]
    fn resolve_system_tiers_from_group_tiers() {
        let system_toml_str = r#"
[system]
name = "test"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "ctrl_pkg"
class = "ctrl_pkg::Ctrl"
name = "ctrl_node"
group_tiers = { ctrl = "high" }

[[component]]
pkg = "telem_pkg"
class = "telem_pkg::Telem"
name = "telem_node"
group_tiers = { telem = "low" }

[tiers.high]
spin_period_us = 10000
[tiers.high.posix]
priority = 80

[tiers.low]
spin_period_us = 100000
[tiers.low.posix]
priority = 10
"#;
        let system: SystemToml = toml::from_str(system_toml_str).expect("parse system.toml");
        let cfg = NrosConfig::default();
        let callback_groups = collect_callback_groups(&cfg, &system.components);
        let table =
            resolve_system_tiers(&system, &callback_groups, "posix").expect("resolve_system_tiers");
        assert!(
            !table.is_single_tier(),
            "two-tier plan must not be single-tier"
        );
        // highest-priority-first: high (80) = idx 0, low (10) = idx 1.
        assert_eq!(table.tiers[0].name, "high");
        assert_eq!(table.tiers[1].name, "low");
        assert!(
            table.tiers[0]
                .members
                .contains(&("ctrl_node".to_string(), "ctrl".to_string())),
            "ctrl_node/ctrl must be in high tier"
        );
        assert!(
            table.tiers[1]
                .members
                .contains(&("telem_node".to_string(), "telem".to_string())),
            "telem_node/telem must be in low tier"
        );
    }
    /// Phase 273 W4 (RFC-0047) — sub-node: ONE component with TWO groups on TWO
    /// tiers must resolve without error (NodeSpansTiers v1 constraint lifted).
    #[test]
    fn resolve_system_tiers_sub_node_two_groups_two_tiers() {
        let system_toml_str = r#"
[system]
name = "test_subnode"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "subnode_pkg"
class = "subnode_pkg::SubNode"
name = "sub_node"
group_tiers = { ctrl = "high", telem = "low" }

[tiers.high]
spin_period_us = 10000
[tiers.high.posix]
priority = 80

[tiers.low]
spin_period_us = 100000
[tiers.low.posix]
priority = 10
"#;
        let system: SystemToml = toml::from_str(system_toml_str).expect("parse system.toml");
        let cfg = NrosConfig::default();
        let callback_groups = collect_callback_groups(&cfg, &system.components);
        let table = resolve_system_tiers(&system, &callback_groups, "posix")
            .expect("sub-node must resolve");
        assert!(!table.is_single_tier(), "must be multi-tier");
        let high = table.tiers.iter().find(|t| t.name == "high").unwrap();
        let low = table.tiers.iter().find(|t| t.name == "low").unwrap();
        // Same node, both groups resolved to different tiers (the RFC-0047 sub-node proof).
        assert!(
            high.members
                .contains(&("sub_node".to_string(), "ctrl".to_string())),
            "sub_node/ctrl must be in high tier"
        );
        assert!(
            low.members
                .contains(&("sub_node".to_string(), "telem".to_string())),
            "sub_node/telem must be in low tier"
        );
    }

    /// phase-459 W1 (issue 1426) - the cmake keyword as the third source.
    mod cmake_metadata {
        use super::*;

        /// One `nros-metadata.json`, in the shape `_nros_metadata_emit()`
        /// writes it. The island's own file is byte-compatible with this; only
        /// its `callback_groups` arrays were empty, because it never wrote the
        /// keyword.
        fn doc(rows: &[(&str, &[&str])]) -> String {
            let comps: Vec<String> = rows
                .iter()
                .map(|(name, groups)| {
                    let gs: Vec<String> = groups.iter().map(|g| format!("\"{g}\"")).collect();
                    format!(
                        "    {{\"name\": \"{name}\", \"pkg\": \"{name}_pkg\", \
                         \"class\": \"{name}_pkg::C\", \"class_header\": \"h.hpp\", \
                         \"shape\": \"rclcpp\", \"sources\": [], \"deploy\": [], \
                         \"pkg_dir\": \"/nowhere\", \"lang\": \"cpp\", \
                         \"callback_groups\": [{}]}}",
                        gs.join(", ")
                    )
                })
                .collect();
            format!(
                "{{\n  \"components\": [\n{}\n  ],\n  \"applications\": [\n  ]\n}}\n",
                comps.join(",\n")
            )
        }

        fn write(path: &Path, body: &str) {
            std::fs::create_dir_all(path.parent().expect("has a parent")).expect("mkdir");
            std::fs::write(path, body).expect("write");
        }

        fn component(pkg: &str, name: &str) -> SystemComponentEntry {
            SystemComponentEntry {
                pkg: pkg.to_string(),
                class: format!("{pkg}::C"),
                name: name.to_string(),
                group_tiers: BTreeMap::new(),
                params: Default::default(),
                params_files: Vec::new(),
                entities: None,
                dispatch: None,
            }
        }

        fn cfg_at(root: &Path) -> NrosConfig {
            NrosConfig {
                workspace_root: root.to_path_buf(),
                ..NrosConfig::default()
            }
        }

        /// `cmake -S . -B build` at the workspace root: the file is one level
        /// down and the bake's `--workspace` is the root itself.
        #[test]
        fn a_build_tree_at_the_workspace_root_is_read() {
            let tmp = tempfile::tempdir().expect("tempdir");
            write(
                &tmp.path().join("build/nros-metadata.json"),
                &doc(&[("ctrl_node", &["main"])]),
            );
            let groups = metadata_callback_groups(tmp.path());
            assert_eq!(
                groups.get("ctrl_node").map(Vec::as_slice),
                Some(&["main".to_string()][..])
            );
        }

        /// colcon's per-package build tree, `build/<pkg>/`.
        #[test]
        fn a_per_package_build_tree_is_read() {
            let tmp = tempfile::tempdir().expect("tempdir");
            write(
                &tmp.path().join("build/ctrl_pkg/nros-metadata.json"),
                &doc(&[("ctrl_node", &["main"])]),
            );
            assert!(metadata_callback_groups(tmp.path()).contains_key("ctrl_node"));
        }

        /// The Zephyr road: `nros_system_generate()` passes `--workspace` the
        /// PARENT of the bringup package, so the bake's workspace is `<ws>/src`
        /// while `CMAKE_BINARY_DIR` is `<ws>/build-board`. This is the island's
        /// exact layout, and without the one-level walk-up the keyword it
        /// authors would still reach nothing.
        #[test]
        fn the_parent_build_tree_is_read_when_the_parent_is_a_workspace() {
            let tmp = tempfile::tempdir().expect("tempdir");
            std::fs::write(tmp.path().join(".colcon_workspace"), "").expect("marker");
            std::fs::create_dir_all(tmp.path().join("src")).expect("mkdir src");
            write(
                &tmp.path().join("build-board/nros-metadata.json"),
                &doc(&[("mrm_handler", &["main"])]),
            );
            assert!(
                metadata_callback_groups(&tmp.path().join("src")).contains_key("mrm_handler"),
                "the bake's workspace is <ws>/src; its build tree is one level up"
            );
        }

        /// The walk-up stops at a directory that carries no workspace marker,
        /// so a workspace in a home directory never reads its neighbours' build
        /// trees.
        #[test]
        fn the_walk_up_stops_at_a_directory_that_is_not_a_workspace() {
            let tmp = tempfile::tempdir().expect("tempdir");
            std::fs::create_dir_all(tmp.path().join("ws")).expect("mkdir ws");
            write(
                &tmp.path().join("build/nros-metadata.json"),
                &doc(&[("ctrl_node", &["main"])]),
            );
            assert!(
                metadata_callback_groups(&tmp.path().join("ws")).is_empty(),
                "the parent carries no workspace marker, so it is not a root"
            );
        }

        /// An empty array is "this configure said nothing", not "no groups" -
        /// the island's four rows read exactly like this before it wrote the
        /// keyword. A second tree that DOES name the group must still win.
        #[test]
        fn an_empty_array_is_not_a_statement_and_several_trees_union() {
            let tmp = tempfile::tempdir().expect("tempdir");
            write(
                &tmp.path().join("build-stale/nros-metadata.json"),
                &doc(&[("ctrl_node", &[])]),
            );
            write(
                &tmp.path().join("build-board/nros-metadata.json"),
                &doc(&[("ctrl_node", &["main"])]),
            );
            assert_eq!(
                metadata_callback_groups(tmp.path()).get("ctrl_node"),
                Some(&vec!["main".to_string()])
            );
        }

        /// An unreadable or schema-drifted file is skipped, not fatal: this is
        /// an enrichment of a bake that must keep working for every workspace
        /// that has no such file at all.
        #[test]
        fn a_garbage_file_is_skipped() {
            let tmp = tempfile::tempdir().expect("tempdir");
            write(&tmp.path().join("build/nros-metadata.json"), "{not json");
            write(
                &tmp.path().join("build-2/nros-metadata.json"),
                &doc(&[("ctrl_node", &["main"])]),
            );
            assert!(metadata_callback_groups(tmp.path()).contains_key("ctrl_node"));
        }

        /// The whole point: a C/C++ component, no `group_tiers`, no cargo
        /// package entry - the two sources that existed - and the keyword still
        /// reaches `collect_callback_groups`, bound to `DEFAULT_TIER`, which is
        /// the input shape `derive_tiers_from_contracts` keys on.
        #[test]
        fn a_cmake_component_with_no_binding_gets_its_keyword_on_the_default_tier() {
            let tmp = tempfile::tempdir().expect("tempdir");
            write(
                &tmp.path().join("build/nros-metadata.json"),
                &doc(&[("ctrl_node", &["main"])]),
            );
            let map =
                collect_callback_groups(&cfg_at(tmp.path()), &[component("ctrl_pkg", "ctrl_node")]);
            let decls = map
                .get("ctrl_node")
                .expect("the keyword must reach the bake");
            assert_eq!(decls.len(), 1);
            assert_eq!(decls[0].id, "main");
            assert_eq!(
                decls[0].tier, DEFAULT_TIER,
                "the keyword says WHICH groups exist, never where they run"
            );
        }

        /// The new rung is the LAST one: an authored `group_tiers` still
        /// decides, and the metadata does not add a second entry beside it.
        #[test]
        fn an_authored_binding_still_outranks_the_keyword() {
            let tmp = tempfile::tempdir().expect("tempdir");
            write(
                &tmp.path().join("build/nros-metadata.json"),
                &doc(&[("ctrl_node", &["main", "telem"])]),
            );
            let mut c = component("ctrl_pkg", "ctrl_node");
            c.group_tiers.insert("main".to_string(), "high".to_string());
            let map = collect_callback_groups(&cfg_at(tmp.path()), &[c]);
            let decls = map.get("ctrl_node").expect("bound");
            assert_eq!(decls.len(), 1, "only the authored binding: {decls:?}");
            assert_eq!(decls[0].tier, "high");
        }

        /// A component the metadata does not name is untouched - it stays
        /// groupless and `derive_tiers_from_contracts` keeps it on the default
        /// tier with the note issue 1371 persists.
        #[test]
        fn a_component_the_metadata_does_not_name_stays_groupless() {
            let tmp = tempfile::tempdir().expect("tempdir");
            write(
                &tmp.path().join("build/nros-metadata.json"),
                &doc(&[("ctrl_node", &["main"])]),
            );
            let map = collect_callback_groups(
                &cfg_at(tmp.path()),
                &[component("telem_pkg", "telem_node")],
            );
            assert!(map.is_empty(), "{map:?}");
        }
    }
}
