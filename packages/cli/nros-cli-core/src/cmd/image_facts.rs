//! `nros image-facts <bringup>:<image>` — the resolved image, as data.
//!
//! THE DUPLICATION THIS EXISTS TO REMOVE
//!
//! `zephyr/cmake/nros_cargo_build.cmake` hand-assembles a cargo invocation:
//! package, manifest, target dir, features, profile, target triple, a rustup
//! toolchain override and `-Z build-std`. Every one of those is a fact
//! `nros build` already computes from `[image.*]`. So one thing has two
//! derivations, and only one of them knows about images — which is RFC-0085
//! D2's complaint in one sentence: *"a Zephyr build and an `nros build` of the
//! same image are two different derivations of the same thing"*.
//!
//! WHY A QUERY AND NOT A BUILD VERB
//!
//! D2 sketched a "supplier" that west's configure would invoke, and worried it
//! must not be `nros build` or west would loop: `nros build` runs `west build`,
//! which would run `nros build`. **A query cannot loop.** This runs stages 1–4
//! — discover, resolve, preflight — and stops before the handoff, which is
//! exactly `plan_builds`, already exercised by `--dry-run` and already reused
//! by `nros materialize`.
//!
//! It also follows an idiom this repository already has four of — `nros
//! profile`, `nros model-path`, `nros sdk-path`, `nros codegen resolve-deps` —
//! described in `NanoRosCargoProfile.cmake` as *"the bridge cmake/bash use so
//! the derivations are not re-spelled per language"*. This is the fifth, and
//! the one that makes an IMAGE reach a west build.
//!
//! WHAT IT DELIBERATELY DOES NOT DO
//!
//! It produces no artifacts. Generated message crates and the resolved
//! SystemModel already have a producer (`nros sync`); the entry staticlib and
//! the per-build sizes headers already have one (cargo, driven by cmake). What
//! was missing was never a builder — it was the ANSWERS those builders were
//! guessing at.
//!
//! AND IT MUST DEGRADE, NOT FAIL
//!
//! A plain Zephyr application using nano-ros as a module — the book's original
//! flow — is not in a nano-ros workspace at all. For it there is no image and
//! no facts, and that is not an error: `--if-present` exits 0 having printed
//! nothing, so cmake keeps its existing Kconfig derivation. Any other behaviour
//! would break the promise that `west build -b <board> <app>` just works.

use std::path::PathBuf;

use clap::Args as ClapArgs;
use eyre::{Result, bail};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Image to resolve — `zephyr`, or `<bringup>:<image>` when two bringups
    /// declare the same id.
    pub image: Option<String>,

    /// Workspace root. Defaults to the current directory.
    ///
    /// cmake passes the APPLICATION directory, which is a package inside the
    /// workspace rather than its root, so the root is walked up to from here.
    #[arg(long)]
    pub workspace: Option<PathBuf>,

    /// nano-ros checkout. Defaults to `NROS_REPO_DIR`, then an autodetect walk.
    #[arg(long)]
    pub nano_ros_path: Option<PathBuf>,

    /// Emit `set(NROS_IMAGE_* …)` lines for `include()`/`cmake -P`.
    #[arg(long)]
    pub cmake: bool,

    /// Print nothing and exit 0 when this directory is not in a nano-ros
    /// workspace, or declares no such image.
    ///
    /// For the caller that must work BOTH ways — a west build of an app that
    /// may or may not belong to a workspace.
    #[arg(long)]
    pub if_present: bool,
}

/// Walk up to the workspace root from anywhere inside it.
///
/// A nano-ros workspace is recognised by holding a bringup — a package
/// carrying `system.toml`. That is the definition `nros build` already uses
/// (an image is declared in a bringup), so this cannot disagree with it.
///
/// Not `Cargo.toml`: a C or C++ workspace has none, and an entry package HAS
/// one, so walking to the nearest manifest finds the package rather than the
/// workspace.
pub fn workspace_root_from(start: &std::path::Path) -> Option<PathBuf> {
    // Absolute first. `Path::new(".").parent()` is `Some("")` and then `None`,
    // so a relative start ends the walk after one step — and `.` is exactly
    // what a caller passes when it means "here", which cmake does.
    let abs = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    let mut cur = Some(abs.as_path());
    while let Some(dir) = cur {
        let src = dir.join("src");
        if src.is_dir()
            && let Ok(entries) = std::fs::read_dir(&src)
            && entries
                .flatten()
                .any(|e| e.path().join("system.toml").is_file())
        {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}

pub fn run(args: Args) -> Result<()> {
    let start = match &args.workspace {
        Some(w) => w.clone(),
        None => std::env::current_dir()?,
    };

    let Some(root) = workspace_root_from(&start) else {
        if args.if_present {
            return Ok(());
        }
        bail!(
            "{} is not inside a nano-ros workspace (no package carrying \
             `system.toml` above it).\n  \
             A plain Zephyr application need not be in one — pass --if-present \
             to make that a no-op rather than an error.",
            start.display()
        );
    };

    let build_args = crate::cmd::build::Args {
        images: args.image.clone().into_iter().collect(),
        workspace: Some(root.clone()),
        nano_ros_path: args.nano_ros_path.clone(),
        // A query resolves; it never runs west, so no Zephyr is looked for.
        zephyr_workspace: None,
        all: false,
        dry_run: true,
        offline: true,
        native_args: Vec::new(),
    };

    let plans = match crate::cmd::build::plan_builds(&build_args) {
        Ok(p) => p,
        Err(e) if args.if_present => {
            // The workspace exists but does not declare this image. Same
            // reasoning as above: the caller asked whether facts are available,
            // and "no" is an answer.
            let _ = e;
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let Some(plan) = plans.first() else {
        if args.if_present {
            return Ok(());
        }
        bail!("no image resolved in {}", root.display());
    };

    if args.cmake {
        print!("{}", as_cmake(plan, &root));
    } else {
        print!("{}", as_plain(plan, &root));
    }
    Ok(())
}

/// `set()` lines, quoted so a path with a space survives.
fn as_cmake(plan: &crate::cmd::build::ResolvedBuild, root: &std::path::Path) -> String {
    let mut out = String::new();
    out.push_str("# generated by `nros image-facts --cmake` — do not edit\n");
    let mut put = |k: &str, v: &str| {
        out.push_str(&format!("set(NROS_IMAGE_{k} \"{v}\")\n"));
    };
    put("QUALIFIED", &plan.qualified);
    put("BOARD", &plan.board);
    put("PLATFORM", &plan.platform);
    put("WORKSPACE", &root.display().to_string());
    put("DRIVER", &format!("{:?}", plan.driver).to_lowercase());
    // The four a west build currently re-derives. RMW is the one that can
    // actually disagree — cmake reads `CONFIG_NROS_RMW_*`, the image says
    // `rmw`, and nothing made them agree.
    if let Some(v) = &plan.rmw {
        put("RMW", v);
    }
    if let Some(v) = &plan.entry_package {
        put("ENTRY_PACKAGE", v);
    }
    if let Some(v) = &plan.target {
        put("TARGET", v);
    }
    if let Some(v) = &plan.profile {
        put("PROFILE", v);
    }
    out
}

fn as_plain(plan: &crate::cmd::build::ResolvedBuild, root: &std::path::Path) -> String {
    let mut out = format!(
        "qualified={}\nboard={}\nplatform={}\nworkspace={}\ndriver={}\n",
        plan.qualified,
        plan.board,
        plan.platform,
        root.display(),
        format!("{:?}", plan.driver).to_lowercase(),
    );
    for (k, v) in [
        ("rmw", &plan.rmw),
        ("entry_package", &plan.entry_package),
        ("target", &plan.target),
        ("profile", &plan.profile),
    ] {
        if let Some(v) = v {
            out.push_str(&format!("{k}={v}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The walk finds the workspace from a package inside it — which is the
    /// only thing cmake can pass, since a west application knows its own
    /// directory and nothing above it.
    #[test]
    fn the_workspace_is_found_from_a_package_inside_it() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("my_robot");
        std::fs::create_dir_all(ws.join("src/demo_bringup")).unwrap();
        std::fs::write(ws.join("src/demo_bringup/system.toml"), "[system]\n").unwrap();
        let entry = ws.join("src/zephyr_entry");
        std::fs::create_dir_all(&entry).unwrap();

        assert_eq!(workspace_root_from(&entry).as_deref(), Some(ws.as_path()));
    }

    /// A bringup is what makes a directory a workspace — NOT a `Cargo.toml`.
    ///
    /// An entry package has a `Cargo.toml`, so a walk to the nearest manifest
    /// would stop at the package and call it the workspace. And a C or C++
    /// workspace has no manifest at all, so it would never terminate anywhere
    /// right.
    #[test]
    fn a_cargo_manifest_alone_is_not_a_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg = tmp.path().join("just_a_crate");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();

        assert_eq!(workspace_root_from(&pkg), None);
    }

    /// Outside any workspace the answer is None, so `--if-present` can turn it
    /// into a clean no-op for the plain-Zephyr-app case.
    #[test]
    fn nothing_above_a_bare_directory() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(workspace_root_from(tmp.path()), None);
    }

    /// A RELATIVE start walks too.
    ///
    /// `Path::new(".").parent()` is `Some("")` and then `None`, so the walk
    /// ended after one step — and `.` is exactly what a caller passes when it
    /// means "here". Measured: `nros image-facts --workspace .` from an entry
    /// package printed nothing at all.
    #[test]
    fn a_relative_start_still_finds_the_root() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("my_robot");
        std::fs::create_dir_all(ws.join("src/demo_bringup")).unwrap();
        std::fs::write(ws.join("src/demo_bringup/system.toml"), "[system]\n").unwrap();
        let entry = ws.join("src/zephyr_entry");
        std::fs::create_dir_all(&entry).unwrap();

        let prev = std::env::current_dir().unwrap();
        // `set_current_dir` is process-global; this test is the only one that
        // uses it and restores it immediately.
        std::env::set_current_dir(&entry).unwrap();
        let got = workspace_root_from(std::path::Path::new("."));
        std::env::set_current_dir(prev).unwrap();

        assert_eq!(
            got.map(|p| p.canonicalize().unwrap()),
            Some(ws.canonicalize().unwrap())
        );
    }
}
