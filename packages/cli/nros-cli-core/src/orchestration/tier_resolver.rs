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
use std::collections::{BTreeMap, BTreeSet};

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
pub fn collect_callback_groups(
    cfg: &NrosConfig,
    components: &[SystemComponentEntry],
) -> BTreeMap<String, Vec<CallbackGroupDecl>> {
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
        let Some(pkg) = cfg.component_packages.get(&c.pkg) else {
            continue;
        };
        // Single-node pkg → node_or_component; multi-node pkg → match by name/class.
        let groups = pkg
            .nros
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
        }
    }
    map
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
/// deploy block. The Zephyr module's `nros_system_generate()` passes
/// `--target zephyr-<rmw>`, which is the second case for every in-tree bringup
/// it bakes.
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
    let Some((origin, board)) = target_board_id(system, target) else {
        return Ok(nros_entry_lower::BoardFamily::Native.tier_rtos_key());
    };
    let descriptor = super::image::resolve_board_id(catalog, &origin, &board)?;
    Ok(descriptor.platform.tier_rtos_key())
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
        /// so it needs no catalog. `zephyr-zenoh` is what the Zephyr module's
        /// `nros_system_generate()` passes, and it names no block.
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
}
