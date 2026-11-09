//! Phase-535 W2 (RFC-0122 D4) — which workspace IMAGE is a pair cell's role.
//!
//! RFC-0122 D1 moves the example pairs (talker/listener, service
//! server/client, action server/client) into the language workspaces as image
//! rows: one bringup per behaviour (`{pubsub,service,action}_bringup`) in
//! `examples/workspaces/{rust,c,cpp}`, one `[image.<id>]` per (platform, role,
//! rmw). The cells keep `Kind::Example` (D4 — flipping them to `Workspace`
//! would reroute every service and action cell to `roundtrip_xprocess_e2e`
//! through `matrix::consumer_of`), so the test files and their rstest case
//! names do not move. What moves is the answer to "which binary is this
//! cell's listener", and this module is the ONLY place that knows it.
//!
//! # The image id
//!
//! `<platform>_<role>[_<variant>][_<rmw>][_<config>…]`, every part a fixed token:
//!
//! - `<platform>` — [`image_prefix`], the entry vocabulary
//!   `examples/workspaces/README-layout.md` names (`freertos`, `nuttx`,
//!   `threadx`, `zephyr`, …). Where a workspace already spells a platform
//!   (`threadx` for threadx-linux, `riscv_threadx`, `riscv_nuttx`,
//!   `freertos_posix`) this reuses it, so a bringup's ids read the same as
//!   `demo_bringup`'s.
//! - `<role>` — [`PairRole::token`]; the two roles of one workload never
//!   share a token, so the role alone names the workload.
//! - `<variant>` — a [`Variant`]: different node CODE for the same role
//!   (RFC-0122 D2's "variant" row, phase-535 W6), e.g. the callback-receive
//!   service client. A variant is NOT a cell axis: its image is modelled by the
//!   same coordinate's Example cell as the plain role, so a test asks for it by
//!   name ([`resolve_coord_variant`]) and the cell lookup ignores it.
//! - `<rmw>` — omitted for zenoh (README-layout's "zenoh takes no suffix"),
//!   else `xrce` / `cyclonedds`.
//! - `<config>` — a configured cell's `Config` set (RFC-0051 §7a), in the
//!   cell's order, `-` spelled `_`.
//!
//! Example: `(ZephyrNativeSim, Rust, Xrce, Pubsub)`'s listener is
//! `examples/workspaces/rust` · `src/pubsub_bringup` · `zephyr_listener_xrce`,
//! which is RFC-0122 D4's own example.
//!
//! The mapping is total over the pair cells and injective; both are unit
//! tests over `matrix::CELLS`, as is the inverse [`parse_image`], which is what
//! lets a gate that only sees a manifest row (`check-image-locator-bake`'s
//! "a manifest row carries no workload") recover the cell.

use crate::{
    TestError, TestResult,
    matrix::{Cell, Config, Kind, Lang, PlatformId, Rmw, Workload},
};
use std::path::PathBuf;

/// One process of a pair. Each workload has two, and the two of one workload
/// spell different tokens.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PairRole {
    Talker,
    Listener,
    ServiceServer,
    ServiceClient,
    ActionServer,
    ActionClient,
}

impl PairRole {
    pub const ALL: [PairRole; 6] = [
        PairRole::Talker,
        PairRole::Listener,
        PairRole::ServiceServer,
        PairRole::ServiceClient,
        PairRole::ActionServer,
        PairRole::ActionClient,
    ];

    /// The id token. Matches the demo bringups' existing `linux_service_server`
    /// / `linux_action_client` spelling.
    pub const fn token(self) -> &'static str {
        match self {
            PairRole::Talker => "talker",
            PairRole::Listener => "listener",
            PairRole::ServiceServer => "service_server",
            PairRole::ServiceClient => "service_client",
            PairRole::ActionServer => "action_server",
            PairRole::ActionClient => "action_client",
        }
    }

    /// The workload this role is half of.
    pub const fn workload(self) -> Workload {
        match self {
            PairRole::Talker | PairRole::Listener => Workload::Pubsub,
            PairRole::ServiceServer | PairRole::ServiceClient => Workload::Service,
            PairRole::ActionServer | PairRole::ActionClient => Workload::Action,
        }
    }

    /// The two roles of a pair workload, or `None` for any other workload.
    pub const fn of(workload: Workload) -> Option<[PairRole; 2]> {
        match workload {
            Workload::Pubsub => Some([PairRole::Talker, PairRole::Listener]),
            Workload::Service => Some([PairRole::ServiceServer, PairRole::ServiceClient]),
            Workload::Action => Some([PairRole::ActionServer, PairRole::ActionClient]),
            _ => None,
        }
    }
}

/// Different node code for one role (RFC-0122 D2 "variant", phase-535 W6):
/// its own node package in the language workspace, and an image of the
/// role's behaviour bringup whose id carries the token after the role.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Variant {
    /// The reply (or goal response, feedback and result) arrives through a
    /// callback dispatched at spin, not a poll (RFC-0041). Client roles only.
    Callback,
}

impl Variant {
    pub const ALL: [Variant; 1] = [Variant::Callback];

    /// The id token.
    pub const fn token(self) -> &'static str {
        match self {
            Variant::Callback => "callback",
        }
    }
}

/// The bringup (relative to its workspace) that holds a workload's images.
pub const fn bringup_of(workload: Workload) -> Option<&'static str> {
    match workload {
        Workload::Pubsub => Some("src/pubsub_bringup"),
        Workload::Service => Some("src/service_bringup"),
        Workload::Action => Some("src/action_bringup"),
        _ => None,
    }
}

/// The platform part of an image id. `None` for a platform with no image road
/// (PX4: its cells are uORB/XRCE carve-outs built by the SITL job).
pub const fn image_prefix(platform: PlatformId) -> Option<&'static str> {
    match platform {
        PlatformId::Linux => Some("linux"),
        PlatformId::ZephyrNativeSim => Some("zephyr"),
        PlatformId::ZephyrQemuCortexM => Some("zephyr_cortex_m"),
        PlatformId::ZephyrNucleoF767zi => Some("zephyr_nucleo_f767zi"),
        PlatformId::FreertosMps2 => Some("freertos"),
        PlatformId::FreertosPosix => Some("freertos_posix"),
        PlatformId::NuttxArm => Some("nuttx"),
        PlatformId::NuttxRiscv => Some("riscv_nuttx"),
        PlatformId::ThreadxLinux => Some("threadx"),
        PlatformId::ThreadxRiscv64 => Some("riscv_threadx"),
        PlatformId::QemuBaremetal => Some("baremetal"),
        PlatformId::Fvp => Some("fvp"),
        PlatformId::Px4 => None,
    }
}

const fn rmw_suffix(rmw: Rmw) -> Option<&'static str> {
    match rmw {
        Rmw::Zenoh => Some(""),
        Rmw::Xrce => Some("_xrce"),
        Rmw::Cyclonedds => Some("_cyclonedds"),
        Rmw::Uorb => None,
    }
}

const fn workspace_of(lang: Lang) -> Option<&'static str> {
    match lang {
        Lang::Rust => Some("examples/workspaces/rust"),
        Lang::C => Some("examples/workspaces/c"),
        Lang::Cpp => Some("examples/workspaces/cpp"),
        // `mixed` keeps its own packages (RFC-0122 D1) and has no pair cells.
        Lang::Mixed => None,
    }
}

/// A pair cell is an `Example` cell whose workload has two roles.
pub fn is_pair_cell(cell: &Cell) -> bool {
    cell.kind == Kind::Example && PairRole::of(cell.workload).is_some()
}

/// Where one role of a pair cell lives.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WorkspaceImage {
    /// Repo-relative workspace root (`examples/workspaces/<lang>`), the
    /// manifest row's `dir`.
    pub workspace: &'static str,
    /// Bringup relative to the workspace, the row's `bringup`.
    pub bringup: &'static str,
    /// The `[image.<id>]` id, the row's `image`. Its generated binary is
    /// `<image>_entry`.
    pub image: String,
}

impl WorkspaceImage {
    /// The generated entry's binary / target name.
    pub fn entry(&self) -> String {
        format!("{}_entry", self.image)
    }

    /// `<bringup pkg>:<image>`, the qualified spelling
    /// `scripts/build/workspace-fixtures-build.sh` and the west lane pass to
    /// `nros build`.
    pub fn qualified(&self) -> String {
        let pkg = self.bringup.rsplit('/').next().unwrap_or(self.bringup);
        format!("{pkg}:{}", self.image)
    }
}

/// (cell, role) → the workspace image that runs that role. `Err` says which
/// half of the question had no answer.
pub fn workspace_image(cell: &Cell, role: PairRole) -> Result<WorkspaceImage, String> {
    workspace_image_variant(cell, role, None)
}

/// [`workspace_image`] for one [`Variant`] of the role (`None` = the plain role).
pub fn workspace_image_variant(
    cell: &Cell,
    role: PairRole,
    variant: Option<Variant>,
) -> Result<WorkspaceImage, String> {
    if cell.kind != Kind::Example {
        return Err(format!("{cell:?} is not an Example cell"));
    }
    if role.workload() != cell.workload {
        return Err(format!(
            "role {role:?} is half of {:?}, not of the cell's {:?}",
            role.workload(),
            cell.workload
        ));
    }
    let bringup =
        bringup_of(cell.workload).ok_or_else(|| format!("{:?} has no pair", cell.workload))?;
    let workspace = workspace_of(cell.lang)
        .ok_or_else(|| format!("{:?} has no language workspace", cell.lang))?;
    let prefix = image_prefix(cell.platform)
        .ok_or_else(|| format!("{:?} has no image road", cell.platform))?;
    let rmw = rmw_suffix(cell.rmw).ok_or_else(|| format!("{:?} has no image road", cell.rmw))?;
    let variant = variant
        .map(|v| format!("_{}", v.token()))
        .unwrap_or_default();
    let mut image = format!("{prefix}_{}{variant}{rmw}", role.token());
    for c in cell.config {
        image.push('_');
        image.push_str(&c.as_str().replace('-', "_"));
    }
    Ok(WorkspaceImage {
        workspace,
        bringup,
        image,
    })
}

/// What [`parse_image`] recovers from a row: everything a [`Cell`] keys on
/// except its tier, plus the role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedImage {
    pub platform: PlatformId,
    pub lang: Lang,
    pub rmw: Rmw,
    pub role: PairRole,
    pub variant: Option<Variant>,
    pub config: Vec<Config>,
}

const CONFIGS: [Config; 5] = [
    Config::Lending,
    Config::Multisession,
    Config::SignalFdWake,
    Config::CustomTransport,
    Config::Serial,
];

/// Whether an image of this configuration dials a router PORT. A `Serial`
/// image's locator names a UART, and its router (or Agent) is the far end of
/// a PTY pair the test creates, so there is no `alloc::port_of` to bake.
fn dials_router_port(config: &[Config]) -> bool {
    !config.contains(&Config::Serial)
}

/// Phase-531 W6 (RFC-0121 D8, RFC-0122 amendment) — whether a workspace at
/// `dir` is a single-node WITNESS migrated in place: `examples/<plat>/<lang>/<name>`,
/// with `<plat>` neither `workspaces` nor `templates` and `<lang>` one of
/// `rust`, `c` or `cpp`.
///
/// W6 turns `examples/<plat>/<lang>/talker` (and `custom-platform`) into a
/// one-node workspace where it stands, so its fixture rows become
/// `[[workspace_fixture]]` rows. Its program and its cell do not change: it is
/// still the `Kind::Example` cell's image, exactly as a pair image in a language
/// workspace is (see [`workspace_row_models_example_cell`]). The test keys on
/// the DIRECTORY because that is the one fact every witness row states. A west
/// row's `west_role` would also work, but a linux row has none.
///
/// A test-tree bin migrated the same way (`packages/testing/nros-tests/bins/
/// <name>`, e.g. the mps2 idle-wake witness in batch 8) is one too: its rows
/// were `[[fixture]]` rows of the `Kind::Example` cell, and moving them to a
/// bringup image changes the road, not the cell.
pub fn is_witness_workspace(dir: &str) -> bool {
    let parts: Vec<&str> = dir.trim_end_matches('/').split('/').collect();
    matches!(
        parts.as_slice(),
        ["examples", plat, "rust" | "c" | "cpp", name]
            if !matches!(*plat, "workspaces" | "templates") && !name.is_empty()
    ) || matches!(
        parts.as_slice(),
        ["packages", "testing", "nros-tests", "bins", name] if !name.is_empty()
    )
}

/// Whether a `[[workspace_fixture]]` row models its coordinate's `Kind::Example`
/// cell rather than a `Kind::Workspace` one. There are two such shapes: a pair
/// image of a language workspace ([`parse_image`], phase-535 W4), and a witness
/// migrated in place ([`is_witness_workspace`], phase-531 W6). Every other
/// workspace row (`demo_bringup`, a feature image) is a Workspace cell's.
pub fn workspace_row_models_example_cell(dir: &str, bringup: &str, image: &str) -> bool {
    is_witness_workspace(dir) || parse_image(dir, bringup, image).is_some()
}

/// The inverse of [`workspace_image`]: a manifest row's (dir, bringup, image)
/// back to the cell coordinate and role, or `None` when the row is not a pair
/// image (`demo_bringup`'s rows, a feature image, …). Exactly one parse may
/// succeed — the prefix table is checked for that by
/// `every_pair_image_parses_back_to_its_cell`.
pub fn parse_image(workspace: &str, bringup: &str, image: &str) -> Option<ParsedImage> {
    let workspace = workspace.trim_end_matches('/');
    let lang = [Lang::Rust, Lang::C, Lang::Cpp]
        .into_iter()
        .find(|l| workspace_of(*l) == Some(workspace))?;
    let bringup = bringup.trim_end_matches('/');
    let roles: Vec<PairRole> = PairRole::ALL
        .into_iter()
        .filter(|r| bringup_of(r.workload()) == Some(bringup))
        .collect();
    let mut hits = Vec::new();
    for platform in PlatformId::ALL.iter().copied() {
        let Some(prefix) = image_prefix(platform) else {
            continue;
        };
        let Some(rest) = image.strip_prefix(prefix).and_then(|r| r.strip_prefix('_')) else {
            continue;
        };
        for &role in &roles {
            let Some(rest) = rest.strip_prefix(role.token()) else {
                continue;
            };
            let variants =
                std::iter::once((None, rest)).chain(Variant::ALL.into_iter().filter_map(|v| {
                    rest.strip_prefix('_')
                        .and_then(|r| r.strip_prefix(v.token()))
                        .map(|r| (Some(v), r))
                }));
            for (variant, rest) in variants {
                for rmw in [Rmw::Zenoh, Rmw::Xrce, Rmw::Cyclonedds] {
                    let suffix = rmw_suffix(rmw).unwrap_or_default();
                    let Some(rest) = rest.strip_prefix(suffix) else {
                        continue;
                    };
                    // A zenoh image's empty suffix leaves `_xrce` in `rest`, which
                    // `parse_configs` refuses, so an xrce id parses only as xrce.
                    if let Some(config) = parse_configs(rest) {
                        hits.push(ParsedImage {
                            platform,
                            lang,
                            rmw,
                            role,
                            variant,
                            config,
                        });
                    }
                }
            }
        }
    }
    match hits.len() {
        1 => hits.pop(),
        _ => None,
    }
}

fn parse_configs(mut rest: &str) -> Option<Vec<Config>> {
    let mut out = Vec::new();
    while !rest.is_empty() {
        rest = rest.strip_prefix('_')?;
        let c = CONFIGS
            .into_iter()
            .find(|c| rest.starts_with(&c.as_str().replace('-', "_")))?;
        rest = &rest[c.as_str().len()..];
        out.push(c);
    }
    Some(out)
}

/// The `[[workspace_fixture]]` row id that builds `img`. Exactly one row may.
pub fn row_id(img: &WorkspaceImage) -> TestResult<String> {
    crate::fixtures::workspace_fixture_id_for_image(img.workspace, img.bringup, &img.image)
}

/// The prebuilt binary for one role of a pair cell, through the workspace
/// resolvers (`build_workspace_rust_entry` / `build_workspace_cmake_entry`
/// and their stamp, lane and staleness checks). A zenoh image on a platform
/// whose isolation is BAKED also has its locator checked against the cell's
/// `alloc::port_of` (issue 1581), so a row that dials the wrong router fails
/// here by name rather than as `ConnectionFailed` at boot.
///
/// Zephyr native_sim images are built by the west lane, into the build dir
/// their row names (`west_build_name`); see `resolve_zephyr`. The other
/// west boards have no pair images yet.
pub fn resolve(cell: &Cell, role: PairRole) -> TestResult<PathBuf> {
    resolve_variant(cell, role, None)
}

/// [`resolve`] for one [`Variant`] of the role.
pub fn resolve_variant(
    cell: &Cell,
    role: PairRole,
    variant: Option<Variant>,
) -> TestResult<PathBuf> {
    let img = workspace_image_variant(cell, role, variant).map_err(TestError::BuildFailed)?;
    if cell.platform == PlatformId::ZephyrNativeSim {
        return resolve_zephyr(cell, &img);
    }
    if matches!(
        cell.platform,
        PlatformId::ZephyrQemuCortexM | PlatformId::ZephyrNucleoF767zi | PlatformId::Fvp
    ) {
        return Err(TestError::BuildFailed(format!(
            "{} is a west image; zephyr cells resolve through the zephyr build-dir table",
            img.qualified()
        )));
    }
    let id = row_id(&img)?;
    let lang_dir = img.workspace.rsplit('/').next().unwrap_or(img.workspace);
    let binary = match cell.lang {
        Lang::Rust => match crate::fixtures::baked_locator::row_str(&id, "target") {
            Some(triple) => {
                crate::fixtures::build_workspace_rust_cross_entry(&id, &triple, &img.entry())?
            }
            None => crate::fixtures::build_workspace_rust_entry(&id, lang_dir, &img.entry())?,
        },
        Lang::C | Lang::Cpp => {
            crate::fixtures::build_workspace_cmake_entry(&id, lang_dir, &img.entry())?
        }
        Lang::Mixed => unreachable!("workspace_image refuses Mixed"),
    };
    if cell.rmw == Rmw::Zenoh
        && cell.platform != PlatformId::Linux
        && dials_router_port(cell.config)
    {
        crate::fixtures::baked_locator::assert_row_dials_port(&id, crate::alloc::port(cell));
    }
    Ok(binary)
}

/// A zephyr native_sim pair image: the west lane builds row `id` into
/// `<zephyr build root>/<west_build_name>/`, so the name is read from the row,
/// never spelled here. The staleness probe watches the whole language
/// workspace, like every other generated west application
/// (`build_zephyr_workspace_c_entry`).
fn resolve_zephyr(cell: &Cell, img: &WorkspaceImage) -> TestResult<PathBuf> {
    let id = row_id(img)?;
    let build =
        crate::fixtures::baked_locator::row_str(&id, "west_build_name").ok_or_else(|| {
            TestError::BuildFailed(format!(
                "row {id} ({}) states no `west_build_name`",
                img.qualified()
            ))
        })?;
    let exe = crate::zephyr::zephyr_build_root()
        .join(&build)
        .join("zephyr")
        .join("zephyr.exe");
    let rmw = match cell.rmw {
        Rmw::Zenoh => "zenoh",
        Rmw::Xrce => "xrce",
        Rmw::Cyclonedds => "cyclonedds",
        Rmw::Uorb => {
            return Err(TestError::BuildFailed(format!(
                "{} is a uORB cell; zephyr has no uORB images",
                img.qualified()
            )));
        }
    };
    crate::fixtures::require_prebuilt_binary_fresh_zephyr(
        &exe,
        crate::fixtures::ZephyrLeafSource {
            dir: img.workspace,
            lang: Some(cell.lang.as_str()),
            rmw: Some(rmw),
            conf_files: None,
        },
    )
}

/// Issue 1581 for pair images: the row's baked locator must dial its cell's
/// `alloc::port_of`. `None` when it does, or when the row is not a baked
/// zenoh pair image (linux is ephemeral; xrce and cyclone bake no locator).
/// `root` is the checkout, so a test can hand it a synthetic one.
pub fn pair_row_locator_error(
    root: &std::path::Path,
    id: &str,
    dir: &str,
    bringup: &str,
    image: &str,
) -> Option<String> {
    let p = parse_image(dir, bringup, image)?;
    if p.rmw != Rmw::Zenoh || p.platform == PlatformId::Linux || !dials_router_port(&p.config) {
        return None;
    }
    let want = crate::alloc::port_of(p.platform, p.lang, p.role.workload());
    use crate::fixtures::baked_locator::{baked_locator_in, locator_port};
    match baked_locator_in(root, id) {
        Ok(loc) if locator_port(&loc) == Some(want) => None,
        Ok(loc) => Some(format!(
            "{id} ({image}): bakes `{loc}`, its cell's router is alloc::port_of = {want}"
        )),
        Err(e) => Some(format!("{id} ({image}): {e}")),
    }
}

/// [`resolve`] for the plain (unconfigured) Example cell at this coordinate.
pub fn resolve_coord(
    platform: PlatformId,
    lang: Lang,
    rmw: Rmw,
    role: PairRole,
) -> TestResult<PathBuf> {
    resolve_coord_variant(platform, lang, rmw, role, None)
}

/// [`resolve_coord`] for one [`Variant`] of the role. The variant image is the
/// same coordinate's Example cell's, so the cell is looked up exactly as for
/// the plain role.
pub fn resolve_coord_variant(
    platform: PlatformId,
    lang: Lang,
    rmw: Rmw,
    role: PairRole,
    variant: Option<Variant>,
) -> TestResult<PathBuf> {
    resolve_coord_in(platform, lang, rmw, role, variant, &[])
}

/// [`resolve`] for the Example cell at this coordinate under exactly the
/// configuration set `config` (RFC-0051 §7a); `&[]` is [`resolve_coord`].
pub fn resolve_configured(
    platform: PlatformId,
    lang: Lang,
    rmw: Rmw,
    role: PairRole,
    config: &[Config],
) -> TestResult<PathBuf> {
    resolve_coord_in(platform, lang, rmw, role, None, config)
}

fn resolve_coord_in(
    platform: PlatformId,
    lang: Lang,
    rmw: Rmw,
    role: PairRole,
    variant: Option<Variant>,
    config: &[Config],
) -> TestResult<PathBuf> {
    let cell = crate::matrix::CELLS
        .iter()
        .find(|c| {
            is_pair_cell(c)
                && c.platform == platform
                && c.lang == lang
                && c.rmw == rmw
                && c.workload == role.workload()
                && c.config == config
        })
        .ok_or_else(|| {
            TestError::BuildFailed(format!(
                "no ({platform:?}, {lang:?}, {rmw:?}, {:?}) + {config:?} Example cell in matrix::CELLS",
                role.workload()
            ))
        })?;
    resolve_variant(cell, role, variant)
}

/// [`resolve_coord`], cached for the process: the `BinaryPair` tables hold
/// `fn() -> TestResult<&'static Path>`, so a resolved image lives as long as
/// the test binary. One cache for every platform, keyed on the coordinate.
pub fn resolve_coord_cached(
    platform: PlatformId,
    lang: Lang,
    rmw: Rmw,
    role: PairRole,
) -> TestResult<&'static std::path::Path> {
    use std::{collections::HashMap, path::Path, sync::Mutex};
    static CACHE: Mutex<Option<HashMap<String, &'static Path>>> = Mutex::new(None);
    let key = format!("{platform:?}/{lang:?}/{rmw:?}/{}", role.token());
    if let Some(p) = CACHE
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .get(&key)
    {
        return Ok(p);
    }
    let path = resolve_coord(platform, lang, rmw, role)?;
    let leaked: &'static Path = Box::leak(path.into_boxed_path());
    CACHE
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(key, leaked);
    Ok(leaked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matrix::CELLS;
    use std::collections::HashMap;

    fn pair_cells() -> impl Iterator<Item = &'static Cell> {
        CELLS.iter().filter(|c| is_pair_cell(c))
    }

    /// A witness migrated in place is recognised by its directory alone, and
    /// nothing that is a language workspace, a template or a deeper package is.
    #[test]
    fn a_witness_workspace_is_recognised_by_its_directory() {
        for dir in [
            "examples/zephyr/c/talker",
            "examples/zephyr/rust/talker/",
            "examples/linux/cpp/talker",
            "examples/linux/c/custom-platform",
            "examples/zephyr-nucleo-f767zi/c/talker",
            "packages/testing/nros-tests/bins/idle-wake-zephyr-mps2",
        ] {
            assert!(is_witness_workspace(dir), "{dir} should be a witness");
            assert!(workspace_row_models_example_cell(
                dir,
                "src/x_bringup",
                "zephyr"
            ));
        }
        for dir in [
            "examples/workspaces/rust",
            "examples/workspaces/c/talker",
            "examples/templates/rust/talker",
            "examples/zephyr/python/talker",
            "examples/zephyr/c",
            "examples/zephyr/c/talker/src/talker_pkg",
            "packages/testing/zephyr/c/talker",
            "packages/testing/nros-tests/fixtures/image_config_ws",
            "packages/testing/nros-tests/bins",
            "examples/workspaces/features",
        ] {
            assert!(!is_witness_workspace(dir), "{dir} should not be a witness");
        }
        // A demo_bringup row stays a Workspace cell's; a pair image an Example's.
        assert!(!workspace_row_models_example_cell(
            "examples/workspaces/rust",
            "src/demo_bringup",
            "zephyr"
        ));
        assert!(workspace_row_models_example_cell(
            "examples/workspaces/rust",
            "src/pubsub_bringup",
            "zephyr_listener_xrce"
        ));
    }

    /// Every role of every pair cell has an image, unless the cell's platform
    /// or RMW has no image road at all (PX4 / uORB), and nothing else.
    #[test]
    fn every_pair_cell_role_maps_to_an_image() {
        let mut bad = Vec::new();
        let mut n = 0;
        for cell in pair_cells() {
            for role in PairRole::of(cell.workload).unwrap() {
                match workspace_image(cell, role) {
                    Ok(_) => n += 1,
                    Err(_) if cell.platform == PlatformId::Px4 || cell.rmw == Rmw::Uorb => {}
                    Err(e) => bad.push(format!("{cell:?} {role:?}: {e}")),
                }
            }
        }
        assert!(
            bad.is_empty(),
            "unmapped pair roles:\n  {}",
            bad.join("\n  ")
        );
        // Not vacuous: the matrix holds ~100 pair cells today.
        assert!(n > 100, "only {n} pair roles mapped");
    }

    /// No two (cell, role) share an image — two cells built from one image
    /// would each claim the other's isolation.
    #[test]
    fn pair_images_are_injective() {
        let mut seen: HashMap<WorkspaceImage, String> = HashMap::new();
        let mut bad = Vec::new();
        for cell in pair_cells() {
            for role in PairRole::of(cell.workload).unwrap() {
                let Ok(img) = workspace_image(cell, role) else {
                    continue;
                };
                let me = format!("{cell:?} {role:?}");
                if let Some(other) = seen.insert(img.clone(), me.clone()) {
                    bad.push(format!("{} : {other} and {me}", img.image));
                }
            }
        }
        assert!(bad.is_empty(), "shared images:\n  {}", bad.join("\n  "));
    }

    /// The inverse recovers the cell's coordinate and role, and exactly one
    /// prefix parses (so `zephyr` never claims a `zephyr_cortex_m_*` id).
    #[test]
    fn every_pair_image_parses_back_to_its_cell() {
        let mut bad = Vec::new();
        for cell in pair_cells() {
            for role in PairRole::of(cell.workload).unwrap() {
                let Ok(img) = workspace_image(cell, role) else {
                    continue;
                };
                let want = ParsedImage {
                    platform: cell.platform,
                    lang: cell.lang,
                    rmw: cell.rmw,
                    role,
                    variant: None,
                    config: cell.config.to_vec(),
                };
                let got = parse_image(img.workspace, img.bringup, &img.image);
                if got.as_ref() != Some(&want) {
                    bad.push(format!("{}: {got:?} != {want:?}", img.image));
                }
            }
        }
        assert!(bad.is_empty(), "inverse disagrees:\n  {}", bad.join("\n  "));
    }

    /// A variant id parses back to its role and variant through the SAME
    /// parser, for every rmw and config spelling, and never collides with a
    /// plain role (phase-535 W6: one parser, no second spelling).
    #[test]
    fn variant_images_parse_back() {
        let mut bad = Vec::new();
        let mut n = 0;
        for cell in pair_cells() {
            for role in PairRole::of(cell.workload).unwrap() {
                for v in Variant::ALL {
                    let Ok(img) = workspace_image_variant(cell, role, Some(v)) else {
                        continue;
                    };
                    n += 1;
                    let got = parse_image(img.workspace, img.bringup, &img.image);
                    let ok = got.as_ref().is_some_and(|p| {
                        p.platform == cell.platform
                            && p.lang == cell.lang
                            && p.rmw == cell.rmw
                            && p.role == role
                            && p.variant == Some(v)
                            && p.config == cell.config
                    });
                    if !ok {
                        bad.push(format!("{}: {got:?}", img.image));
                    }
                }
            }
        }
        assert!(n > 100, "only {n} variant ids built");
        assert!(bad.is_empty(), "variant ids:\n  {}", bad.join("\n  "));
        let p = parse_image(
            "examples/workspaces/cpp",
            "src/service_bringup",
            "linux_service_client_callback_cyclonedds",
        )
        .expect("the C++ Cyclone callback client");
        assert_eq!(
            (p.role, p.variant, p.rmw),
            (
                PairRole::ServiceClient,
                Some(Variant::Callback),
                Rmw::Cyclonedds
            )
        );
        // An unknown token after the role is still a misspelt id.
        assert!(
            parse_image(
                "examples/workspaces/c",
                "src/service_bringup",
                "linux_service_client_callbak"
            )
            .is_none()
        );
    }

    /// RFC-0122 D4's worked example, and a non-pair row that must not parse.
    #[test]
    fn the_rfc_example_and_a_demo_row() {
        let cell = CELLS
            .iter()
            .find(|c| {
                c.platform == PlatformId::ZephyrNativeSim
                    && c.lang == Lang::Rust
                    && c.rmw == Rmw::Xrce
                    && c.workload == Workload::Pubsub
                    && c.kind == Kind::Example
            })
            .expect("the (zephyr, rust, xrce, pubsub) example cell");
        let img = workspace_image(cell, PairRole::Listener).unwrap();
        assert_eq!(img.workspace, "examples/workspaces/rust");
        assert_eq!(img.qualified(), "pubsub_bringup:zephyr_listener_xrce");
        assert_eq!(img.entry(), "zephyr_listener_xrce_entry");
        assert!(workspace_image(cell, PairRole::ServiceServer).is_err());
        // `demo_bringup`'s rows are not pair images, whatever their id says.
        assert!(
            parse_image(
                "examples/workspaces/c",
                "src/demo_bringup",
                "linux_service_server"
            )
            .is_none()
        );
        assert!(
            parse_image(
                "examples/workspaces/c",
                "src/pubsub_bringup",
                "linux_robot1"
            )
            .is_none()
        );
    }

    /// Every `[[workspace_fixture]]` row that IS a pair image names a cell that
    /// exists in `matrix::CELLS`: an image no cell consumes is a build nobody
    /// tests (the 0350 class).
    #[test]
    fn every_pair_image_row_has_a_cell() {
        let mut bad = Vec::new();
        for (id, dir, bringup, image) in crate::fixtures::workspace_fixture_images() {
            let Some(p) = parse_image(&dir, &bringup, &image) else {
                // A row in a pair bringup of a language workspace that does
                // not parse is a misspelt id (a typo in the role token): no resolver
                // would ever ask for it.
                let pair_bringup = [Workload::Pubsub, Workload::Service, Workload::Action]
                    .into_iter()
                    .any(|w| bringup_of(w) == Some(bringup.as_str()));
                let lang_ws = [Lang::Rust, Lang::C, Lang::Cpp]
                    .into_iter()
                    .any(|l| workspace_of(l) == Some(dir.as_str()));
                if pair_bringup && lang_ws {
                    bad.push(format!(
                        "{id} ({image}): not a `<platform>_<role>[_<rmw>]` id"
                    ));
                }
                continue;
            };
            let found = CELLS.iter().any(|c| {
                is_pair_cell(c)
                    && c.platform == p.platform
                    && c.lang == p.lang
                    && c.rmw == p.rmw
                    && c.workload == p.role.workload()
                    && c.config == p.config.as_slice()
            });
            if !found {
                bad.push(format!("{id} ({image}): {p:?}"));
            }
        }
        assert!(
            bad.is_empty(),
            "pair image rows with no cell:\n  {}",
            bad.join("\n  ")
        );
    }

    /// RFC-0122 W2 (c) — every baked zenoh pair image row dials its cell's
    /// allocator port. `check-image-locator-bake` cannot ask this: a manifest
    /// row carries no workload, and [`parse_image`] is what recovers it.
    #[test]
    fn pair_image_rows_dial_their_allocator_port() {
        let root = crate::project_root();
        let bad: Vec<String> = crate::fixtures::workspace_fixture_images()
            .into_iter()
            .filter_map(|(id, dir, bringup, image)| {
                pair_row_locator_error(&root, &id, &dir, &bringup, &image)
            })
            .collect();
        assert!(bad.is_empty(), "issue 1581:\n  {}", bad.join("\n  "));
    }

    /// Negative control for the test above: a pair image whose bringup states
    /// another cell's port is refused, and the allocator's own port passes.
    #[test]
    fn a_pair_image_dialling_another_port_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let bringup = dir.join("examples/workspaces/c/src/pubsub_bringup");
        std::fs::create_dir_all(&bringup).unwrap();
        std::fs::write(
            dir.join("examples/fixtures.toml"),
            "[[workspace_fixture]]\nid = \"r\"\ndir = \"examples/workspaces/c\"\n\
             bringup = \"src/pubsub_bringup\"\nimage = \"freertos_talker\"\n",
        )
        .unwrap();
        let want = crate::alloc::port_of(PlatformId::FreertosMps2, Lang::C, Workload::Pubsub);
        let check = |port: u16| {
            std::fs::write(
                bringup.join("system.toml"),
                format!(
                    "[image.freertos_talker]\nboard = \"b\"\nlocator = \"tcp/10.0.2.2:{port}\"\n"
                ),
            )
            .unwrap();
            pair_row_locator_error(
                dir,
                "r",
                "examples/workspaces/c",
                "src/pubsub_bringup",
                "freertos_talker",
            )
        };
        assert!(check(want + 1).is_some(), "a wrong port passed");
        assert_eq!(check(want), None);
    }
}
