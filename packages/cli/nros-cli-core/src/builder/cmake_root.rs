//! Stage 4b — emit the cmake workspace root (phase-383 W4, RFC-0065 D3).
//!
//! ## What this replaces
//!
//! `examples/workspaces/c/CMakeLists.txt` — ~70 lines doing four jobs, none of
//! them user intent:
//!
//! 1. map the board to a `CMAKE_TOOLCHAIN_FILE`, **before `project()`**,
//!    because that is the first compiler probe;
//! 2. list the packages by hand;
//! 3. filter which entries belong to the active platform, by hand;
//! 4. promote `NUTTX_DIR` out of a cmake directory scope.
//!
//! All four are derivable. There are nine such roots in-tree and each can drift
//! independently; a workspace that gains a package but forgets the `SUBDIRS`
//! line simply does not build it, silently, because an absent subdir is not an
//! error.
//!
//! ## Unlike the cargo root, this DOES live under `build/`
//!
//! Cargo pins its workspace manifest to the workspace root (a package belongs
//! to one workspace, found by walking up; members must sit below the root — see
//! [`super::cargo_root`]). CMake has neither rule: `add_subdirectory` takes an
//! arbitrary source dir, so the generated root sits at
//! `build/<coord>/CMakeLists.txt` where RFC-0065 D8 wants it, and switching
//! board or RMW selects a different coordinate instead of thrashing one tree.
//!
//! One consequence worth stating: `add_subdirectory(<src> <bin>)` needs the
//! second argument when the source is outside the tree, and every subdir here
//! is. That is why the emitted calls carry a binary dir.
//!
//! ## What it does NOT derive
//!
//! **The preamble** (W4.c). `autoware-safety-island`'s root calls
//! `find_package(Eigen3 REQUIRED)`, which nothing in the tree implies. An
//! optional `<bringup>/cmake/preamble.cmake` is included before `project()` if
//! present.
//!
//! **Whether a package builds.** ASI adds `src/s32z2_board_glue` only when the
//! NXP SDK is provisioned — "the pkg's own CMakeLists gates and reports". The
//! emitter lists it; the package decides. A package that excludes itself is
//! normal, not an error (W4.d).

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use super::discover::Discovered;

/// Everything the emitted root needs that is not in [`Discovered`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmakeRootSpec {
    /// Workspace root, so subdir paths can be made relative to the manifest.
    pub workspace: PathBuf,
    /// Bringup package name — `nano_ros_workspace(SYSTEM …)`.
    pub system: String,
    /// nano-ros platform token (`posix`, `freertos`, `nuttx`, …).
    pub platform: String,
    /// nano-ros board id, or `None` for a host build that names none.
    pub board: Option<String>,
    /// RMW backend.
    pub rmw: String,
    /// Repo-relative toolchain file from the board descriptor's `[board.cmake]`,
    /// or `None` for a host board that needs none.
    pub toolchain_file: Option<String>,
    /// nano-ros checkout, for resolving the toolchain file.
    pub nano_ros_root: PathBuf,
    /// Package dirs to omit — west/idf entries, and entries for other boards.
    pub excluded: BTreeSet<PathBuf>,
}

/// Render the root `CMakeLists.txt` written to `manifest_dir`.
pub fn render(
    discovered: &Discovered,
    manifest_dir: &Path,
    spec: &CmakeRootSpec,
) -> Result<String, String> {
    // A cmake subdir must carry a CMakeLists. A pure-Rust package in a mixed
    // workspace does not, and reaches the image through corrosion from a
    // package that does — listing it here would be a configure error.
    let mut subdirs: Vec<(String, String)> = Vec::new();
    for pkg in &discovered.packages {
        if spec.excluded.contains(&pkg.dir) || !pkg.dir.join("CMakeLists.txt").is_file() {
            continue;
        }
        // Relative to the WORKSPACE, not to this file.
        //
        // `nano_ros_workspace` hands SUBDIRS to `nros ws order --workspace
        // <root> --subdir <s>`, which resolves each against the workspace and
        // walks it for `package.xml`. A hand-written root sits AT the workspace
        // root, so the two bases coincide and nothing distinguished them; this
        // root sits in `build/<coord>/`, and manifest-relative paths made the
        // ordering tool look inside the build directory ("no package.xml under
        // .../build/posix-zenoh"). Workspace-relative is also what a reader
        // wants to see: `src/talker_pkg`, not `../../src/talker_pkg`.
        let rel = super::paths::relative_or_err(&spec.workspace, &pkg.dir)?;
        subdirs.push((rel, pkg.name.clone()));
    }
    if subdirs.is_empty() {
        return Err(
            "no cmake packages in this workspace — nothing for a cmake root to \
             add_subdirectory. A pure-Rust workspace builds through the cargo \
             root instead (phase-383 W3)."
                .to_string(),
        );
    }
    // Sorted for byte-identical output (W3.c). ORDER_FROM_DEPENDS re-derives
    // the BUILD order from each package's `<depend>` tags, so the order written
    // here carries no meaning and must not churn.
    subdirs.sort();

    let mut out = String::new();
    out.push_str(
        "# GENERATED by `nros build` (phase-383 W4) — DO NOT EDIT.\n\
         #\n\
         # Regenerated on every build. Edit the workspace, not this file.\n\
         #\n\
         # Paths are RELATIVE and the subdir list is SORTED, so this file is\n\
         # byte-identical across machines (phase-383 W3.c). Build ORDER comes\n\
         # from ORDER_FROM_DEPENDS, not from the order written here.\n\n",
    );
    out.push_str("cmake_minimum_required(VERSION 3.22)\n\n");

    // The toolchain file must precede project(): that call runs the first
    // compiler probe, and a toolchain set afterwards is a toolchain nobody used.
    if let Some(tc) = &spec.toolchain_file {
        let abs = spec.nano_ros_root.join(tc);
        let rel = super::paths::relative(manifest_dir, &abs)
            .ok_or_else(|| format!("cannot express toolchain file {} relatively", abs.display()))?;
        out.push_str(&format!(
            "# Before project(): that call is the first compiler probe, so a\n\
             # toolchain file set after it is one nobody used.\n\
             if(NOT CMAKE_TOOLCHAIN_FILE)\n    \
             set(CMAKE_TOOLCHAIN_FILE \"${{CMAKE_CURRENT_LIST_DIR}}/{rel}\")\nendif()\n\n"
        ));
    }

    // W4.c — the user preamble, if the bringup ships one.
    out.push_str(
        "# W4.c — optional user preamble (`<bringup>/cmake/preamble.cmake`).\n\
         # For what the builder cannot derive: `find_package(Eigen3 REQUIRED)`\n\
         # in autoware-safety-island's root is the motivating case.\n\
         if(DEFINED NROS_WS_PREAMBLE AND EXISTS \"${NROS_WS_PREAMBLE}\")\n    \
         include(\"${NROS_WS_PREAMBLE}\")\nendif()\n\n",
    );

    out.push_str(&format!(
        "project({}_nros_workspace LANGUAGES C CXX)\n\n",
        sanitize(&spec.system)
    ));
    out.push_str("find_package(nano_ros REQUIRED COMPONENTS workspace)\n\n");

    out.push_str(&format!(
        "set(NANO_ROS_PLATFORM {} CACHE STRING \"\" FORCE)\n",
        spec.platform
    ));
    if let Some(board) = &spec.board {
        out.push_str(&format!(
            "set(NANO_ROS_BOARD {board} CACHE STRING \"\" FORCE)\n"
        ));
    }
    out.push('\n');

    // Relative like every other path here, so the file stays byte-identical
    // across machines (W3.c).
    let ws_rel = super::paths::relative_or_err(manifest_dir, &spec.workspace)?;
    out.push_str("nano_ros_workspace(\n");
    // WHERE THE WORKSPACE IS, which is not where this file is.
    //
    // A hand-written root lives at the workspace root, so `nano_ros_workspace`
    // could read `CMAKE_SOURCE_DIR` for both. This one sits in `build/<coord>/`
    // (RFC-0065 D3/D8), and without this the bringup lookup searches the build
    // directory: "no bringup pkg named 'demo_bringup' in .../build/posix-zenoh".
    out.push_str(&format!("    WORKSPACE_ROOT \"{ws_rel}\"\n"));
    out.push_str(&format!("    BACKEND  {}\n", spec.rmw));
    out.push_str(&format!("    PLATFORM \"{}\"\n", spec.platform));
    out.push_str(&format!("    SYSTEM   {}\n", spec.system));
    out.push_str("    ORDER_FROM_DEPENDS\n");
    out.push_str("    SUBDIRS\n");
    for (rel, name) in &subdirs {
        out.push_str(&format!("        \"{rel}\"   # {name}\n"));
    }
    out.push_str(")\n");
    Ok(out)
}

/// A cmake `project()` name: letters, digits and underscores only.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

/// Write the root, creating `manifest_dir` if needed.
pub fn write(
    discovered: &Discovered,
    manifest_dir: &Path,
    spec: &CmakeRootSpec,
) -> Result<PathBuf, String> {
    let body = render(discovered, manifest_dir, spec)?;
    std::fs::create_dir_all(manifest_dir)
        .map_err(|e| format!("creating {}: {e}", manifest_dir.display()))?;
    let path = manifest_dir.join("CMakeLists.txt");
    // Rewrite only on change: cmake re-configures when its root is newer than
    // the cache, and a gratuitous touch costs a full reconfigure.
    if std::fs::read_to_string(&path).ok().as_deref() != Some(body.as_str()) {
        std::fs::write(&path, &body).map_err(|e| format!("writing {}: {e}", path.display()))?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cargo_nano_ros::provider_scan::WorkspacePackage;

    fn pkg(root: &Path, name: &str, cmake: bool) -> WorkspacePackage {
        let dir = root.join("src").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        if cmake {
            std::fs::write(dir.join("CMakeLists.txt"), "# pkg\n").unwrap();
        }
        WorkspacePackage {
            name: name.to_string(),
            dir,
            depends: Default::default(),
        }
    }

    fn discovered(packages: Vec<WorkspacePackage>) -> Discovered {
        Discovered {
            packages,
            cargo_only: Default::default(),
            warnings: Vec::new(),
        }
    }

    fn spec(root: &Path) -> CmakeRootSpec {
        CmakeRootSpec {
            workspace: root.to_path_buf(),
            system: "demo_bringup".to_string(),
            platform: "posix".to_string(),
            board: None,
            rmw: "zenoh".to_string(),
            toolchain_file: None,
            nano_ros_root: PathBuf::from("/nros"),
            excluded: Default::default(),
        }
    }

    #[test]
    fn subdirs_are_relative_and_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let d = discovered(vec![pkg(root, "zzz_pkg", true), pkg(root, "aaa_pkg", true)]);
        let body = render(&d, &root.join("build/posix"), &spec(root)).expect("renders");
        // Workspace-relative, not manifest-relative — see the note at the
        // computation. A hand-written root cannot tell the difference; this one
        // can, and got it wrong.
        assert!(body.contains("\"src/aaa_pkg\""), "{body}");
        assert!(
            !body.contains("\"../../src/aaa_pkg\""),
            "subdirs must not be relative to the generated file: {body}"
        );
        assert!(
            !body.contains(root.to_str().unwrap()),
            "no absolute path (W3.c): {body}"
        );
        let a = body.find("aaa_pkg").unwrap();
        let z = body.find("zzz_pkg").unwrap();
        assert!(a < z, "sorted: {body}");
    }

    #[test]
    fn the_toolchain_file_precedes_project() {
        // project() runs the first compiler probe, so a toolchain set after it
        // is a toolchain nobody used — the bug every hand-written root guards
        // against with a comment.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let d = discovered(vec![pkg(root, "a_pkg", true)]);
        let mut s = spec(root);
        s.toolchain_file = Some("cmake/toolchain/arm-freertos-armcm3.cmake".to_string());
        s.board = Some("mps2-an385-freertos".to_string());
        s.platform = "freertos".to_string();
        let body = render(&d, &root.join("build/freertos"), &s).expect("renders");
        // Compare LINE positions among non-comment lines: the explanatory
        // comment above the toolchain block legitimately contains the text
        // "project(", and a naive substring search matches that first.
        let code: Vec<&str> = body
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect();
        let tc = code
            .iter()
            .position(|l| l.contains("CMAKE_TOOLCHAIN_FILE"))
            .expect("toolchain emitted");
        let proj = code
            .iter()
            .position(|l| l.starts_with("project("))
            .expect("project emitted");
        assert!(tc < proj, "toolchain must precede project(): {body}");
    }

    #[test]
    fn a_host_board_emits_no_toolchain_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let d = discovered(vec![pkg(root, "a_pkg", true)]);
        let body = render(&d, &root.join("build/posix"), &spec(root)).expect("renders");
        assert!(!body.contains("CMAKE_TOOLCHAIN_FILE"), "{body}");
    }

    #[test]
    fn a_rust_only_package_is_not_a_subdir() {
        // It has no CMakeLists; listing it would be a configure error. It
        // reaches the image through corrosion from a package that does.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let d = discovered(vec![pkg(root, "c_pkg", true), pkg(root, "rust_pkg", false)]);
        let body = render(&d, &root.join("build/posix"), &spec(root)).expect("renders");
        assert!(body.contains("c_pkg"), "{body}");
        assert!(!body.contains("rust_pkg"), "{body}");
    }

    #[test]
    fn an_excluded_entry_is_omitted() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let zephyr = pkg(root, "zephyr_entry", true);
        let d = discovered(vec![pkg(root, "native_entry", true), zephyr.clone()]);
        let mut s = spec(root);
        s.excluded = [zephyr.dir.clone()].into_iter().collect();
        let body = render(&d, &root.join("build/posix"), &s).expect("renders");
        assert!(body.contains("native_entry"), "{body}");
        assert!(!body.contains("zephyr_entry"), "{body}");
    }

    #[test]
    fn the_root_orders_from_depends_not_from_the_written_order() {
        // The SET is chosen here; the ORDER is derived. phase-348 W4 made that
        // possible, and it means this file's ordering carries no meaning.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let d = discovered(vec![pkg(root, "a_pkg", true)]);
        let body = render(&d, &root.join("build/posix"), &spec(root)).expect("renders");
        assert!(body.contains("ORDER_FROM_DEPENDS"), "{body}");
    }

    #[test]
    fn a_workspace_with_no_cmake_packages_points_at_the_cargo_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let d = discovered(vec![pkg(root, "rust_pkg", false)]);
        let e = render(&d, &root.join("build/posix"), &spec(root)).expect_err("nothing to add");
        assert!(e.contains("W3"), "{e}");
    }

    #[test]
    fn output_is_byte_identical_across_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let d = discovered(vec![pkg(root, "a_pkg", true), pkg(root, "b_pkg", true)]);
        let dir = root.join("build/posix");
        assert_eq!(
            render(&d, &dir, &spec(root)).unwrap(),
            render(&d, &dir, &spec(root)).unwrap()
        );
    }

    #[test]
    fn writing_twice_does_not_touch_the_file() {
        // cmake reconfigures when its root outdates the cache; a gratuitous
        // touch costs a full reconfigure.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let d = discovered(vec![pkg(root, "a_pkg", true)]);
        let dir = root.join("build/posix");
        let p = write(&d, &dir, &spec(root)).expect("first");
        let m1 = std::fs::metadata(&p).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&d, &dir, &spec(root)).expect("second");
        let m2 = std::fs::metadata(&p).unwrap().modified().unwrap();
        assert_eq!(m1, m2);
    }
}
