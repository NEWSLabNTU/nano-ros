//! Stage 4d — the WEST APPLICATION around a generated Zephyr entry
//! (phase-470 W5.a, issue 1288, RFC-0065 D4).
//!
//! ## What was missing, exactly
//!
//! [`super::entry`] has emitted the Rust half of a Zephyr entry since
//! phase-383 W3.b: an `EntryKind::ZephyrStaticlib` renders `#![no_std]`, a
//! `librustapp.a` staticlib and one `nros::main!(launch = …)`. What it could
//! not produce is the **west application** around it — the `CMakeLists.txt`
//! with `find_package(Zephyr)` + `project()` + `rust_cargo_application()`, and
//! the `build.rs` that bridges Kconfig into rustc. So `cmd::build`'s
//! `west_application_dir` could only ever LOCATE a hand-written package, and
//! fifteen of them existed to be located.
//!
//! ## The application directory IS the entry directory
//!
//! Not a choice: zephyr-lang-rust's `rust_cargo_application()` runs its cargo
//! command with `WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}` and passes no
//! `--manifest-path`. So the manifest cargo builds is whatever sits beside the
//! `CMakeLists.txt` west was pointed at, and the two files have to share a
//! directory. That directory is the generated entry's, `build/<coord>/<id>_entry/`.
//!
//! ## Why the manifest needs more here than on any other road
//!
//! Every other driver hands cargo a settings file (`--config`, RFC-0098 D1).
//! This one cannot: the cargo command line belongs to zephyr-lang-rust, and we
//! never see it. Four facts therefore have to reach cargo through the MANIFEST
//! instead, and that is the whole of [`WestApp`]:
//!
//! | fact | why the manifest |
//! | --- | --- |
//! | `zephyr` crate + `zephyr-build` | the board's own boilerplate (`extern crate zephyr;`) and the Kconfig bridge; both resolve through the `--config patch.crates-io.zephyr*.path=` rows zephyr-lang-rust adds |
//! | `nros-zephyr-build` | `bake_nros_config()` — the locator/domain/XRCE bake |
//! | `[features] rmw-<x>` + the backend dep | see below |
//! | `[patch.crates-io]` | the entry is its own cargo root (RFC-0098 D9), so it inherits no `[patch]` from a workspace that no longer has one |
//!
//! ## The RMW feature is not decoration
//!
//! `nros::main!`'s Zephyr arm emits
//!
//! ```ignore
//! #[cfg(feature = "rmw-zenoh")]
//! { let _ = ::nros_rmw_zenoh::register(); }
//! ```
//!
//! because Zephyr has no `BoardEntry` and `.init_array` constructors are
//! compiled out on `target_os = "none"` (issue #129 / RFC-0031 C5b). An entry
//! without that feature and that direct dep compiles cleanly and then fails at
//! run time with `Transport(ConnectionFailed)` — the registry is empty. The
//! selection facade cannot supply it: a `#[cfg(feature = …)]` is evaluated on
//! THIS crate, and `::nros_rmw_zenoh::` has to be a name in THIS crate's scope.
//!
//! Which crate that is comes from `[rmw.provides.cargo] crate` in the
//! backend's own `nros-rmw.toml`, at the directory the provider scan resolved
//! (RFC-0071 D15, amended ruling 3; the walk of `packages/rmw/<family>/` that
//! stood here is gone). Cyclone DDS and uorb name no `crate` (C/C++ CMake
//! projects the Zephyr C port links), so they get no dep, and the backend
//! arrives through cmake. Such a backend still DECLARES the feature, empty and
//! off by default: the macro's `#[cfg(feature = …)]` names it, and an
//! undeclared feature is an `unexpected_cfgs` error in the `-D warnings` clippy
//! pass zephyr-lang-rust runs; enabled, its arm calls
//! `::nros_rmw_cyclonedds_sys::register()`, a crate a Zephyr image does not
//! link (phase-531 W6, both measured on the Cyclone Rust talker).

use std::path::{Path, PathBuf};

use super::paths::relative_or_err;
use crate::orchestration::cargo_metadata_schema::SystemToml;

/// The west-application half of a generated Zephyr entry.
///
/// Manifest LINES rather than a structured dependency type, for the reason
/// [`super::entry::EntrySpec::bringup_deps`] gives: a dependency spec is
/// already a small language, and re-modelling it here would be a second
/// grammar to keep in step with cargo's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WestApp {
    /// cmake `project()` identifier.
    pub project: String,
    /// `(cargo feature, backend crate)` — the `[features]` row the entry
    /// defaults to, and the optional dep it enables. The crate is `None` when
    /// the image's RMW is not a Rust crate (Cyclone DDS): the feature is still
    /// declared, empty, because `nros::main!` names it in a `#[cfg]` and an
    /// undeclared one is an `unexpected_cfgs` error under the `-D warnings`
    /// clippy pass zephyr-lang-rust runs (phase-531 W6, measured). `None`
    /// altogether only when the image names no RMW.
    pub rmw_feature: Option<(String, Option<String>)>,
    /// `[dependencies]` lines this road adds (the backend, the `zephyr` crate).
    pub deps: Vec<String>,
    /// The features on the backend's line in [`Self::deps`] that the image's
    /// declarations imply (its ROS edition and capabilities, RFC-0071 D9) as
    /// opposed to the board's platform feature. The entry writer marks them
    /// derived, so a materialised entry does not freeze them (RFC-0071 D13).
    pub backend_derived_features: Vec<String>,
    /// `[build-dependencies]` lines.
    pub build_deps: Vec<String>,
    /// `[patch.crates-io]` rows, `name = { path = "…" }`.
    pub patches: Vec<String>,
    /// The C/C++ half (phase-470 W5.b3). `Some` when the image's node graph
    /// crosses out of Rust, which is the same predicate that routes a NATIVE
    /// image to the cmake driver — see [`CmakeApp`].
    ///
    /// When it is `Some`, every field above is unused: a C/C++ west
    /// application has no `Cargo.toml` at all, so there is no manifest for the
    /// RMW feature, the deps or the patches to reach.
    pub cmake: Option<CmakeApp>,
    /// The bringup's declared capability axes (`param_services`,
    /// `lifecycle`, ...), for [`capability_kconfig_cmake`]. Issue 1702.
    ///
    /// Plus [`PARAM_STORE_AXIS`] when the LAUNCH seeds a parameter (issue
    /// 1706): not an axis anyone declares, but a fact the heap default needs
    /// beside them. Only Kconfig reads this list; `NROS_FEATURES` is
    /// derived from the bringup separately, so the pseudo-axis never reaches a
    /// cargo feature.
    pub capabilities: Vec<String>,
    /// phase-481 W1 (RFC-0098 D11) -- which image this application builds, for
    /// the nano-ros module's `module_ext_root` hook. `None` renders nothing
    /// (and the hook finds no `system.toml` beside a generated application).
    pub image: Option<ImageStatement>,
    /// phase-531 W6 -- does the image's backend need per-message type support
    /// that a Rust application has no other way to get? True when the backend
    /// declares a `[rmw.codegen] per_message` hook and no `[rmw.provides.cargo]
    /// crate` (Cyclone DDS: the Zephyr port links the C++ library, whose
    /// topic lookup is an idlc descriptor table). A C/C++ application gets the
    /// descriptors through `find_package(<pkg>)`; a Rust one calls neither, so
    /// [`Self::typesupport_packages`] carries them instead.
    pub needs_message_typesupport: bool,
    /// The image's interface closure in dependency order, as
    /// `nros_zephyr_interface_typesupport(PACKAGES …)` specs: `<pkg>` for an
    /// ament package, `<pkg>=<dir>` for one in the workspace. Filled by the
    /// caller when [`Self::needs_message_typesupport`]; empty renders nothing.
    pub typesupport_packages: Vec<String>,
}

/// phase-481 W1 (RFC-0098 D11) -- the three facts a generated application
/// states BEFORE `find_package(Zephyr)` so the module hook can render the
/// image's Kconfig fragment: a generated application has no `system.toml` of
/// its own, and the hook runs inside `find_package(Zephyr)`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageStatement {
    /// The bringup directory (holding the `system.toml`), relative to the
    /// application.
    pub system_dir: String,
    /// The `[image.<id>]` this application is.
    pub image_id: String,
    /// The entry's language, for the fragment's API row: `rust` for a
    /// `rust_cargo_application()` entry, `cpp` for a `nano_ros_add_executable`
    /// one (its entry is TYPED, i.e. C++, whatever its nodes are written in).
    pub language: String,
}

impl ImageStatement {
    /// The statement for `image_id` of the bringup at `bringup_dir`, as seen
    /// from the application at `entry_dir`.
    pub fn new(
        entry_dir: &Path,
        bringup_dir: &Path,
        image_id: &str,
        language: &str,
    ) -> Result<Self, String> {
        Ok(Self {
            system_dir: relative_or_err(entry_dir, bringup_dir)?,
            image_id: image_id.to_string(),
            language: language.to_string(),
        })
    }
}

/// The pre-`find_package(Zephyr)` lines an [`ImageStatement`] renders to.
#[must_use]
pub fn image_statement_cmake(image: Option<&ImageStatement>) -> String {
    let Some(i) = image else {
        return String::new();
    };
    format!(
        // History: phase-481 W1, RFC-0098 D11.
        "# Which image this is. The nano-ros Zephyr\n\
         # module's `module_ext_root` hook reads these DURING `find_package(Zephyr)`\n\
         # and renders the image's `system.toml` into a Kconfig fragment placed\n\
         # last in `EXTRA_CONF_FILE`.\n\
         get_filename_component(NROS_IMAGE_SYSTEM_DIR\n    \
         \"${{CMAKE_CURRENT_SOURCE_DIR}}/{}\" ABSOLUTE)\n\
         set(NROS_IMAGE {})\n\
         set(NROS_IMAGE_LANGUAGE {})\n\n",
        i.system_dir, i.image_id, i.language
    )
}

/// Issue 1702 -- the capability axes a Kconfig default depends on, and the
/// symbol each one sets.
pub const CAPABILITY_KCONFIG: &[(&str, &str)] = &[
    ("param_services", "CONFIG_NROS_CAPABILITY_PARAM_SERVICES"),
    ("lifecycle", "CONFIG_NROS_CAPABILITY_LIFECYCLE"),
    // phase-556 W13 -- `/rosout` is a stock node interface; the symbol lets a
    // hand-written Zephyr application's module derive `NROS_FEATURES` from it.
    ("rosout", "CONFIG_NROS_CAPABILITY_ROSOUT"),
    // phase-556 W11 -- `use_sim_time` (the `/clock` time source), a stock
    // node interface too.
    ("sim_time", "CONFIG_NROS_CAPABILITY_SIM_TIME"),
    (PARAM_STORE_AXIS, "CONFIG_NROS_PARAM_STORE"),
];

/// Issue 1706 -- a launch `<param>` seeds the parameter store through the same
/// declare path a `param_services` image uses (`shared/declare_calls.jinja`:
/// "seeding params is independent of whether the param-SERVICES surface is
/// enabled"), so the image needs the store's heap whether or not it declares
/// the axis. It implies the STORE, never the services.
pub const PARAM_STORE_AXIS: &str = "param_store";

/// Issue 1702 -- the declared capability axes as Kconfig assignments, set
/// BEFORE `find_package(Zephyr)` in the generated application.
///
/// Both capabilities allocate from the nros heap (`CONFIG_NROS_ZEPHYR_HEAP_SIZE`):
/// the parameter store is ONE 285,696-byte allocation at the default sizing and
/// the lifecycle services own a buffer pair each. Nothing connected the
/// declaration to the heap, so an image declaring `param_services` linked and
/// then halted at boot on the 64 KiB default. With these symbols set,
/// `zephyr/Kconfig` derives the heap's DEFAULT from the declaration, and a value
/// a conf fragment states still wins -- a Kconfig default is only a default.
///
/// Zephyr reads a `CONFIG_*` CACHE variable as a command-line Kconfig
/// assignment (`kconfig.cmake`), so this is the documented way in. Here and not
/// on `nros build`'s `west build` line: the fixture lanes re-run an already
/// configured build with plain `ninja`, which reconfigures from THIS file and
/// never re-reads a `-D`, so a `-D` would lag the declaration by one `nros
/// build`. EVERY axis is written, `n` when not declared, with `FORCE`: a cache
/// entry outlives the line that set it, so dropping an axis must reset it.
#[must_use]
pub fn capability_kconfig_cmake(capabilities: &[String]) -> String {
    let mut out = String::from(
        // History: issue 1702.
        "# The declared capability axes as Kconfig: the nros heap's\n\
         # default follows them. Set before `find_package(Zephyr)`, which is\n\
         # where Kconfig runs.\n",
    );
    for (axis, sym) in CAPABILITY_KCONFIG {
        // phase-461 W6 -- the `params` axis states the store the way a launch
        // seed implies it: `CONFIG_NROS_PARAM_STORE`, never the services.
        let on = capabilities
            .iter()
            .any(|c| c == axis || (*axis == PARAM_STORE_AXIS && c == "params"));
        out.push_str(&format!(
            "set({sym} {} CACHE STRING \"declared by the bringup\" FORCE)\n",
            if on { "y" } else { "n" }
        ));
    }
    out.push('\n');
    out
}

/// The C/C++ half of a west application (phase-470 W5.b3, issue 1288).
///
/// ## Why this is a field on [`WestApp`] and not a second module
///
/// W5.a predicted "a C/C++ arm adds FIELDS rather than a second emitter", and
/// that is **half** right, which is worth stating plainly because the half that
/// is wrong is the one a reader would rely on. The *resolution* half really
/// does collapse: one `write`, one `is_materialized` guard, one `project_name`,
/// one directory (`build/<coord>/<id>_entry/`). The *rendering* half does not —
/// `rust_cargo_application()` and `nano_ros_add_executable()` share the four
/// lines above them and nothing below, so [`render_cmakelists`] has two arms.
///
/// ## What a C/C++ Zephyr application is
///
/// Measured across the eight hand-written ones: `find_package(Zephyr)`,
/// `project()`, `find_package(nano_ros)`, one `add_subdirectory` per node
/// package the launch file names, and ONE `nano_ros_add_executable` call. Every
/// difference between them is a declaration the image already carries or can:
///
/// | axis | where it comes from |
/// | --- | --- |
/// | `LANG c` | the node packages' own sources — `c` unless one is C++ |
/// | `PANIC platform` | `[image.<id>] panic` |
/// | `BRINGUP` | the bringup being built, which is what makes `realtime-c`'s `if(CONFIG_SMP)` switch disappear: two bringups are two images |
/// | `NROS_WS_RUST_NODE_DIRS` | a node package that is a cargo crate |
/// | `nano_ros_use_board(...)` + `EXTRA_CONF_FILE` | NOT emitted; the board files go on the `west build` line — see [`resolve_cmake`] |
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmakeApp {
    /// `(source dir relative to the application, binary dir name)` for each
    /// node package the launch file names.
    pub subdirs: Vec<(String, String)>,
    /// Target name — `<id>_entry`, the same derivation the Rust side uses.
    pub target: String,
    /// `BRINGUP` — relative to the application.
    pub bringup: String,
    /// `LAUNCH` — a launch file name, or `default` for the bringup's own.
    pub launch: String,
    /// `LAUNCH_ARGS k=v`.
    pub args: Vec<(String, String)>,
    /// `LANG` — `c` or `cpp`.
    pub lang: String,
    /// `BOARD` / `DEPLOY` — the board FAMILY token (`zephyr`), which is the
    /// platform. NOT the image's board string: `board_family()` matches exact
    /// tokens and an unknown one falls through to the HOST default, which emits
    /// `int main(int, char**)` where the Zephyr kernel declares
    /// `extern int main(void)`. The hand-written files this replaced all said
    /// `zephyr`, and ~~`zephyr_cyclonedds_entry`~~'s header recorded the
    /// measurement.
    pub deploy: String,
    /// `PANIC` — RFC-0077 policy, when the image declares one.
    pub panic: Option<String>,
    /// `NROS_WS_RUST_NODE_DIRS` — node packages that are cargo crates, relative
    /// to the application. Set BEFORE `find_package(Zephyr)`, which is the one
    /// ordering constraint here: the nano-ros Zephyr module reads it DURING
    /// find_package to decide whether to build the `nros_ws_runtime` umbrella
    /// (nros-cpp + the node) in place of plain nros-cpp — the single-runtime
    /// invariant, one Rust staticlib and one `nros-rmw-cffi` registry.
    pub rust_node_dirs: Vec<String>,
    /// `NROS_FEATURES` -- the bringup's capability axes (`[system]
    /// features` and the deprecated typed blocks), set BEFORE
    /// `find_package(Zephyr)` for the same reason as `rust_node_dirs`: the
    /// nano-ros Zephyr module reads it DURING find_package, to add
    /// `param-services` to the nros-cpp cargo build and to define
    /// `NROS_SYSTEM_PARAM_SERVICES`. Issue 1681 -- nothing on this road set it,
    /// so an image whose generated entry calls
    /// `nros_cpp_register_parameter_services` linked an nros-cpp built without
    /// it.
    pub features: Vec<String>,
    /// `nros_register_cargo_node(...)` arguments, one per Rust node package of
    /// the image that ships no `CMakeLists.txt`: registered from the bringup's
    /// `[[component]]` row ([`super::cargo_node`]) after the subdirs, with its
    /// directory relative to the application. Such a package is in
    /// [`Self::rust_node_dirs`] too, and is never a subdir.
    pub cargo_nodes: Vec<String>,
    /// Workspace packages of the image's closure that are Zephyr modules (a
    /// `zephyr/module.yml`), relative to the application. Appended to
    /// `ZEPHYR_EXTRA_MODULES` BEFORE `find_package(Zephyr)`, which is where
    /// Zephyr reads the module list: Kconfig is parsed inside find_package,
    /// so a package's own Kconfig symbols can reach the image no other way.
    pub zephyr_modules: Vec<String>,
    /// The workspace's `src/`, relative to the application: where the
    /// configure finds the workspace's own interface packages.
    pub interface_src: String,
}

/// A cmake `project()` identifier for this image.
///
/// The hand-written applications this replaced spelled it by hand
/// (~~`nros_zephyr_workspace_entry_rs`~~, ~~`nros_zephyr_ws_qos_entry_rs`~~, …), a
/// name nobody read and no two of them agreed on. Derived from the two
/// things that actually identify the application — the workspace directory and
/// the image — so it is unique across a west build root, which is the only
/// property cmake needs of it.
#[must_use]
pub fn project_name(workspace: &Path, image_id: &str) -> String {
    let ws = workspace
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");
    let sanitize = |s: &str| {
        s.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>()
    };
    format!("nros_{}_{}", sanitize(ws), sanitize(image_id))
}

/// Whether `dir` is a Zephyr application in its own right: its
/// `CMakeLists.txt` calls `find_package(Zephyr)` outside a comment.
///
/// That call is what makes a directory a west application (every
/// `examples/zephyr/<lang>/<example>/` has one, and so does a `nros new
/// --board <zephyr board>` package), and it is also what a HOST CMake
/// configure cannot get past: Zephyr's boilerplate stops at "BOARD is not
/// being defined" (issue 2280). A workspace node package or bringup is an
/// ament package and never names Zephyr; a directory with no
/// `CMakeLists.txt` is not an application at all.
#[must_use]
pub fn cmakelists_finds_zephyr(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("CMakeLists.txt")).is_ok_and(|cmake| {
        cmake
            .lines()
            .map(|l| l.split('#').next().unwrap_or(""))
            .any(|l| l.contains("find_package(Zephyr"))
    })
}

/// Derive the west-application facts for one image.
///
/// `rmw` is the image's effective backend (`[image.<id>] rmw`, already folded
/// with `[image_defaults]` and the system header by the caller).
///
/// `sys` is the bringup's `system.toml`, which the image belongs to: it
/// states the image's ROS edition and capabilities, which the backend
/// dependency carries (RFC-0071 D9, issue 2393).
///
/// The adapter over [`resolve_in`]: the RMW index is `workspace`'s.
#[allow(clippy::too_many_arguments)]
pub fn resolve(
    workspace: &Path,
    entry_dir: &Path,
    nros_root: &Path,
    image_id: &str,
    rmw: Option<&str>,
    sys: &SystemToml,
    platform_feature: &str,
    patches: &std::collections::BTreeMap<String, PathBuf>,
) -> Result<WestApp, String> {
    // phase-545 W1: threaded from build/configure after phase-530 W2
    let rmws = crate::orchestration::rmw_index::for_workspace(workspace)
        .map_err(|e| format!("`{image_id}`: {e:#}"))?;
    resolve_in(
        &rmws,
        workspace,
        entry_dir,
        nros_root,
        image_id,
        rmw,
        sys,
        platform_feature,
        patches,
    )
}

/// [`resolve`] with the image's backend resolved through `rmws` (RFC-0071
/// D14).
#[allow(clippy::too_many_arguments)]
pub fn resolve_in(
    rmws: &crate::orchestration::rmw_index::RmwIndex,
    workspace: &Path,
    entry_dir: &Path,
    nros_root: &Path,
    image_id: &str,
    rmw: Option<&str>,
    sys: &SystemToml,
    platform_feature: &str,
    patches: &std::collections::BTreeMap<String, PathBuf>,
) -> Result<WestApp, String> {
    let mut app = WestApp {
        project: project_name(workspace, image_id),
        ..Default::default()
    };

    // `zephyr-build` and `zephyr` are registry names on purpose: the only
    // thing that knows where the rust module lives is the Zephyr build itself,
    // which passes `--config patch.crates-io.zephyr-build.path=…` on the cargo
    // command line. Deriving that path here would be a second answer to a
    // question west already answers, and the two would drift the first time a
    // workspace moved its modules. (The `zephyr` RUNTIME dep is the board
    // descriptor's `crate_root_deps`, beside the `extern crate zephyr;` it
    // serves — see `builder::entry`.)
    app.build_deps.push("zephyr-build = \"0.1.0\"".to_string());
    let zb = nros_root.join("packages/tooling/nros-zephyr-build");
    let rel = relative_or_err(entry_dir, &zb)?;
    // A PATH dep, not the `nros-zephyr-build = "*"` the hand-written entries
    // use. Those resolve through a `[patch.crates-io]` row `nros sync` writes
    // into the workspace's `.cargo/config.toml`, i.e. through a file this road
    // does not control and a user may not have synced; the path is a fact this
    // process already holds.
    app.build_deps
        .push(format!("nros-zephyr-build = {{ path = \"{rel}\" }}"));

    // The backend. See the module docs: this is what makes the macro's
    // `register()` reachable, and no `crate` is a real answer.
    if let Some(declared) = rmw {
        let resolved = rmws
            .resolve(declared)
            .map_err(|e| format!("`{image_id}`: {e}"))?;
        // Issue 2487 — a declared capability this backend does not carry is
        // refused here as on the cargo road, never dropped.
        if let Some(refusal) =
            crate::orchestration::backend_link::undeclared_capability_refusal(rmws, &resolved, sys)
        {
            return Err(format!("`{image_id}`: {refusal}"));
        }
        // Issue 2704 -- the sibling: an enabled node interface whose runtime
        // types this backend cannot be given is refused, never booted without
        // its services.
        let search = std::env::var_os("NROS_INTERFACE_SEARCH_PATH");
        if let Some(refusal) = crate::orchestration::backend_link::runtime_interface_types_refusal(
            &resolved,
            sys,
            &|p| {
                crate::orchestration::backend_link::interface_package_on_index(search.as_deref(), p)
            },
        ) {
            return Err(format!("image `{image_id}`: {refusal}"));
        }
        // RFC-0071 D15 — the crate is `[rmw.provides.cargo] crate`, at the
        // directory the scan resolved (amended ruling 3), so a backend outside
        // the SDK root is found the same way as one inside it. No `crate` is a
        // real answer (cyclonedds, uorb: C/C++ projects the Zephyr C port
        // links); a `crate` with no such crate at its directory is the
        // provider's error, refused rather than built with no Rust backend.
        let crate_dir = crate::orchestration::backend_link::crate_dir(&resolved)
            .map_err(|e| format!("`{image_id}`: {e}"))?;
        app.needs_message_typesupport =
            crate_dir.is_none() && !resolved.descriptor.per_message_hook.is_empty();
        if let Some(dir) = crate_dir {
            let krate = resolved.descriptor.cargo_crate.as_str();
            let rel = relative_or_err(entry_dir, &dir)?;
            // RFC-0071 D9 — the same feature list the cargo road's direct
            // backend dependency carries (`backend_link::dependency_features`):
            // the edition and capabilities, then the board's platform feature.
            // Before issue 2393 this forwarded the platform feature alone.
            let edition = sys
                .image_ros_edition(Some(image_id))
                .map_err(|e| format!("`{image_id}`: {e:#}"))?;
            let platform = [platform_feature.to_string()];
            let features = crate::orchestration::backend_link::dependency_features(
                &resolved, &dir, sys, edition, &platform,
            );
            app.backend_derived_features = features
                .iter()
                .filter(|f| f.as_str() != platform_feature)
                .cloned()
                .collect();
            let feats = if features.is_empty() {
                String::new()
            } else {
                let list: Vec<String> = features.iter().map(|f| format!("\"{f}\"")).collect();
                format!(", features = [{}]", list.join(", "))
            };
            app.deps.push(format!(
                "{krate} = {{ path = \"{rel}\", default-features = false{feats}, \
                 optional = true }}"
            ));
            app.rmw_feature = Some((resolved.cargo_feature.clone(), Some(krate.to_string())));
        } else {
            // No crate to enable, and still a feature to declare: the macro's
            // `#[cfg(feature = "<rmw feature>")]` arm is evaluated on THIS crate.
            // The hand-written leaves carried `rmw-cyclonedds = []` for it.
            app.rmw_feature = Some((resolved.cargo_feature.clone(), None));
        }
    }

    // The entry is its own cargo root (RFC-0098 D9), so nothing above it
    // supplies a `[patch]`. Derived by the same walk the cargo road's settings
    // file uses (`cmd::build::registry_patches`), so the two roads patch the
    // same set.
    for (name, path) in patches {
        let rel = relative_or_err(entry_dir, path)?;
        app.patches.push(format!("{name} = {{ path = \"{rel}\" }}"));
    }

    Ok(app)
}

/// A workspace package's `zephyr/module.yml`, as far as the generated
/// application needs it: whether the module has CMake of its own, and where.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ZephyrModule {
    /// `build.cmake` — the module's CMake directory, relative to the package.
    cmake: Option<String>,
    /// `build.cmake-ext` — the module's CMake is supplied by someone else.
    cmake_ext: bool,
}

impl ZephyrModule {
    /// `Ok(None)` when `dir` is not a Zephyr module. A `module.yml` that does
    /// not parse is refused, naming the file: Zephyr would refuse it too, later
    /// and less legibly.
    fn read(dir: &Path) -> Result<Option<Self>, String> {
        let path = dir.join("zephyr").join("module.yml");
        if !path.is_file() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text)
            .map_err(|e| format!("parsing {}: {e}", path.display()))?;
        let build = doc.get("build");
        let cmake = build
            .and_then(|b| b.get("cmake"))
            .and_then(|c| c.as_str())
            .map(str::to_string);
        let cmake_ext = build
            .and_then(|b| b.get("cmake-ext"))
            .and_then(serde_yaml_ng::Value::as_bool)
            .unwrap_or(false);
        Ok(Some(Self { cmake, cmake_ext }))
    }

    /// Whether Zephyr builds the package's OWN top-level `CMakeLists.txt` as
    /// this module's CMake (`build.cmake: .`). A module whose CMake is a
    /// separate directory (`build.cmake: zephyr`) leaves the package's own
    /// CMakeLists to the application.
    fn builds_package_cmake(&self, dir: &Path) -> bool {
        if self.cmake_ext {
            return false;
        }
        let Some(rel) = &self.cmake else {
            return false;
        };
        let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        canon(&dir.join(rel)) == canon(dir)
    }
}

/// Is this node package a cargo crate?
///
/// The `NROS_WS_RUST_NODE_DIRS` question, asked of the package rather than of a
/// registry: a Rust node in a cmake workspace may carry a `CMakeLists.txt`
/// registering it with `LANGUAGE RUST`, or none (then its registration is
/// derived, [`super::cargo_node`]), so the manifest is the discriminator and
/// the build file is not.
fn is_cargo_node(dir: &Path) -> bool {
    dir.join("Cargo.toml").is_file()
}

/// Derive the C/C++ west-application facts for one image.
///
/// `nodes` is the launch closure in BUILD order (`package_closure`: every
/// package after the workspace packages it `<depend>`s on, ties by name), and
/// the subdirs are emitted in that order, unsorted. The order is
/// deterministic, so the file is still byte-identical across machines (W3.c).
/// Sorting by name here was issue 2756: a package whose standalone fallback
/// `add_subdirectory()`s its dependency ran before the dependency's own line,
/// and the second add of the same target failed the configure.
///
/// **What this deliberately does NOT emit: a `nano_ros_use_board(...)`
/// preamble** (the retired `fvp_entry`'s). That stated the board inside the
/// application, which is the state this whole item exists to end — `[image.fvp]
/// board` says it (issue 1517) and `nros build` passes it as `-b`. The board
/// package's FILES the preamble also added (base `prj.conf`,
/// `boards/<board>.{conf,overlay}`) reach a generated application on the
/// `west build` line instead, from `board_projection::board_files` (issue
/// 1288), so they are derived by one rule for both roads.
#[allow(clippy::too_many_arguments)]
pub fn resolve_cmake(
    workspace: &Path,
    entry_dir: &Path,
    bringup_dir: &Path,
    image_id: &str,
    launch: Option<&str>,
    args: &std::collections::BTreeMap<String, String>,
    panic: Option<&str>,
    platform: &str,
    nodes: &[(String, PathBuf)],
) -> Result<WestApp, String> {
    let mut subdirs = Vec::new();
    let mut rust_node_dirs = Vec::new();
    let mut zephyr_modules = Vec::new();
    let mut lang = "c";
    // A Rust node package with no CMakeLists of its own: registered from its
    // bringup row, the rule the native cmake root uses too.
    let mut cargo_nodes = Vec::new();
    let derived = super::cargo_node::derived_registrations(
        bringup_dir,
        nodes.iter().map(|(n, d)| (n.as_str(), d.as_path())),
    )?;
    if !derived.is_empty() {
        // A Rust node: the C++ entry pack is the one that constructs it
        // (`discover::needs_cpp_entry`). Its package has no CMakeLists.txt,
        // so the scan below never reaches it.
        lang = "cpp";
    }
    for n in &derived {
        let rel = relative_or_err(entry_dir, &n.dir)?;
        cargo_nodes.push(n.cmake_args(&format!("${{CMAKE_CURRENT_SOURCE_DIR}}/{rel}")));
        rust_node_dirs.push(rel);
    }
    for (name, dir) in nodes {
        let module = ZephyrModule::read(dir)?;
        if let Some(m) = &module {
            zephyr_modules.push(relative_or_err(entry_dir, dir)?);
            if m.builds_package_cmake(dir) {
                // Zephyr already runs this CMakeLists as the module's own;
                // adding it again would define every target twice.
                continue;
            }
        }
        if !dir.join("CMakeLists.txt").is_file() {
            // A module that brings Kconfig only, with no CMake of its own.
            continue;
        }
        subdirs.push((relative_or_err(entry_dir, dir)?, name.clone()));
        if is_cargo_node(dir) {
            rust_node_dirs.push(relative_or_err(entry_dir, dir)?);
        }
        if super::discover::needs_cpp_entry(dir) {
            lang = "cpp";
        }
    }
    rust_node_dirs.sort();
    zephyr_modules.sort();

    Ok(WestApp {
        project: project_name(workspace, image_id),
        cmake: Some(CmakeApp {
            subdirs,
            target: super::entry::package_name(image_id),
            bringup: relative_or_err(entry_dir, bringup_dir)?,
            launch: launch.unwrap_or("default").to_string(),
            args: args.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            lang: lang.to_string(),
            deploy: platform.to_string(),
            panic: panic.map(str::to_string),
            rust_node_dirs,
            cargo_nodes,
            zephyr_modules,
            interface_src: relative_or_err(entry_dir, &workspace.join("src"))?,
            features: crate::cmd::build::declared_capabilities_for(bringup_dir, Some(image_id))
                .into_iter()
                .map(str::to_string)
                .collect(),
        }),
        capabilities: crate::cmd::build::declared_capabilities_for(bringup_dir, Some(image_id))
            .into_iter()
            .map(str::to_string)
            .collect(),
        ..Default::default()
    })
}

/// Render the application's `CMakeLists.txt`.
///
/// Deliberately four lines of cmake. The hand-written applications carry an
/// `if(CONFIG_NROS_RMW_ZENOH) … EXTRA_CARGO_ARGS --features rmw-zenoh` ladder
/// because a single package served every RMW and Kconfig was the only thing
/// that could choose. A generated application is per IMAGE and the image
/// DECLARES its `rmw`, so the choice is already made — it reaches cargo as the
/// manifest's `[features] default`, which needs no `EXTRA_CARGO_ARGS` at all.
#[must_use]
pub fn render_cmakelists(app: &WestApp) -> String {
    with_digest(&render_cmakelists_body(app))
}

/// Issue 1707 -- the cmake variable a generated application sets to its own
/// fingerprint. `cmake/NanoRosImageAgreement.cmake`'s
/// `nros_check_generated_app_current` compares it with the line on disk.
pub const APP_DIGEST_VAR: &str = "NROS_GENERATED_APP_DIGEST";

/// Issue 1707 -- stamp the rendered application with a fingerprint of itself.
///
/// The application is regenerated by EVERY `plan_builds` over its workspace,
/// and until issue 1716 one of those ran INSIDE the configure that reads it:
/// `nros_check_image_agreement` calls `nros image-facts --for-entry`, which
/// planned the whole workspace (it now uses `plan_query`, which writes
/// nothing; a concurrent `nros build` of a sibling image still can). So when
/// the declaration changed after the last
/// `nros build`, the configure reads the old file, the query rewrites it, and
/// the build system is written AFTER the rewrite -- newer than its input, so no
/// later `ninja` reconfigures. The image keeps the previous declaration's
/// Kconfig and `NROS_FEATURES` with nothing saying so.
///
/// The variable is what this configure EXECUTED; the line on disk is what the
/// file holds now. They differ exactly when the file was rewritten after cmake
/// read it, which no mtime or before/after hash taken from inside the configure
/// can tell without a window. A digest of the body (without this line) is a
/// fact the file states about itself, so the comparison has none.
#[must_use]
fn with_digest(body: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = format!("{:x}", Sha256::digest(body.as_bytes()));
    // History: issue 1707.
    let anchor = "cmake_minimum_required(VERSION 3.20.0)\n";
    let line = format!(
        "\n# This file's own fingerprint. The nano-ros Zephyr module\n\
         # compares it with the line on disk once the configure has run, and\n\
         # refuses a configure that read a file which has since been rewritten.\n\
         set({APP_DIGEST_VAR} {digest})\n"
    );
    match body.find(anchor) {
        Some(at) => {
            let split = at + anchor.len();
            format!("{}{line}{}", &body[..split], &body[split..])
        }
        // Both renderers emit the anchor; a body without one is a renderer
        // change this test suite catches (`the_application_states_its_own_digest`).
        None => format!("{body}{line}"),
    }
}

fn render_cmakelists_body(app: &WestApp) -> String {
    if let Some(c) = &app.cmake {
        return render_cmake_app(&app.project, c, &app.capabilities, app.image.as_ref());
    }
    format!(
        // History: phase-470 W5.a, RFC-0065 D4.
        "# GENERATED by `nros build` — DO NOT EDIT.\n\
         #\n\
         # The west application for one `[image.*]`. Its Rust half is the\n\
         # `Cargo.toml` + `src/lib.rs` beside this file; zephyr-lang-rust's\n\
         # `rust_cargo_application()` builds them from THIS directory, which\n\
         # is why the two share one.\n\
         #\n\
         # The image's Kconfig is NOT here: `nros build` passes it as\n\
         # `-DAPPLICATION_CONFIG_DIR` + `-DEXTRA_CONF_FILE` from the bringup,\n\
         # so this file states no `prj.conf` and no board.\n\
         #\n\
         # `nros materialize <image>` takes ownership of this directory.\n\n\
         cmake_minimum_required(VERSION 3.20.0)\n\n\
         {}{}\
         find_package(Zephyr REQUIRED HINTS $ENV{{ZEPHYR_BASE}})\n\
         project({})\n\n\
         {}\
         rust_cargo_application()\n",
        image_statement_cmake(app.image.as_ref()),
        capability_kconfig_cmake(&app.capabilities),
        app.project,
        typesupport_cmake(&app.typesupport_packages),
    )
}

/// phase-531 W6 -- the per-message type support of a Rust application, or
/// nothing. See [`WestApp::needs_message_typesupport`]: before this, a Rust
/// Cyclone image got its idlc descriptors only from a hand-written block in the
/// leaf's `CMakeLists.txt`, so a generated one had none and every
/// `find_descriptor()` failed at run time.
fn typesupport_cmake(packages: &[String]) -> String {
    if packages.is_empty() {
        return String::new();
    }
    format!(
        "# The image's backend looks each topic type up in a per-message table\n\
         # that a C/C++ image fills through `find_package(<pkg>)`. A Rust image\n\
         # calls no find_package, so its interface closure is named here, in\n\
         # dependency order.\n\
         nros_zephyr_interface_typesupport(PACKAGES\n    {})\n\n",
        packages.join("\n    ")
    )
}

/// Render a C/C++ west application's `CMakeLists.txt` (phase-470 W5.b3).
///
/// The ordering is the hand-written files' own and two rungs of it are
/// load-bearing:
///
/// * `NROS_WS_RUST_NODE_DIRS` **before** `find_package(Zephyr)` — the nano-ros
///   Zephyr module reads it while find_package runs, to decide whether to build
///   the `nros_ws_runtime` umbrella instead of plain nros-cpp.
/// * the `nano_ros_workspace_pkg_guard` stub **after**
///   `find_package(nano_ros)` and **before** the `add_subdirectory` calls — a
///   Rust node package opens with that guard and would otherwise bootstrap a
///   SECOND nano-ros import, which on Zephyr means `add_subdirectory`-ing the
///   checkout with `NROS_PLATFORM=zephyr`, which `nros-c` rejects.
///   (W5.b3's brief said this stub goes before `find_package`; measured against
///   `examples/workspaces/mixed/src/zephyr_entry`, it does not — only
///   `NROS_WS_RUST_NODE_DIRS` does, and the two are separate lines with
///   separate reasons.)
#[must_use]
fn render_cmake_app(
    project: &str,
    c: &CmakeApp,
    capabilities: &[String],
    image: Option<&ImageStatement>,
) -> String {
    let mut out = String::new();
    out.push_str(
        // History: phase-470 W5.b3, RFC-0065 D4, phase-383 W3.c.
        "# GENERATED by `nros build` — DO NOT EDIT.\n\
         #\n\
         # The west application for one C/C++ `[image.*]`. It has no Rust half:\n\
         # `nano_ros_add_executable(... BRINGUP ...)` generates the entry TU\n\
         # that carries `main`, which is why no `SOURCES` appear below.\n\
         #\n\
         # The image's Kconfig is NOT here: `nros build` passes it as\n\
         # `-DAPPLICATION_CONFIG_DIR` + `-DEXTRA_CONF_FILE` from the bringup,\n\
         # so this file states no `prj.conf` and no board.\n\
         #\n\
         # Paths are RELATIVE and the subdir list is SORTED, so this file is\n\
         # byte-identical across machines.\n\
         #\n\
         # `nros materialize <image>` takes ownership of this directory.\n\n\
         cmake_minimum_required(VERSION 3.20.0)\n\n",
    );
    if !c.rust_node_dirs.is_empty() {
        out.push_str(
            "# The workspace's Rust node packages, bundled into the single\n\
             # `nros_ws_runtime` umbrella staticlib (ONE Rust staticlib, ONE\n\
             # `nros-rmw-cffi` registry). MUST be set before\n\
             # `find_package(Zephyr)`: the nano-ros Zephyr module reads it\n\
             # DURING find_package to decide whether to build and link the\n\
             # umbrella in place of plain nros-cpp.\n",
        );
        for (i, d) in c.rust_node_dirs.iter().enumerate() {
            out.push_str(&format!(
                "get_filename_component(_nros_rust_node_{i}\n    \
                 \"${{CMAKE_CURRENT_SOURCE_DIR}}/{d}\" ABSOLUTE)\n"
            ));
        }
        let refs: Vec<String> = (0..c.rust_node_dirs.len())
            .map(|i| format!("${{_nros_rust_node_{i}}}"))
            .collect();
        out.push_str(&format!(
            "set(NROS_WS_RUST_NODE_DIRS \"{}\")\n\n",
            refs.join(";")
        ));
    }
    if !c.features.is_empty() {
        out.push_str(
            // History: issue 1681.
            "# The bringup's capability axes. MUST be set before\n\
             # `find_package(Zephyr)`: the nano-ros Zephyr module reads it DURING\n\
             # find_package to pick the nros-cpp cargo features (`param-services`)\n\
             # and the `NROS_SYSTEM_*` defines the generated entry relies on.\n",
        );
        out.push_str(&format!(
            "set(NROS_FEATURES \"{}\")\n\n",
            c.features.join(";")
        ));
    }
    if !c.zephyr_modules.is_empty() {
        out.push_str(
            // History: issue 2594, phase-478 I11; issue 2609 (GitHub #3948).
            "# Workspace packages of this image that are Zephyr modules. MUST be\n\
             # added before `find_package(Zephyr)`: Zephyr reads the module list\n\
             # and parses each module's Kconfig DURING find_package. The nano-ros\n\
             # module arrives on the `west build` line as a CACHE entry, and\n\
             # Zephyr reads the cache before a normal variable, so these are\n\
             # added to that cache entry rather than to a variable it would\n\
             # never read.\n\
             set(_nros_zephyr_modules ${ZEPHYR_EXTRA_MODULES})\n",
        );
        for (i, d) in c.zephyr_modules.iter().enumerate() {
            out.push_str(&format!(
                "get_filename_component(_nros_zephyr_module_{i}\n    \
                 \"${{CMAKE_CURRENT_SOURCE_DIR}}/{d}\" ABSOLUTE)\n\
                 list(APPEND _nros_zephyr_modules \"${{_nros_zephyr_module_{i}}}\")\n"
            ));
        }
        out.push_str(
            "list(REMOVE_DUPLICATES _nros_zephyr_modules)\n\
             set(ZEPHYR_EXTRA_MODULES \"${_nros_zephyr_modules}\" CACHE STRING\n    \
             \"Zephyr modules: nano-ros and this image's workspace modules\" FORCE)\n\n",
        );
    }
    // Issue 2610 (GitHub #3952): the workspace's own interface packages, by
    // the CMake root's rule. They are not subdirectories, so without this a
    // node's message dependency on one is unseen by the configure.
    out.push_str(&super::cmake_root::interface_search_path_cmake(
        &c.interface_src,
    ));
    out.push_str(&image_statement_cmake(image));
    out.push_str(&capability_kconfig_cmake(capabilities));
    out.push_str(&format!(
        // History: RFC-0048 (287-W6).
        "find_package(Zephyr REQUIRED HINTS $ENV{{ZEPHYR_BASE}})\nproject({project})\n\n"
    ));
    out.push_str(
        "# The ament shape: on Zephyr `find_package(nano_ros)`\n\
         # supplies the verbs + entry/register machinery WITHOUT re-importing\n\
         # the runtime — the nano-ros west module (loaded by\n\
         # `find_package(Zephyr)`) already provides NanoRos::*.\n\
         find_package(nano_ros REQUIRED)\n\n",
    );
    if !c.rust_node_dirs.is_empty() {
        out.push_str(
            "# A Rust node package opens with the workspace guard (LANGUAGE RUST\n\
             # is outside the C/C++ verb surface). Stub it so it does NOT\n\
             # bootstrap a second nano-ros import: on Zephyr the west module\n\
             # owns the runtime, and an import here would `add_subdirectory` the\n\
             # checkout with `NROS_PLATFORM=zephyr`, which `nros-c` rejects.\n\
             if(NOT COMMAND nano_ros_workspace_pkg_guard)\n    \
             function(nano_ros_workspace_pkg_guard)\n    \
             endfunction()\nendif()\n\n",
        );
    }
    out.push_str(
        "# The image's package closure: the node packages the launch file names\n\
         # and the workspace packages they depend on. A node's registration\n\
         # carries no DEPLOY, so it stays component-only; the entry's auto-link\n\
         # sidecar pulls it into `app`.\n",
    );
    for (dir, name) in &c.subdirs {
        out.push_str(&format!(
            "add_subdirectory(\"${{CMAKE_CURRENT_SOURCE_DIR}}/{dir}\" {name})\n"
        ));
    }
    if !c.cargo_nodes.is_empty() {
        out.push_str(
            "\n# The image's Rust node packages that ship no CMakeLists.txt,\n\
             # registered from the bringup's `[[component]]` rows.\n",
        );
        for args in &c.cargo_nodes {
            out.push_str(&format!("nros_register_cargo_node({args})\n"));
        }
    }
    out.push_str(
        "\n# BOARD and DEPLOY are the board FAMILY token, which on this road is\n\
         # the PLATFORM. Not the image's board string: `board_family()` matches\n\
         # exact tokens and an unknown one falls through to the HOST default,\n\
         # which emits `int main(int, char**)` where the Zephyr kernel declares\n\
         # `extern int main(void)`.\n",
    );
    out.push_str(&format!("nano_ros_add_executable({}\n", c.target));
    out.push_str(&format!("    BOARD   {}\n", c.deploy));
    out.push_str(&format!(
        "    BRINGUP \"${{CMAKE_CURRENT_SOURCE_DIR}}/{}\"\n",
        c.bringup
    ));
    out.push_str(&format!("    LAUNCH  {}\n", c.launch));
    for (k, v) in &c.args {
        out.push_str(&format!("    LAUNCH_ARGS {k}={v}\n"));
    }
    out.push_str(&format!("    LANG    {}\n", c.lang));
    if let Some(p) = &c.panic {
        out.push_str(&format!("    PANIC   {p}\n"));
    }
    out.push_str("    TYPED\n");
    out.push_str(&format!("    DEPLOY  {})\n", c.deploy));
    out
}

/// Render the application's `build.rs`.
///
/// Byte-identical across all seven hand-written Rust entries (measured: one
/// md5), which is what makes it generated rather than authored.
#[must_use]
pub fn render_build_rs() -> String {
    // History: phase-470 W5.a.
    "// GENERATED by `nros build` — DO NOT EDIT.\n\
     //\n\
     // The two Kconfig bridges a Zephyr Rust application needs:\n\
     //   - `export_kconfig_bool_options()` — Kconfig -> `cfg(…)`\n\
     //   - `bake_nros_config()` — the locator / domain / XRCE bake\n\
     //     (`packages/tooling/nros-zephyr-build`, which owns the rationale).\n\
     fn main() {\n    \
     zephyr_build::export_kconfig_bool_options();\n    \
     nros_zephyr_build::bake_nros_config();\n\
     }\n"
    .to_string()
}

/// Write the application shell beside an already-written entry package.
///
/// **Refuses to touch a materialised entry**, for the same reason
/// [`super::entry::write`] does: once a user owns the directory the builder
/// leaves all of it alone, `CMakeLists.txt` included.
pub fn write(app: &WestApp, dir: &Path) -> Result<(), String> {
    if super::materialize::is_materialized(dir) {
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    write_if_changed(&dir.join("CMakeLists.txt"), &render_cmakelists(app))?;
    // Issue 2177 — Zephyr REQUIRES a `prj.conf` in `APPLICATION_CONFIG_DIR`,
    // which is the bringup's `boards/<board>/` when that exists and this
    // directory otherwise. A scaffolded workspace's bringup has neither, so the
    // configure stopped at `No prj.conf file(s) was found`. The real settings
    // arrive as `EXTRA_CONF_FILE` (the board package's files, then the image's
    // fragments), so this one states only what THIS directory is: a nano-ros
    // application (`CONFIG_NROS`), and, for a Rust one, a Rust application
    // (`CONFIG_RUST`, without which `rust_cargo_application()` cannot build
    // the `zephyr` crate), and, for a C++ one, its C++ standard -- see
    // [`render_prj_conf`].
    write_if_changed(&dir.join("prj.conf"), &render_prj_conf(app))?;
    // A C/C++ application has no cargo half, so no `build.rs`: that file exists
    // to bridge Kconfig into RUSTC, and there is no rustc on this road.
    if app.cmake.is_some() {
        return Ok(());
    }
    write_if_changed(&dir.join("build.rs"), &render_build_rs())
}

/// The `prj.conf` a generated application carries so Zephyr's required
/// `CONF_FILE` resolves, stating that it is a nano-ros application. Board and
/// image settings are not here: see [`write`].
const GENERATED_PRJ_CONF: &str = "# Generated by `nros build`. Zephyr requires a prj.conf here; the image's\n\
# settings come from the board package's files and the image's `os_config`\n\
# fragments, passed as EXTRA_CONF_FILE.\n\
\n\
# This application is a nano-ros image: without this the module builds\n\
# nothing, and the link fails on every zpico_* and nros_platform_* symbol.\n\
CONFIG_NROS=y\n";

/// What a generated RUST application adds: the language, which is a fact about
/// this directory (its `CMakeLists.txt` calls `rust_cargo_application()`), not
/// about the board or the image.
const GENERATED_RUST_PRJ_CONF: &str =
    "\n# This application is Rust (zephyr-lang-rust).\nCONFIG_RUST=y\n";

/// What a generated C++ application adds: C++ itself and the standard the
/// nano-ros C++ API is written against (`cxx_std_17`, the floor
/// `packages/api/nros-cpp/CMakeLists.txt` declares). Phase-548 W2: without it
/// `CONFIG_NROS_CPP_API` selects `CONFIG_CPP` and Zephyr's own default,
/// `CONFIG_STD_CPP11` on the 3.7 line, compiled the generated entry TU at
/// `-std=c++11`, where `rclcpp/qos.hpp`'s `constexpr` constructors do not
/// compile. A hand-written leaf states the same two lines in its own
/// `prj.conf` (`examples/zephyr/cpp/talker`, the `nros new` scaffold).
///
/// This file is merged FIRST, so a board or image fragment that names a newer
/// standard still wins; that is why the line lives here and not in the
/// module's rendered fragment, which is merged last.
const GENERATED_CPP_PRJ_CONF: &str = "\n# This application is C++, at the standard nros-cpp requires (cxx_std_17).\n\
CONFIG_CPP=y\n\
CONFIG_STD_CPP17=y\n";

/// The `prj.conf` [`write`] puts beside the generated `CMakeLists.txt`: the
/// nano-ros line, plus the application's own language. A C application gets
/// nothing more -- its entry TU is C.
#[must_use]
pub fn render_prj_conf(app: &WestApp) -> String {
    use nros_lang::Language;
    let Some(c) = &app.cmake else {
        return format!("{GENERATED_PRJ_CONF}{GENERATED_RUST_PRJ_CONF}");
    };
    // `CmakeApp::lang` is `c` or `cpp` (`resolve_cmake`); anything else
    // states no language, as a C application does.
    match Language::parse(&c.lang) {
        Ok(Language::Cpp) => format!("{GENERATED_PRJ_CONF}{GENERATED_CPP_PRJ_CONF}"),
        Ok(Language::C | Language::Rust) | Err(_) => GENERATED_PRJ_CONF.to_string(),
    }
}

/// Write only when the content differs — see [`super::entry`]'s note on the
/// mtime treadmill.
fn write_if_changed(path: &Path, body: &str) -> Result<(), String> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(body) {
        return Ok(());
    }
    std::fs::write(path, body).map_err(|e| format!("writing {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// phase-481 W1 -- both arms state the bringup, the image and the entry's
    /// language BEFORE `find_package(Zephyr)`, where the module hook reads
    /// them; an application with no statement renders none.
    #[test]
    fn the_image_is_stated_before_find_package_on_both_arms() {
        let td = tempfile::tempdir().unwrap();
        let entry = td.path().join("build/zephyr-zenoh/zephyr_entry");
        let bringup = td.path().join("src/demo_bringup");
        let stmt =
            |lang: &str| Some(ImageStatement::new(&entry, &bringup, "zephyr", lang).unwrap());
        let rust = WestApp {
            project: "p".to_string(),
            image: stmt("rust"),
            ..Default::default()
        };
        let mut c = cmake_app(td.path(), &[], None);
        c.image = stmt("cpp");
        for (app, lang) in [(&rust, "rust"), (&c, "cpp")] {
            let s = render_cmakelists(app);
            let at = |n: &str| s.find(n).unwrap_or_else(|| panic!("`{n}` missing:\n{s}"));
            let zephyr = at("find_package(Zephyr REQUIRED");
            assert!(
                at("\"${CMAKE_CURRENT_SOURCE_DIR}/../../../src/demo_bringup\" ABSOLUTE)") < zephyr
            );
            assert!(at("set(NROS_IMAGE zephyr)\n") < zephyr);
            assert!(at(&format!("set(NROS_IMAGE_LANGUAGE {lang})\n")) < zephyr);
        }
        let bare = render_cmakelists(&WestApp {
            project: "p".to_string(),
            ..Default::default()
        });
        assert!(!bare.contains("NROS_IMAGE"), "{bare}");
    }

    /// Issue 1702 -- both roads (the Rust `rust_cargo_application()` one and
    /// the C/C++ one) hand Kconfig every capability axis BEFORE
    /// `find_package(Zephyr)`, `y` when declared and `n` otherwise.
    #[test]
    fn the_capability_axes_reach_kconfig_before_find_package() {
        let rust = WestApp {
            project: "p".to_string(),
            capabilities: vec!["param_services".to_string()],
            ..Default::default()
        };
        let s = render_cmakelists(&rust);
        let at = |n: &str| s.find(n).unwrap_or_else(|| panic!("`{n}` missing:\n{s}"));
        assert!(
            at("set(CONFIG_NROS_CAPABILITY_PARAM_SERVICES y CACHE STRING")
                < at("find_package(Zephyr REQUIRED")
        );
        assert!(
            at("set(CONFIG_NROS_CAPABILITY_LIFECYCLE n CACHE STRING")
                < at("find_package(Zephyr REQUIRED")
        );

        let bare = render_cmakelists(&WestApp {
            project: "p".to_string(),
            ..Default::default()
        });
        assert!(
            bare.contains("set(CONFIG_NROS_CAPABILITY_PARAM_SERVICES n CACHE STRING"),
            "{bare}"
        );

        let td = tempfile::tempdir().unwrap();
        let mut c = cmake_app(td.path(), &[], None);
        c.capabilities = vec!["lifecycle".to_string()];
        let s = render_cmakelists(&c);
        let lc = s
            .find("set(CONFIG_NROS_CAPABILITY_LIFECYCLE y CACHE STRING")
            .expect(&s);
        assert!(lc < s.find("find_package(Zephyr REQUIRED").unwrap(), "{s}");
    }

    /// Issue 1707 -- both renderers stamp the file with a digest of the rest
    /// of it, in the one spelling the module's regex reads, and the digest
    /// moves with the declaration (a capability toggle).
    #[test]
    fn the_application_states_its_own_digest() {
        use sha2::{Digest, Sha256};
        // `nros_check_generated_app_current`'s regex,
        // `^set\(NROS_GENERATED_APP_DIGEST ([0-9a-f]+)\)$`, by hand.
        let digest_of = |l: &str| -> Option<String> {
            let d = l
                .strip_prefix("set(NROS_GENERATED_APP_DIGEST ")?
                .strip_suffix(')')?;
            (!d.is_empty() && d.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')))
                .then(|| d.to_string())
        };
        let td = tempfile::tempdir().unwrap();
        let mut cpp = cmake_app(td.path(), &[], None);
        cpp.capabilities = vec!["param_services".to_string()];
        let rust = WestApp {
            project: "p".to_string(),
            capabilities: vec!["param_services".to_string()],
            ..Default::default()
        };
        for app in [&rust, &cpp] {
            let s = render_cmakelists(app);
            let lines: Vec<&str> = s.lines().filter(|l| digest_of(l).is_some()).collect();
            assert_eq!(lines.len(), 1, "exactly one digest line:\n{s}");
            let stated = digest_of(lines[0]).unwrap();
            assert_eq!(
                stated,
                format!(
                    "{:x}",
                    Sha256::digest(render_cmakelists_body(app).as_bytes())
                ),
                "the digest is of the body without its own line"
            );
            // Before `find_package(Zephyr)`: the module reads the variable,
            // and the module is loaded BY find_package.
            assert!(
                s.find(lines[0]).unwrap() < s.find("find_package(Zephyr REQUIRED").unwrap(),
                "{s}"
            );
            // A declaration change moves it; an unchanged one does not, which
            // is what keeps `write_if_changed` from re-arming a configure.
            let mut off = app.clone();
            off.capabilities.clear();
            assert_ne!(render_cmakelists(&off), s);
            assert_eq!(render_cmakelists(app), s);
        }
    }

    #[test]
    fn the_cmakelists_states_no_board_and_no_conf() {
        // RFC-0065 D4: the board is `[image.<id>] board` and the Kconfig is the
        // bringup's. An application that spelled either would be the second
        // source this whole item exists to remove — `fvp_entry`'s
        // `nano_ros_use_board(fvp-aemv8r-smp)` is the measured case (issue
        // 1517).
        let app = WestApp {
            project: "nros_rust_zephyr".to_string(),
            ..Default::default()
        };
        let s = render_cmakelists(&app);
        assert!(s.contains("find_package(Zephyr"), "{s}");
        assert!(s.contains("project(nros_rust_zephyr)"), "{s}");
        assert!(s.contains("rust_cargo_application()"), "{s}");
        // The NEGATIVE half matches CODE, not prose — phase-350 W1's lesson,
        // and this test learned it the same way `fixtures-manifest.py` did:
        // the header above explains that the file "states no `prj.conf` and no
        // board", so the first version of this assertion failed on its own
        // explanation.
        let code: String = s
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!code.contains("nano_ros_use_board"), "{code}");
        assert!(!code.contains("prj.conf"), "names no fragment: {code}");
        assert!(!code.contains("EXTRA_CARGO_ARGS"), "{code}");
    }

    /// phase-461 W6 -- the `params` axis states the STORE, as a seed does,
    /// and leaves the services' symbol `n`: that is what makes nros-params
    /// reserve no service sets (`NROS_CAPABILITY_PARAM_SERVICES`).
    #[test]
    fn the_params_axis_states_the_store_and_not_the_services() {
        let s = render_cmakelists(&WestApp {
            project: "p".to_string(),
            capabilities: vec!["params".to_string()],
            ..Default::default()
        });
        assert!(
            s.contains("set(CONFIG_NROS_PARAM_STORE y CACHE STRING"),
            "{s}"
        );
        assert!(
            s.contains("set(CONFIG_NROS_CAPABILITY_PARAM_SERVICES n CACHE STRING"),
            "{s}"
        );
    }

    /// Issue 1706 -- a launch `<param>` seeds the store, so the heap default
    /// must see it on both roads even when no axis is declared; and it is
    /// written `n` otherwise, because a cache entry outlives its line.
    #[test]
    fn a_launch_param_seed_reaches_kconfig_without_the_axis() {
        let td = tempfile::tempdir().unwrap();
        let seeded = vec![PARAM_STORE_AXIS.to_string()];
        let rust = WestApp {
            project: "p".to_string(),
            capabilities: seeded.clone(),
            ..Default::default()
        };
        let mut cpp = cmake_app(td.path(), &[], None);
        cpp.capabilities = seeded;
        for s in [render_cmakelists(&rust), render_cmakelists(&cpp)] {
            let at = s
                .find("set(CONFIG_NROS_PARAM_STORE y CACHE STRING")
                .unwrap_or_else(|| panic!("seed not carried:\n{s}"));
            assert!(at < s.find("find_package(Zephyr REQUIRED").unwrap(), "{s}");
            // The SERVICES axis is untouched: a seed implies the store only.
            assert!(
                s.contains("set(CONFIG_NROS_CAPABILITY_PARAM_SERVICES n CACHE STRING"),
                "{s}"
            );
        }
        let bare = render_cmakelists(&WestApp {
            project: "p".to_string(),
            ..Default::default()
        });
        assert!(
            bare.contains("set(CONFIG_NROS_PARAM_STORE n CACHE STRING"),
            "{bare}"
        );
    }

    #[test]
    fn a_project_name_is_derived_from_the_workspace_and_the_image() {
        assert_eq!(
            project_name(Path::new("/ws/examples/workspaces/rust"), "zephyr"),
            "nros_rust_zephyr"
        );
        // A board-shaped image id carries characters cmake will not take.
        assert_eq!(
            project_name(Path::new("/ws/realtime-rust"), "zephyr.robot1"),
            "nros_realtime_rust_zephyr_robot1"
        );
    }

    #[test]
    fn a_rust_backend_becomes_a_feature_and_an_optional_dep() {
        let td = tempfile::tempdir().unwrap();
        let nros_root = td.path().join("nano-ros");
        let backend = nros_root.join("packages/rmw/zenoh/nros-rmw-zenoh");
        std::fs::create_dir_all(&backend).unwrap();
        std::fs::write(
            backend.join("Cargo.toml"),
            "[package]\nname = \"nros-rmw-zenoh\"\n\n[features]\nplatform-zephyr = []\n",
        )
        .unwrap();
        std::fs::create_dir_all(nros_root.join("packages/tooling/nros-zephyr-build")).unwrap();
        let entry = td.path().join("ws/build/zephyr-zenoh/zephyr_entry");
        std::fs::create_dir_all(&entry).unwrap();

        let app = resolve(
            &td.path().join("ws"),
            &entry,
            &nros_root,
            "zephyr",
            Some("zenoh"),
            &humble_sys(),
            "platform-zephyr",
            &std::collections::BTreeMap::new(),
        )
        .expect("resolves");

        assert_eq!(
            app.rmw_feature,
            Some(("rmw-zenoh".to_string(), Some("nros-rmw-zenoh".to_string())))
        );
        assert!(
            app.deps.iter().any(|d| d.starts_with("nros-rmw-zenoh =")
                && d.contains("optional = true")
                && d.contains("platform-zephyr")),
            "{:?}",
            app.deps
        );
    }

    /// RFC-0071 D14 — the west road resolves the image's backend through the
    /// scan: an `acme` image whose provider is a workspace package is not
    /// refused as unknown, and the SDK-only index refuses it naming the search.
    #[test]
    fn an_out_of_tree_backend_resolves_through_the_scan() {
        let td = tempfile::tempdir().unwrap();
        let ws = td.path().join("ws");
        let acme = cargo_nano_ros::rmw_resolver::test_support::acme_index(&ws);
        let nros_root = td.path().join("nano-ros");
        std::fs::create_dir_all(nros_root.join("packages/tooling/nros-zephyr-build")).unwrap();
        let entry = ws.join("build/zephyr-acme/zephyr_entry");
        std::fs::create_dir_all(&entry).unwrap();
        let none = std::collections::BTreeMap::new();
        let app = resolve_in(
            &acme,
            &ws,
            &entry,
            &nros_root,
            "zephyr",
            Some("acme"),
            &humble_sys(),
            "platform-zephyr",
            &none,
        )
        .expect("acme resolves through the scan");
        // RFC-0071 D15 — the backend's directory is the scan's, outside the
        // SDK root (the deleted `backend_dir` looked only under it).
        // RFC-0071 D9 — the edition reaches it too (issue 2393); acme declares
        // no `platform-zephyr`, so that is left out.
        assert!(
            app.deps.contains(
                &"acme-rmw = { path = \"../../../src/acme_rmw\", default-features = false, \
                  features = [\"ros-humble\"], optional = true }"
                    .to_string()
            ),
            "{:?}",
            app.deps
        );
        assert_eq!(
            app.rmw_feature,
            Some(("rmw-acme".to_string(), Some("acme-rmw".to_string())))
        );
        // ... and a `crate` the directory does not hold is refused, not
        // warned past.
        let manifest = ws.join("src/acme_rmw/Cargo.toml");
        let raw = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(
            &manifest,
            raw.replace("name = \"acme-rmw\"", "name = \"other\""),
        )
        .unwrap();
        let e = resolve_in(
            &acme,
            &ws,
            &entry,
            &nros_root,
            "zephyr",
            Some("acme"),
            &humble_sys(),
            "platform-zephyr",
            &none,
        )
        .unwrap_err();
        assert!(e.contains("is `other`"), "{e}");
        std::fs::write(&manifest, raw).unwrap();
        let e = resolve_in(
            &cargo_nano_ros::rmw_resolver::test_support::in_tree_index(),
            &ws,
            &entry,
            &nros_root,
            "zephyr",
            Some("acme"),
            &humble_sys(),
            "platform-zephyr",
            &none,
        )
        .unwrap_err();
        assert!(e.contains("`zephyr`") && e.contains("searched:"), "{e}");
    }

    /// A bringup `system.toml` for a `zephyr` image on `acme`, stating
    /// `image_extra` under `[image.zephyr]` and `system_extra` under `[system]`.
    fn sys_with(system_extra: &str, image_extra: &str) -> SystemToml {
        toml::from_str(&format!(
            "[system]\nname = \"t\"\nrmw = \"acme\"\ndomain_id = 0\n{system_extra}\n\
             [image.zephyr]\nboard = \"zephyr\"\n{image_extra}\n"
        ))
        .expect("a system.toml")
    }

    /// No edition stated anywhere: humble.
    fn humble_sys() -> SystemToml {
        sys_with("", "")
    }

    /// Issue 2393 / RFC-0071 D9 — the west road's backend dependency carries
    /// what the cargo road's does (`backend_link::dependency_features`): the
    /// image's ROS edition, the capability features it declares, and the
    /// board's platform feature, each where the crate declares it. Before, it
    /// carried the platform feature alone, so a jazzy image built `nros` for
    /// jazzy and its backend for humble.
    #[test]
    fn the_west_backend_dependency_carries_the_edition_and_capabilities() {
        let td = tempfile::tempdir().unwrap();
        let ws = td.path().join("ws");
        let acme = cargo_nano_ros::rmw_resolver::test_support::acme_index(&ws);
        // The scan's acme declares no `platform-zephyr`; a Zephyr-capable
        // backend would.
        let manifest = ws.join("src/acme_rmw/Cargo.toml");
        let raw = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(&manifest, format!("{raw}platform-zephyr = []\n")).unwrap();
        let nros_root = td.path().join("nano-ros");
        std::fs::create_dir_all(nros_root.join("packages/tooling/nros-zephyr-build")).unwrap();
        let entry = ws.join("build/zephyr-acme/zephyr_entry");
        std::fs::create_dir_all(&entry).unwrap();
        let none = std::collections::BTreeMap::new();
        let dep_for = |sys: &SystemToml| {
            let app = resolve_in(
                &acme,
                &ws,
                &entry,
                &nros_root,
                "zephyr",
                Some("acme"),
                sys,
                "platform-zephyr",
                &none,
            )
            .expect("acme resolves");
            assert_eq!(app.deps.len(), 1, "{:?}", app.deps);
            (app.deps[0].clone(), app.backend_derived_features)
        };
        let line = |feats: &str| {
            format!(
                "acme-rmw = {{ path = \"../../../src/acme_rmw\", default-features = false, \
                 features = [{feats}], optional = true }}"
            )
        };

        let (dep, derived) = dep_for(&sys_with("", "ros_edition = \"jazzy\""));
        assert_eq!(dep, line("\"ros-jazzy\", \"platform-zephyr\""));
        assert_eq!(derived, vec!["ros-jazzy".to_string()]);

        let (dep, derived) = dep_for(&humble_sys());
        assert_eq!(dep, line("\"ros-humble\", \"platform-zephyr\""));
        assert_eq!(derived, vec!["ros-humble".to_string()]);

        // The edition stated system-wide reaches the image too.
        let (dep, _) = dep_for(&sys_with("ros_edition = \"jazzy\"", ""));
        assert_eq!(dep, line("\"ros-jazzy\", \"platform-zephyr\""));

        // A declared capability the backend implements: acme's `safety` is
        // its own `acme-safety` (`[rmw.capabilities]`), not core's name.
        let (dep, derived) = dep_for(&sys_with("features = [\"safety\"]", ""));
        assert_eq!(
            dep,
            line("\"acme-safety\", \"ros-humble\", \"platform-zephyr\"")
        );
        assert_eq!(
            derived,
            vec!["acme-safety".to_string(), "ros-humble".to_string()]
        );
    }

    #[test]
    fn a_cmake_backend_gets_no_feature_and_no_dep() {
        // cyclonedds names no cargo `crate` — it is a C++ library the Zephyr
        // C port links, so the macro's `#[cfg(feature = "rmw-cyclonedds")]`
        // register is correctly absent rather than missing.
        let td = tempfile::tempdir().unwrap();
        let nros_root = td.path().join("nano-ros");
        std::fs::create_dir_all(nros_root.join("packages/tooling/nros-zephyr-build")).unwrap();
        let entry = td.path().join("ws/build/zephyr-cyclonedds/zephyr_entry");
        std::fs::create_dir_all(&entry).unwrap();

        let app = resolve(
            &td.path().join("ws"),
            &entry,
            &nros_root,
            "zephyr",
            Some("cyclonedds"),
            &humble_sys(),
            "platform-zephyr",
            &std::collections::BTreeMap::new(),
        )
        .expect("resolves");

        // The feature is declared with no dep (an undeclared one fails
        // `-D unexpected-cfgs`); no crate is pulled in.
        assert_eq!(app.rmw_feature, Some(("rmw-cyclonedds".to_string(), None)));
        assert!(
            !app.deps.iter().any(|d| d.contains("nros-rmw-cyclonedds")),
            "{:?}",
            app.deps
        );
    }

    /// phase-531 W6 -- a backend with a per-message hook and no rlib (Cyclone)
    /// marks the application as needing type support. A Rust backend does not:
    /// its messages register through the crate.
    #[test]
    fn only_a_per_message_backend_without_an_rlib_needs_type_support() {
        let td = tempfile::tempdir().unwrap();
        let nros_root = td.path().join("nano-ros");
        std::fs::create_dir_all(nros_root.join("packages/tooling/nros-zephyr-build")).unwrap();
        let entry = td.path().join("ws/build/zephyr/zephyr_entry");
        std::fs::create_dir_all(&entry).unwrap();
        let app_for = |rmw: &str| {
            resolve(
                &td.path().join("ws"),
                &entry,
                &nros_root,
                "zephyr",
                Some(rmw),
                &humble_sys(),
                "platform-zephyr",
                &std::collections::BTreeMap::new(),
            )
            .expect("resolves")
        };
        assert!(app_for("cyclonedds").needs_message_typesupport);
        assert!(!app_for("zenoh").needs_message_typesupport);
        assert!(!app_for("xrce").needs_message_typesupport);
    }

    /// phase-531 W6 -- a Rust application on a Cyclone image names its
    /// interface closure after `project()` (the call links into `app`, which
    /// `find_package(Zephyr)` creates) and before `rust_cargo_application()`.
    /// Without the call, every topic's `find_descriptor()` fails at run time.
    #[test]
    fn a_rust_cyclone_application_names_its_interface_closure() {
        let with = render_cmakelists(&WestApp {
            project: "p".into(),
            typesupport_packages: vec![
                "builtin_interfaces".into(),
                "std_msgs".into(),
                "my_msgs=../../../src/my_msgs".into(),
            ],
            ..Default::default()
        });
        let call = with
            .find("nros_zephyr_interface_typesupport(PACKAGES\n    builtin_interfaces\n    std_msgs\n    my_msgs=../../../src/my_msgs)")
            .unwrap_or_else(|| panic!("no type-support call in:\n{with}"));
        let project = with.find("project(p)").unwrap();
        let rust = with.rfind("rust_cargo_application()").unwrap();
        assert!(project < call && call < rust, "{with}");

        let without = render_cmakelists(&WestApp {
            project: "p".into(),
            ..Default::default()
        });
        assert!(
            !without.contains("nros_zephyr_interface_typesupport"),
            "{without}"
        );
    }

    #[test]
    fn the_build_rs_bridges_both_kconfig_paths() {
        let s = render_build_rs();
        assert!(
            s.contains("zephyr_build::export_kconfig_bool_options()"),
            "{s}"
        );
        assert!(s.contains("nros_zephyr_build::bake_nros_config()"), "{s}");
    }

    // ---- the C/C++ arm (phase-470 W5.b3, issue 1288) ---------------------

    /// A workspace whose launch names `<pkgs>`; each entry is
    /// `(name, holds a C++ source, is a cargo crate)`.
    fn cmake_workspace(root: &Path, pkgs: &[(&str, bool, bool)]) -> Vec<(String, PathBuf)> {
        let mut out = Vec::new();
        for (name, cpp, cargo) in pkgs {
            let dir = root.join("src").join(name);
            std::fs::create_dir_all(dir.join("src")).unwrap();
            std::fs::write(dir.join("CMakeLists.txt"), "").unwrap();
            if *cpp {
                std::fs::write(dir.join("src").join("Node.cpp"), "").unwrap();
            } else {
                std::fs::write(dir.join("src").join("node.c"), "").unwrap();
            }
            if *cargo {
                std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"n\"\n").unwrap();
            }
            out.push(((*name).to_string(), dir));
        }
        out
    }

    fn cmake_app(root: &Path, nodes: &[(String, PathBuf)], panic: Option<&str>) -> WestApp {
        let entry = root.join("build/zephyr-zenoh/zephyr_entry");
        std::fs::create_dir_all(&entry).unwrap();
        resolve_cmake(
            root,
            &entry,
            &root.join("src/demo_bringup"),
            "zephyr",
            None,
            &std::collections::BTreeMap::new(),
            panic,
            "zephyr",
            nodes,
        )
        .expect("resolves")
    }

    /// A Rust node package with no CMakeLists is registered from its bringup
    /// row and bundled into the runtime, never added as a subdir; one that
    /// ships its own CMakeLists keeps it.
    #[test]
    fn a_cargo_only_node_is_registered_from_its_row() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        let mut nodes = cmake_workspace(root, &[("c_talker_pkg", false, false)]);
        let beat = root.join("src/rust_beat_pkg");
        let own = root.join("src/rust_own_pkg");
        for (dir, cmake) in [(&beat, false), (&own, true)] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("Cargo.toml"), "[package]\n").unwrap();
            std::fs::write(dir.join("package.xml"), "<package/>").unwrap();
            if cmake {
                std::fs::write(dir.join("CMakeLists.txt"), "").unwrap();
            }
        }
        nodes.push(("rust_beat_pkg".into(), beat));
        nodes.push(("rust_own_pkg".into(), own));
        let bringup = root.join("src/demo_bringup");
        std::fs::create_dir_all(&bringup).unwrap();
        std::fs::write(
            bringup.join("system.toml"),
            "[system]\nname = \"d\"\n\n\
             [[component]]\npkg = \"rust_beat_pkg\"\nclass = \"rust_beat_pkg::Beat\"\nname = \"beat\"\n\n\
             [[component]]\npkg = \"rust_own_pkg\"\nclass = \"rust_own_pkg::Own\"\nname = \"own\"\n",
        )
        .unwrap();
        let app = cmake_app(root, &nodes, None);
        let c = app.cmake.as_ref().unwrap();
        assert_eq!(
            c.cargo_nodes,
            [
                "rust_beat_pkg \"${CMAKE_CURRENT_SOURCE_DIR}/../../../src/rust_beat_pkg\" beat \
              rust_beat_pkg::Beat"
            ]
        );
        assert_eq!(
            c.rust_node_dirs,
            ["../../../src/rust_beat_pkg", "../../../src/rust_own_pkg"]
        );
        let names: Vec<&str> = c.subdirs.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(names, ["c_talker_pkg", "rust_own_pkg"]);
        let body = render_cmakelists(&app);
        let reg = body
            .find("nros_register_cargo_node(rust_beat_pkg ")
            .expect("the derived registration is emitted");
        let entry = body.find("\nnano_ros_add_executable(").unwrap();
        assert!(reg < entry, "registered before the entry plans: {body}");
        assert!(!body.contains("rust_own_pkg::Own"), "explicit wins: {body}");
    }

    /// `LANG` is the workspace's own answer, and the scan reaches `<pkg>/src/`.
    ///
    /// A top-level-only scan called the pure-C++ `cpp` workspace `c`, which is
    /// why the shared `discover::holds_cpp_source` looks one level down. One
    /// C++ node is enough: the generated carrier TU is C++ and has to compile
    /// against what it links.
    #[test]
    fn lang_is_cpp_when_any_node_holds_a_cpp_source() {
        let td = tempfile::tempdir().unwrap();
        let c_only = cmake_workspace(td.path(), &[("talker_pkg", false, false)]);
        let c = cmake_app(td.path(), &c_only, None);
        assert_eq!(c.cmake.as_ref().unwrap().lang, "c");

        let td2 = tempfile::tempdir().unwrap();
        let mixed = cmake_workspace(
            td2.path(),
            &[
                ("c_talker_pkg", false, false),
                ("cpp_listener_pkg", true, false),
            ],
        );
        let m = cmake_app(td2.path(), &mixed, None);
        assert_eq!(m.cmake.as_ref().unwrap().lang, "cpp");
    }

    /// Issue 2644 -- a C node and a Rust node, no C++ source: `cpp`. The C
    /// entry pack refuses a Rust component by name, so a `c` here refused the
    /// island-shaped workspace's Zephyr image at configure.
    #[test]
    fn lang_is_cpp_when_a_rust_node_joins_c_nodes() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        let mut nodes = cmake_workspace(root, &[("c_controller_pkg", false, false)]);
        let rust = root.join("src/rust_monitor_pkg");
        std::fs::create_dir_all(&rust).unwrap();
        std::fs::write(rust.join("Cargo.toml"), "[package]\n").unwrap();
        std::fs::write(rust.join("package.xml"), "<package/>").unwrap();
        nodes.push(("rust_monitor_pkg".into(), rust));
        let bringup = root.join("src/demo_bringup");
        std::fs::create_dir_all(&bringup).unwrap();
        std::fs::write(
            bringup.join("system.toml"),
            "[system]\nname = \"d\"\n\n\
             [[component]]\npkg = \"rust_monitor_pkg\"\nclass = \"rust_monitor_pkg::Monitor\"\nname = \"monitor\"\n",
        )
        .unwrap();
        let app = cmake_app(root, &nodes, None);
        assert_eq!(app.cmake.as_ref().unwrap().lang, "cpp");
    }

    /// Phase-548 W2 -- a C++ application's `prj.conf` states C++ and the
    /// standard nros-cpp declares as its floor; a C or Rust one states no C++
    /// at all. The number is read back from the floor's one home, so moving the
    /// floor without moving this file fails here.
    #[test]
    fn the_prj_conf_states_the_cpp_standard_only_for_a_cpp_image() {
        let floor_file =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../api/nros-cpp/CMakeLists.txt");
        let cmake = std::fs::read_to_string(&floor_file).unwrap();
        let floor = cmake
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .find_map(|l| l.split("INTERFACE cxx_std_").nth(1))
            .and_then(|r| r.trim_end_matches(')').trim().parse::<u32>().ok())
            .expect("nros-cpp declares its cxx_std floor");

        let td = tempfile::tempdir().unwrap();
        let nodes = cmake_workspace(td.path(), &[("talker_pkg", true, false)]);
        let cpp = render_prj_conf(&cmake_app(td.path(), &nodes, None));
        let lines: Vec<&str> = cpp.lines().collect();
        assert!(lines.contains(&"CONFIG_NROS=y"), "{cpp}");
        assert!(lines.contains(&"CONFIG_CPP=y"), "{cpp}");
        assert!(
            lines.contains(&format!("CONFIG_STD_CPP{floor}=y").as_str()),
            "{cpp}"
        );
        assert!(!lines.contains(&"CONFIG_RUST=y"), "{cpp}");

        let td2 = tempfile::tempdir().unwrap();
        let c_nodes = cmake_workspace(td2.path(), &[("talker_pkg", false, false)]);
        let c = render_prj_conf(&cmake_app(td2.path(), &c_nodes, None));
        assert_eq!(c, GENERATED_PRJ_CONF, "a C image's prj.conf is unchanged");

        let rust = render_prj_conf(&WestApp::default());
        assert!(!rust.contains("CONFIG_CPP"), "{rust}");
        assert!(rust.lines().any(|l| l == "CONFIG_RUST=y"), "{rust}");

        // `write` puts exactly this text on disk.
        let out = td.path().join("app");
        write(&cmake_app(td.path(), &nodes, None), &out).unwrap();
        assert_eq!(std::fs::read_to_string(out.join("prj.conf")).unwrap(), cpp);
    }

    /// `PANIC` is emitted only when the image declares one, and `LANG c` only
    /// when the workspace is C — the two axes four of the eight hand-written
    /// applications differed by.
    #[test]
    fn panic_reaches_the_call_only_when_the_image_declares_it() {
        let td = tempfile::tempdir().unwrap();
        let nodes = cmake_workspace(td.path(), &[("ctrl_pkg", false, false)]);
        let plain = render_cmakelists(&cmake_app(td.path(), &nodes, None));
        assert!(plain.contains("LANG    c\n"), "{plain}");
        assert!(!plain.contains("PANIC"), "{plain}");

        let with = render_cmakelists(&cmake_app(td.path(), &nodes, Some("platform")));
        assert!(with.contains("PANIC   platform"), "{with}");
    }

    /// Issue 1681 -- the bringup's capability axes reach the Zephyr module,
    /// BEFORE `find_package(Zephyr)`, which is where it reads them.
    ///
    /// The generated entry calls `nros_cpp_register_parameter_services` for a
    /// bringup that declares `param_services`, and the symbol exists only in an
    /// nros-cpp built with that feature -- which `zephyr/CMakeLists.txt` adds
    /// only when `param_services IN_LIST NROS_FEATURES`. This road never set
    /// the variable, so the image failed to link. Both spellings of the axis
    /// count; a bringup that declares none gets no line at all.
    #[test]
    fn the_capability_axes_are_set_before_find_package_zephyr() {
        let td = tempfile::tempdir().unwrap();
        let nodes = cmake_workspace(td.path(), &[("srv_pkg", true, false)]);
        let bringup = td.path().join("src/demo_bringup");
        std::fs::create_dir_all(&bringup).unwrap();
        let base = "[system]\nname = \"t\"\nrmw = \"zenoh\"\ndomain_id = 0\n";
        let code = |app: &WestApp| -> String {
            render_cmakelists(app)
                .lines()
                .filter(|l| !l.trim_start().starts_with('#'))
                .collect::<Vec<_>>()
                .join("\n")
        };

        std::fs::write(bringup.join("system.toml"), base).unwrap();
        let none = code(&cmake_app(td.path(), &nodes, None));
        assert!(!none.contains("NROS_FEATURES"), "{none}");

        std::fs::write(
            bringup.join("system.toml"),
            base.replace(
                "domain_id = 0\n",
                "domain_id = 0\nfeatures = [\"param_services\", \"lifecycle\"]\n",
            ),
        )
        .unwrap();
        let app = cmake_app(td.path(), &nodes, None);
        let with = code(&app);
        let at = |needle: &str| {
            with.find(needle)
                .unwrap_or_else(|| panic!("missing {needle}:\n{with}"))
        };
        assert!(
            with.contains("set(NROS_FEATURES \"param_services;lifecycle\")"),
            "a cmake LIST, which `IN_LIST` reads: {with}"
        );
        assert!(at("set(NROS_FEATURES") < at("find_package(Zephyr"));

        // The deprecated typed block is the same axis (`[safety]`; its
        // `[param_services]` twin is refused since phase-486 W8).
        std::fs::write(
            bringup.join("system.toml"),
            format!("{base}\n[safety]\nenabled = true\n"),
        )
        .unwrap();
        let typed = code(&cmake_app(td.path(), &nodes, None));
        assert!(typed.contains("set(NROS_FEATURES \"safety\")"), "{typed}");
    }

    /// phase-556 W13 -- the node-interface switch reaches the Zephyr road
    /// (`NROS_FEATURES` and the Kconfig capability symbols) for THIS image:
    /// `stock` adds the parameter services and `/rosout`, `none` on the image
    /// removes what `[system]` declared, and no switch changes nothing.
    #[test]
    fn the_node_interface_switch_reaches_nros_features_and_kconfig() {
        let td = tempfile::tempdir().unwrap();
        let nodes = cmake_workspace(td.path(), &[("srv_pkg", true, false)]);
        let bringup = td.path().join("src/demo_bringup");
        std::fs::create_dir_all(&bringup).unwrap();
        let base = "[system]\nname = \"t\"\nrmw = \"zenoh\"\ndomain_id = 0\n";
        let write = |extra: &str| {
            std::fs::write(bringup.join("system.toml"), format!("{base}{extra}")).unwrap();
            render_cmakelists(&cmake_app(td.path(), &nodes, None))
        };
        let off = write("");
        assert!(!off.contains("set(NROS_FEATURES"), "{off}");
        assert!(
            off.contains("set(CONFIG_NROS_CAPABILITY_ROSOUT n "),
            "every axis is written, `n` when undeclared: {off}"
        );
        let stock = write("node_interfaces = \"stock\"\n");
        assert!(
            stock.contains("set(NROS_FEATURES \"param_services;rosout;sim_time\")"),
            "{stock}"
        );
        assert!(stock.contains("set(CONFIG_NROS_CAPABILITY_PARAM_SERVICES y "));
        assert!(stock.contains("set(CONFIG_NROS_CAPABILITY_ROSOUT y "));
        // phase-556 W11 -- `use_sim_time` is in the stock set.
        assert!(stock.contains("set(CONFIG_NROS_CAPABILITY_SIM_TIME y "));
        assert!(off.contains("set(CONFIG_NROS_CAPABILITY_SIM_TIME n "));
        let none = write(
            "features = [\"param_services\", \"lifecycle\"]\n\
             [image.zephyr]\nboard = \"x\"\nnode_interfaces = \"none\"\n",
        );
        assert!(none.contains("set(NROS_FEATURES \"lifecycle\")"), "{none}");
        assert!(none.contains("set(CONFIG_NROS_CAPABILITY_PARAM_SERVICES n "));
    }

    /// The two lines `mixed` needs, in the order that makes them work.
    ///
    /// `NROS_WS_RUST_NODE_DIRS` must precede `find_package(Zephyr)` — the
    /// nano-ros Zephyr module reads it DURING find_package to decide whether to
    /// build the `nros_ws_runtime` umbrella instead of plain nros-cpp. The
    /// `nano_ros_workspace_pkg_guard` stub is a SECOND line with a second
    /// reason and must come AFTER `find_package(nano_ros)` and before the
    /// `add_subdirectory` calls, so the Rust node package's own guard finds it
    /// already defined and does not bootstrap a second nano-ros import.
    /// Comments cannot hold an ordering; this can.
    #[test]
    fn a_rust_node_orders_the_bundle_var_and_the_guard_stub() {
        let td = tempfile::tempdir().unwrap();
        let nodes = cmake_workspace(
            td.path(),
            &[
                ("c_talker_pkg", false, false),
                ("cpp_listener_pkg", true, false),
                ("rust_heartbeat_pkg", false, true),
            ],
        );
        let app = cmake_app(td.path(), &nodes, None);
        assert_eq!(
            app.cmake.as_ref().unwrap().rust_node_dirs,
            vec!["../../../src/rust_heartbeat_pkg".to_string()],
            "only the cargo crate"
        );

        // Over CODE, not prose — the sibling test above learned this the same
        // way, and so did this one: the bundle variable's own comment SAYS
        // "MUST be set before `find_package(Zephyr)`" two lines above the
        // `set(`, so a whole-file search found the ordering claim ahead of the
        // thing it is a claim about, and the assertion failed on the
        // explanation rather than on the file.
        let code: String = render_cmakelists(&app)
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        let at = |needle: &str| {
            code.find(needle)
                .unwrap_or_else(|| panic!("missing {needle}:\n{code}"))
        };
        assert!(at("set(NROS_WS_RUST_NODE_DIRS") < at("find_package(Zephyr"));
        assert!(
            at("find_package(nano_ros REQUIRED)") < at("function(nano_ros_workspace_pkg_guard)")
        );
        assert!(at("function(nano_ros_workspace_pkg_guard)") < at("add_subdirectory("));
    }

    /// A workspace with no cargo node emits neither line — an empty
    /// `NROS_WS_RUST_NODE_DIRS` would switch the runtime umbrella on for an
    /// image that has no Rust in it.
    #[test]
    fn a_pure_cmake_workspace_emits_no_rust_bundle_lines() {
        let td = tempfile::tempdir().unwrap();
        let nodes = cmake_workspace(td.path(), &[("talker_pkg", true, false)]);
        let s = render_cmakelists(&cmake_app(td.path(), &nodes, None));
        assert!(!s.contains("NROS_WS_RUST_NODE_DIRS"), "{s}");
        assert!(!s.contains("nano_ros_workspace_pkg_guard"), "{s}");
    }

    /// A C/C++ application has no cargo half, so `write` emits no `build.rs`.
    ///
    /// That file exists to bridge Kconfig into rustc, and there is no rustc on
    /// this road. Writing one would put a manifest-less `build.rs` beside a
    /// `CMakeLists.txt`, which reads as a cargo root that is not there.
    #[test]
    fn a_cmake_application_gets_no_build_rs() {
        let td = tempfile::tempdir().unwrap();
        let nodes = cmake_workspace(td.path(), &[("talker_pkg", false, false)]);
        let app = cmake_app(td.path(), &nodes, None);
        let dir = td.path().join("build/zephyr-zenoh/zephyr_entry");
        write(&app, &dir).expect("writes");
        assert!(dir.join("CMakeLists.txt").is_file());
        assert!(
            !dir.join("build.rs").exists(),
            "a C/C++ application has no cargo half"
        );
    }

    /// The subdir list keeps the closure's BUILD order and is RELATIVE. The
    /// order is the caller's (`package_closure`, deterministic), so the file
    /// is still byte-identical across machines (phase-383 W3.c); re-sorting it
    /// by name put a dependent's `add_subdirectory` before its dependency's
    /// (issue 2756). Given in non-name order, it comes back unchanged.
    #[test]
    fn the_subdir_list_keeps_the_closure_order_and_is_relative() {
        let td = tempfile::tempdir().unwrap();
        let nodes = cmake_workspace(
            td.path(),
            &[("talker_pkg", false, false), ("listener_pkg", false, false)],
        );
        let app = cmake_app(td.path(), &nodes, None);
        let subdirs = &app.cmake.as_ref().unwrap().subdirs;
        assert_eq!(
            subdirs,
            &[
                (
                    "../../../src/talker_pkg".to_string(),
                    "talker_pkg".to_string()
                ),
                (
                    "../../../src/listener_pkg".to_string(),
                    "listener_pkg".to_string()
                ),
            ]
        );
        let body = render_cmakelists(&app);
        let t = body.find("src/talker_pkg").expect("talker added");
        let l = body.find("src/listener_pkg").expect("listener added");
        assert!(t < l, "emitted in the given order: {body}");
        let s = render_cmakelists(&app);
        assert!(
            !s.contains(td.path().to_str().unwrap()),
            "no absolute path: {s}"
        );
    }

    /// Issue 2594 (phase-478 I11) -- a package of the image that carries
    /// `zephyr/module.yml` joins `ZEPHYR_EXTRA_MODULES` before
    /// `find_package(Zephyr)`, which is where Zephyr parses its Kconfig.
    #[test]
    fn a_zephyr_module_package_joins_the_extra_modules_before_find_package() {
        let td = tempfile::tempdir().unwrap();
        let nodes = cmake_workspace(
            td.path(),
            &[("talker_pkg", false, false), ("support_pkg", false, false)],
        );
        let module = td.path().join("src/support_pkg/zephyr");
        std::fs::create_dir_all(&module).unwrap();
        std::fs::write(
            module.join("module.yml"),
            "name: support_pkg\nbuild:\n  kconfig: zephyr/Kconfig\n",
        )
        .unwrap();
        let app = cmake_app(td.path(), &nodes, None);
        let c = app.cmake.as_ref().unwrap();
        assert_eq!(c.zephyr_modules, ["../../../src/support_pkg"]);
        // Kconfig only: the package's own CMakeLists is still the app's.
        assert!(c.subdirs.iter().any(|(_, n)| n == "support_pkg"), "{c:?}");
        let s = render_cmakelists(&app);
        let at = |needle: &str| s.find(needle).unwrap_or_else(|| panic!("{needle}: {s}"));
        // Issue 2609 -- added to the CACHE entry `nros build` passes with
        // `-DZEPHYR_EXTRA_MODULES=<nano-ros>`, keeping what it held: Zephyr
        // reads the cache before a normal variable, so a `list(APPEND)` on
        // the variable alone was never read.
        assert!(
            at("set(_nros_zephyr_modules ${ZEPHYR_EXTRA_MODULES})")
                < at("list(APPEND _nros_zephyr_modules")
                && at("list(APPEND _nros_zephyr_modules")
                    < at("set(ZEPHYR_EXTRA_MODULES \"${_nros_zephyr_modules}\" CACHE STRING")
                && at("CACHE STRING") < at("find_package(Zephyr REQUIRED"),
            "{s}"
        );
        assert!(s.contains("FORCE)"), "{s}");
        assert!(!s.contains("list(APPEND ZEPHYR_EXTRA_MODULES"), "{s}");
        assert!(s.contains("\"${CMAKE_CURRENT_SOURCE_DIR}/../../../src/support_pkg\" ABSOLUTE"));
        assert!(
            !s.contains(td.path().to_str().unwrap()),
            "no absolute path: {s}"
        );
    }

    /// A module whose CMake IS the package's own CMakeLists (`build.cmake: .`)
    /// is built by Zephyr, so the application must not add it a second time.
    /// A module with its CMake elsewhere (`build.cmake: zephyr`) leaves the
    /// package's CMakeLists to the application.
    #[test]
    fn a_module_that_builds_the_package_cmake_is_not_added_twice() {
        let td = tempfile::tempdir().unwrap();
        let nodes = cmake_workspace(
            td.path(),
            &[("own_pkg", false, false), ("split_pkg", false, false)],
        );
        for (pkg, cmake) in [("own_pkg", "."), ("split_pkg", "zephyr")] {
            let module = td.path().join("src").join(pkg).join("zephyr");
            std::fs::create_dir_all(&module).unwrap();
            std::fs::write(
                module.join("module.yml"),
                format!("build:\n  cmake: {cmake}\n  kconfig: zephyr/Kconfig\n"),
            )
            .unwrap();
        }
        let app = cmake_app(td.path(), &nodes, None);
        let c = app.cmake.as_ref().unwrap();
        assert_eq!(
            c.zephyr_modules,
            ["../../../src/own_pkg", "../../../src/split_pkg"]
        );
        let names: Vec<&str> = c.subdirs.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(names, ["split_pkg"]);
    }

    /// A module-only package (Kconfig, no CMakeLists) is a module and no
    /// subdirectory; a `module.yml` that does not parse is refused by name.
    #[test]
    fn a_kconfig_only_module_is_no_subdirectory_and_a_bad_module_yml_is_refused() {
        let td = tempfile::tempdir().unwrap();
        let mut nodes = cmake_workspace(td.path(), &[("talker_pkg", false, false)]);
        let kconf = td.path().join("src/kconfig_pkg");
        std::fs::create_dir_all(kconf.join("zephyr")).unwrap();
        std::fs::write(
            kconf.join("zephyr/module.yml"),
            "build:\n  kconfig: Kconfig\n",
        )
        .unwrap();
        nodes.push(("kconfig_pkg".to_string(), kconf.clone()));
        let app = cmake_app(td.path(), &nodes, None);
        let c = app.cmake.as_ref().unwrap();
        assert_eq!(c.zephyr_modules, ["../../../src/kconfig_pkg"]);
        assert!(!c.subdirs.iter().any(|(_, n)| n == "kconfig_pkg"), "{c:?}");

        std::fs::write(kconf.join("zephyr/module.yml"), "build: [unclosed\n").unwrap();
        let entry = td.path().join("build/zephyr-zenoh/zephyr_entry");
        let err = resolve_cmake(
            td.path(),
            &entry,
            &td.path().join("src/demo_bringup"),
            "zephyr",
            None,
            &std::collections::BTreeMap::new(),
            None,
            "zephyr",
            &nodes,
        )
        .unwrap_err();
        assert!(err.contains("module.yml"), "{err}");
    }

    /// No module in the closure, no module lines.
    #[test]
    fn an_image_without_modules_emits_no_extra_modules() {
        let td = tempfile::tempdir().unwrap();
        let nodes = cmake_workspace(td.path(), &[("talker_pkg", false, false)]);
        let app = cmake_app(td.path(), &nodes, None);
        assert!(!render_cmakelists(&app).contains("ZEPHYR_EXTRA_MODULES"));
    }

    /// Issue 2610 -- the generated app puts the workspace's `src/` on
    /// `NROS_INTERFACE_SEARCH_PATH` before `find_package(Zephyr)`, as the
    /// CMake root does, so a workspace message package a node depends on is
    /// found with no environment. Relative, so the file is portable.
    #[test]
    fn the_workspace_interface_packages_are_on_the_search_path() {
        let td = tempfile::tempdir().unwrap();
        let nodes = cmake_workspace(td.path(), &[("talker_pkg", false, false)]);
        let app = cmake_app(td.path(), &nodes, None);
        assert_eq!(app.cmake.as_ref().unwrap().interface_src, "../../../src");
        let s = render_cmakelists(&app);
        let at = |needle: &str| s.find(needle).unwrap_or_else(|| panic!("{needle}: {s}"));
        assert!(
            at("get_filename_component(_nros_ws_src \"../../../src\" ABSOLUTE)")
                < at("set(NROS_INTERFACE_SEARCH_PATH")
                && at("set(NROS_INTERFACE_SEARCH_PATH") < at("find_package(Zephyr REQUIRED"),
            "{s}"
        );
        assert!(
            s.contains("$ENV{NROS_INTERFACE_SEARCH_PATH}"),
            "an exported entry is kept: {s}"
        );
        assert!(
            !s.contains(td.path().to_str().unwrap()),
            "no absolute path: {s}"
        );
    }
}
