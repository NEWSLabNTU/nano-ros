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
         find_package(Zephyr REQUIRED HINTS $ENV{{ZEPHYR_BASE}})\n\
         project({})\n\n\
         rust_cargo_application()\n",
        app.project
    )
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
    write_if_changed(&dir.join("CMakeLists.txt"), &render_cmakelists(app))?;
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
}
