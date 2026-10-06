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
//! Which crate that is comes from `[rmw.link] rlib_dep` in the backend's own
//! `nros-rmw.toml` — the table that already exists to answer "is this backend a
//! Rust crate?", and which answers `""` for cyclonedds and uorb (C/C++ CMake
//! projects the Zephyr C port links). An empty `rlib_dep` therefore emits no
//! feature and no dep, which is exactly right: the macro's `#[cfg]` is then
//! correctly off and the backend arrives through cmake.

use std::path::{Path, PathBuf};

use super::paths::relative_or_err;

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
    /// defaults to, and the optional dep it enables. `None` when the image's
    /// RMW is not a Rust crate.
    pub rmw_feature: Option<(String, String)>,
    /// `[dependencies]` lines this road adds (the backend, the `zephyr` crate).
    pub deps: Vec<String>,
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
    /// beside them. Only Kconfig reads this list; `NANO_ROS_FEATURES` is
    /// derived from the bringup separately, so the pseudo-axis never reaches a
    /// cargo feature.
    pub capabilities: Vec<String>,
}

/// Issue 1702 -- the capability axes a Kconfig default depends on, and the
/// symbol each one sets.
pub const CAPABILITY_KCONFIG: &[(&str, &str)] = &[
    ("param_services", "CONFIG_NROS_CAPABILITY_PARAM_SERVICES"),
    ("lifecycle", "CONFIG_NROS_CAPABILITY_LIFECYCLE"),
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
        "# The declared capability axes as Kconfig (issue 1702): the nros heap's\n\
         # default follows them. Set before `find_package(Zephyr)`, which is\n\
         # where Kconfig runs.\n",
    );
    for (axis, sym) in CAPABILITY_KCONFIG {
        let on = capabilities.iter().any(|c| c == axis);
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
/// | `nano_ros_use_board(...)` + `EXTRA_CONF_FILE` | NOT derived — see [`resolve_cmake`] |
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
    /// `extern int main(void)`. The hand-written files all say `zephyr` and
    /// `zephyr_cyclonedds_entry`'s header records the measurement.
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
    /// `NANO_ROS_FEATURES` -- the bringup's capability axes (`[system]
    /// features` and the deprecated typed blocks), set BEFORE
    /// `find_package(Zephyr)` for the same reason as `rust_node_dirs`: the
    /// nano-ros Zephyr module reads it DURING find_package, to add
    /// `param-services` to the nros-cpp cargo build and to define
    /// `NROS_SYSTEM_PARAM_SERVICES`. Issue 1681 -- nothing on this road set it,
    /// so an image whose generated entry calls
    /// `nros_cpp_register_parameter_services` linked an nros-cpp built without
    /// it.
    pub features: Vec<String>,
}

/// Where a backend crate lives, given its name.
///
/// `packages/rmw/<family>/<crate>` — LOCATED rather than assumed, because the
/// family directory is not the crate name (`nros-rmw-xrce-cffi` lives under
/// `xrce/`). Same search `cmd::build::bridge_entry_deps` makes, for the same
/// reason.
fn backend_dir(nros_root: &Path, krate: &str) -> Option<PathBuf> {
    ["zenoh", "cyclonedds", "xrce", "uorb", "dds"]
        .iter()
        .map(|fam| nros_root.join("packages/rmw").join(fam).join(krate))
        .find(|p| p.join("Cargo.toml").is_file())
}

/// Does the crate at `dir` declare `feature`?
///
/// Named before it is emitted, because a missing feature is a build the user can
/// fix and a bogus one fails cargo resolution outright.
///
/// ONE spelling (phase-470 W5.b2): this was a byte-identical copy of
/// [`crate::orchestration::facade::crate_declares_feature`], and W5.b2 wanted a
/// third for the derived board features. Three copies of a four-line predicate
/// is how a rule ends up true in two places and not the third, so the two that
/// existed were collapsed onto the original rather than joined by a sibling.
fn declares_feature(dir: &Path, feature: &str) -> bool {
    crate::orchestration::facade::crate_declares_feature(dir, feature)
}

/// A cmake `project()` identifier for this image.
///
/// The hand-written applications spell it by hand
/// (`nros_zephyr_workspace_entry_rs`, `nros_zephyr_ws_qos_entry_rs`, …), which
/// is a name nobody reads and no two of them agree on. Derived from the two
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

/// Derive the west-application facts for one image.
///
/// `rmw` is the image's effective backend (`[image.<id>] rmw`, already folded
/// with `[image_defaults]` and the system header by the caller).
pub fn resolve(
    workspace: &Path,
    entry_dir: &Path,
    nros_root: &Path,
    image_id: &str,
    rmw: Option<&str>,
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
    // `register()` reachable, and an empty `rlib_dep` is a real answer.
    if let Some(declared) = rmw {
        let resolved = cargo_nano_ros::rmw_resolver::resolve_rmw(declared)
            .map_err(|e| format!("`{image_id}`: {e}"))?;
        if let Some(krate) = resolved.dispatch.rlib_dep {
            match backend_dir(nros_root, krate) {
                Some(dir) => {
                    let rel = relative_or_err(entry_dir, &dir)?;
                    let feats = if declares_feature(&dir, platform_feature) {
                        format!(", features = [\"{platform_feature}\"]")
                    } else {
                        String::new()
                    };
                    app.deps.push(format!(
                        "{krate} = {{ path = \"{rel}\", default-features = false{feats}, \
                         optional = true }}"
                    ));
                    app.rmw_feature = Some((resolved.cargo_feature.to_string(), krate.to_string()));
                }
                // The table names a crate this checkout does not have. Not
                // fatal: the image still builds its C side, and the missing
                // `register()` fails loudly at run time with the backend
                // named, which is more use than refusing a build here over a
                // path.
                None => {
                    eprintln!(
                        "nros build: warning: `{image_id}` declares rmw `{declared}`, whose \
                         `[rmw.link] rlib_dep = \"{krate}\"` names no crate under \
                         packages/rmw/ — the entry will link no Rust backend"
                    );
                }
            }
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

/// Is this node package a cargo crate?
///
/// The `NROS_WS_RUST_NODE_DIRS` question, asked of the package rather than of a
/// registry: a Rust node in a cmake workspace carries BOTH a `Cargo.toml` and a
/// `CMakeLists.txt` (the latter registering it with `LANGUAGE RUST`), so the
/// manifest is the discriminator and the build file is not.
fn is_cargo_node(dir: &Path) -> bool {
    dir.join("Cargo.toml").is_file()
}

/// Derive the C/C++ west-application facts for one image.
///
/// `nodes` is the launch file's node packages, in the order the emitter should
/// `add_subdirectory` them — which is to say SORTED, so the file is
/// byte-identical across machines (W3.c) and the build order comes from each
/// package's own `<depend>` tags rather than from this list.
///
/// **What this deliberately does NOT derive: `fvp_entry`'s
/// `nano_ros_use_board(fvp-aemv8r-smp)` + `EXTRA_CONF_FILE` preamble.** That
/// application states its own board, which is the state this whole item exists
/// to end — `[image.fvp] board` says it now (issue 1517) and `nros build`
/// passes it as `-b`. Reproducing the preamble would re-create the second
/// source rather than remove it.
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
    let mut lang = "c";
    for (name, dir) in nodes {
        subdirs.push((relative_or_err(entry_dir, dir)?, name.clone()));
        if is_cargo_node(dir) {
            rust_node_dirs.push(relative_or_err(entry_dir, dir)?);
        }
        if super::discover::holds_cpp_source(dir) {
            lang = "cpp";
        }
    }
    subdirs.sort();
    rust_node_dirs.sort();

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
            features: crate::cmd::build::declared_capabilities(bringup_dir)
                .into_iter()
                .map(str::to_string)
                .collect(),
        }),
        capabilities: crate::cmd::build::declared_capabilities(bringup_dir)
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
/// and one of those runs INSIDE the configure that reads it:
/// `nros_check_image_agreement` calls `nros image-facts --for-entry`, which
/// plans the whole workspace. So when the declaration changed after the last
/// `nros build`, the configure reads the old file, the query rewrites it, and
/// the build system is written AFTER the rewrite -- newer than its input, so no
/// later `ninja` reconfigures. The image keeps the previous declaration's
/// Kconfig and `NANO_ROS_FEATURES` with nothing saying so.
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
    let anchor = "cmake_minimum_required(VERSION 3.20.0)\n";
    let line = format!(
        "\n# This file's own fingerprint (issue 1707). The nano-ros Zephyr module\n\
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
        return render_cmake_app(&app.project, c, &app.capabilities);
    }
    format!(
        "# GENERATED by `nros build` (phase-470 W5.a) — DO NOT EDIT.\n\
         #\n\
         # The west application for one `[image.*]`. Its Rust half is the\n\
         # `Cargo.toml` + `src/lib.rs` beside this file; zephyr-lang-rust's\n\
         # `rust_cargo_application()` builds them from THIS directory, which\n\
         # is why the two share one.\n\
         #\n\
         # The image's Kconfig is NOT here: `nros build` passes it as\n\
         # `-DAPPLICATION_CONFIG_DIR` + `-DEXTRA_CONF_FILE` from the bringup\n\
         # (RFC-0065 D4), so this file states no `prj.conf` and no board.\n\
         #\n\
         # `nros materialize <image>` takes ownership of this directory.\n\n\
         cmake_minimum_required(VERSION 3.20.0)\n\n\
         {}\
         find_package(Zephyr REQUIRED HINTS $ENV{{ZEPHYR_BASE}})\n\
         project({})\n\n\
         rust_cargo_application()\n",
        capability_kconfig_cmake(&app.capabilities),
        app.project
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
///   checkout with `NANO_ROS_PLATFORM=zephyr`, which `nros-c` rejects.
///   (W5.b3's brief said this stub goes before `find_package`; measured against
///   `examples/workspaces/mixed/src/zephyr_entry`, it does not — only
///   `NROS_WS_RUST_NODE_DIRS` does, and the two are separate lines with
///   separate reasons.)
#[must_use]
fn render_cmake_app(project: &str, c: &CmakeApp, capabilities: &[String]) -> String {
    let mut out = String::new();
    out.push_str(
        "# GENERATED by `nros build` (phase-470 W5.b3) — DO NOT EDIT.\n\
         #\n\
         # The west application for one C/C++ `[image.*]`. It has no Rust half:\n\
         # `nano_ros_add_executable(... BRINGUP ...)` generates the entry TU\n\
         # that carries `main`, which is why no `SOURCES` appear below.\n\
         #\n\
         # The image's Kconfig is NOT here: `nros build` passes it as\n\
         # `-DAPPLICATION_CONFIG_DIR` + `-DEXTRA_CONF_FILE` from the bringup\n\
         # (RFC-0065 D4), so this file states no `prj.conf` and no board.\n\
         #\n\
         # Paths are RELATIVE and the subdir list is SORTED, so this file is\n\
         # byte-identical across machines (phase-383 W3.c).\n\
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
            "# The bringup's capability axes. MUST be set before\n\
             # `find_package(Zephyr)`: the nano-ros Zephyr module reads it DURING\n\
             # find_package to pick the nros-cpp cargo features (`param-services`)\n\
             # and the `NROS_SYSTEM_*` defines the generated entry relies on\n\
             # (issue 1681).\n",
        );
        out.push_str(&format!(
            "set(NANO_ROS_FEATURES \"{}\")\n\n",
            c.features.join(";")
        ));
    }
    out.push_str(&capability_kconfig_cmake(capabilities));
    out.push_str(&format!(
        "find_package(Zephyr REQUIRED HINTS $ENV{{ZEPHYR_BASE}})\nproject({project})\n\n"
    ));
    out.push_str(
        "# RFC-0048 ament shape (287-W6): on Zephyr `find_package(nano_ros)`\n\
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
             # checkout with `NANO_ROS_PLATFORM=zephyr`, which `nros-c` rejects.\n\
             if(NOT COMMAND nano_ros_workspace_pkg_guard)\n    \
             function(nano_ros_workspace_pkg_guard)\n    \
             endfunction()\nendif()\n\n",
        );
    }
    out.push_str(
        "# The node packages the launch file names. Their registration carries\n\
         # no DEPLOY, so they stay component-only; the entry's auto-link sidecar\n\
         # pulls them into `app`.\n",
    );
    for (dir, name) in &c.subdirs {
        out.push_str(&format!(
            "add_subdirectory(\"${{CMAKE_CURRENT_SOURCE_DIR}}/{dir}\" {name})\n"
        ));
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
    "// GENERATED by `nros build` (phase-470 W5.a) — DO NOT EDIT.\n\
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
    // A C/C++ application has no cargo half, so no `build.rs`: that file exists
    // to bridge Kconfig into RUSTC, and there is no rustc on this road.
    if app.cmake.is_some() {
        return Ok(());
    }
    write_if_changed(&dir.join("build.rs"), &render_build_rs())
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
            "platform-zephyr",
            &std::collections::BTreeMap::new(),
        )
        .expect("resolves");

        assert_eq!(
            app.rmw_feature,
            Some(("rmw-zenoh".to_string(), "nros-rmw-zenoh".to_string()))
        );
        assert!(
            app.deps.iter().any(|d| d.starts_with("nros-rmw-zenoh =")
                && d.contains("optional = true")
                && d.contains("platform-zephyr")),
            "{:?}",
            app.deps
        );
    }

    #[test]
    fn a_cmake_backend_gets_no_feature_and_no_dep() {
        // cyclonedds declares `rlib_dep = ""` — it is a C++ library the Zephyr
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
            "platform-zephyr",
            &std::collections::BTreeMap::new(),
        )
        .expect("resolves");

        assert_eq!(app.rmw_feature, None);
        assert!(
            !app.deps.iter().any(|d| d.contains("nros-rmw-cyclonedds")),
            "{:?}",
            app.deps
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
    /// only when `param_services IN_LIST NANO_ROS_FEATURES`. This road never set
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
        assert!(!none.contains("NANO_ROS_FEATURES"), "{none}");

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
            with.contains("set(NANO_ROS_FEATURES \"param_services;lifecycle\")"),
            "a cmake LIST, which `IN_LIST` reads: {with}"
        );
        assert!(at("set(NANO_ROS_FEATURES") < at("find_package(Zephyr"));

        // The deprecated typed block is the same axis.
        std::fs::write(
            bringup.join("system.toml"),
            format!("{base}\n[param_services]\nenabled = true\n"),
        )
        .unwrap();
        let typed = code(&cmake_app(td.path(), &nodes, None));
        assert!(
            typed.contains("set(NANO_ROS_FEATURES \"param_services\")"),
            "{typed}"
        );
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

    /// The subdir list is SORTED and RELATIVE, so the file is byte-identical
    /// across machines (phase-383 W3.c). Build ORDER comes from each package's
    /// own `<depend>` tags, never from this list.
    #[test]
    fn the_subdir_list_is_sorted_and_relative() {
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
                    "../../../src/listener_pkg".to_string(),
                    "listener_pkg".to_string()
                ),
                (
                    "../../../src/talker_pkg".to_string(),
                    "talker_pkg".to_string()
                ),
            ]
        );
        let s = render_cmakelists(&app);
        assert!(
            !s.contains(td.path().to_str().unwrap()),
            "no absolute path: {s}"
        );
    }
}
