//! `nros build` — the workspace build entry point (RFC-0065, phase-383 W2.c).
//!
//! Five stages, and the last one replaces this process:
//!
//! ```text
//!   1. DISCOVER   package.xml ∪ cargo members → topological order
//!   2. RESOLVE    the image: argument > default_images > list and fail
//!   3. PREFLIGHT  toolchains / SDKs / sources present?
//!   4. GENERATE   msg bindings + model + the ROOT BUILD FILE  (W3/W4)
//!   5. EXEC       cargo / cmake / west / idf.py — stderr untouched
//! ```
//!
//! Stage 4 is not wired yet. That is deliberate and shippable: RFC-0065 D3 says
//! a root is emitted only where one would otherwise be hand-written, and west
//! and ESP-IDF apps ship their own, so those targets go 1→2→3→5 today and work.
//! A cargo/cmake image reports what stage 4 will do and stops, rather than
//! silently building the wrong thing.

use std::path::PathBuf;

use clap::Parser;
use eyre::{Result, WrapErr};

use crate::builder::{
    discover,
    handoff::Handoff,
    plan::{self, Driver},
};

#[derive(Parser, Debug)]
pub struct Args {
    /// Image(s) to build — `native`, or `<bringup>:native` when two bringups
    /// declare the same id. Empty uses `[system] default_images`.
    pub images: Vec<String>,

    /// Workspace root. Defaults to the current directory.
    #[arg(long)]
    pub workspace: Option<PathBuf>,

    /// nano-ros checkout holding `packages/boards`. Defaults to
    /// `NROS_REPO_DIR`, then an autodetect walk from the workspace.
    #[arg(long)]
    pub nano_ros_path: Option<PathBuf>,

    /// Build every declared image.
    #[arg(long)]
    pub all: bool,

    /// Print the stages and the command that would run, then stop.
    ///
    /// Safe by construction: a `Handoff` performs no I/O until `exec`.
    #[arg(long)]
    pub dry_run: bool,

    /// Do not fetch anything; fail naming what is missing (RFC-0065 D14).
    ///
    /// Note this is a SCOPED guarantee: stages 1–4 touch no network, and stage
    /// 5 gets the native tool's own offline spelling. It cannot promise an
    /// arbitrary user `CMakeLists.txt` refrains from fetching.
    #[arg(long)]
    pub offline: bool,

    /// Arguments after `--` go to the native tool verbatim.
    #[arg(last = true)]
    pub native_args: Vec<String>,
}

/// One resolved build: what to say, and what to run.
#[derive(Debug, Clone)]
pub struct ResolvedBuild {
    /// `<bringup>:<image>`.
    pub qualified: String,
    /// nano-ros board id as authored.
    pub board: String,
    /// The board's platform token, resolved through the board catalog.
    pub platform: String,
    pub driver: Driver,
    /// The native command. `None` when stage 4 must run first and is not
    /// implemented for this driver yet.
    pub handoff: Option<Handoff>,
}

/// Stages 1-4: everything up to the handoff, with NO side effects.
///
/// Separated from [`run`] so the composition is testable without a built
/// binary and without exec'ing anything. That separation is also why
/// `--dry-run` is trivially correct rather than a second code path.
pub fn plan_builds(args: &Args) -> Result<Vec<ResolvedBuild>> {
    let root = match &args.workspace {
        Some(w) => w.clone(),
        None => std::env::current_dir().wrap_err("resolving cwd as the workspace root")?,
    };

    // ---- stage 1 --------------------------------------------------------
    let members = discover::cargo_members_or_walk(&root);
    let found = discover::discover(&root, &members).map_err(|e| eyre::eyre!("{e}"))?;
    for w in &found.warnings {
        eprintln!("nros build: warning: {w}");
    }
    if found.packages.is_empty() {
        eyre::bail!(
            "no packages under {} - is this a workspace root? A workspace has \
             packages carrying `package.xml`, or a `[workspace] members` list.",
            root.display()
        );
    }

    // ---- stage 2 --------------------------------------------------------
    let bringups = collect_images(&found.packages)?;
    let requested: Vec<String> = if args.all {
        plan::all_images(&bringups)
            .into_iter()
            .map(|(b, _, i, _)| plan::qualified(&b, &i))
            .collect()
    } else {
        args.images.clone()
    };
    let resolved = plan::resolve(&bringups, &requested).map_err(|e| eyre::eyre!("{e}"))?;

    // The driver is chosen by the board's PLATFORM, never by its name - a
    // Zephyr board is spelled `native_sim/native/64`, which says nothing about
    // being Zephyr. Resolving it needs the board catalog, which lives in a
    // nano-ros checkout, NOT in the user's workspace.
    let nano_ros_root = args
        .nano_ros_path
        .clone()
        .or_else(|| std::env::var_os("NROS_REPO_DIR").map(PathBuf::from))
        .or_else(|| crate::cmd::ws::autodetect_nano_ros_path(&root));
    let catalog = match &nano_ros_root {
        Some(r) => crate::orchestration::board_descriptor::BoardCatalog::load(r)
            .map_err(|e| eyre::eyre!("loading board descriptors from {}: {e}", r.display()))?,
        None => eyre::bail!(
            "no nano-ros checkout found, so board ids cannot be resolved. \
             Pass --nano-ros-path, or set NROS_REPO_DIR."
        ),
    };

    let has_non_rust = found
        .packages
        .iter()
        .any(|p| p.dir.join("CMakeLists.txt").is_file());

    let mut out = Vec::new();
    for (bringup, bringup_dir, image_id, image) in resolved {
        let qual = plan::qualified(&bringup, &image_id);
        let descriptor =
            crate::orchestration::image::resolve_image_board(&catalog, &image_id, &image)
                .map_err(|e| eyre::eyre!("{e}"))?;
        let platform = descriptor.platform.kebab().to_string();
        let board = image.board.clone().unwrap_or_default();
        let driver = plan::driver_for(&platform, has_non_rust);

        // ---- stage 3 ----------------------------------------------------
        // Before anything is generated or compiled: a missing prerequisite
        // fails HERE, naming the command that fixes it (RFC-0065 D2).
        let missing = crate::builder::preflight::check(descriptor, &root);
        if !missing.is_empty() {
            eyre::bail!("{}", crate::builder::preflight::report(&missing));
        }

        // ---- stage 4 ----------------------------------------------------
        let handoff = match driver {
            Driver::Cargo => {
                // The cargo root lives at the WORKSPACE root, not under
                // build/ — cargo requires members to sit below their root and
                // resolves a package's workspace by walking up. An existing
                // hand-written root is used as-is, never overwritten.
                let excluded = framework_entry_dirs(&found, &catalog);
                crate::builder::cargo_root::ensure(&found, &root, &excluded)
                    .map_err(|e| eyre::eyre!("{e}"))?;
                let mut a = vec!["build".to_string()];
                if args.offline {
                    // `--frozen` is `--locked --offline` by definition; issue
                    // 0676 records why `--offline` alone is the wrong spelling
                    // (it restricts the cache without pinning resolution).
                    a.push("--frozen".to_string());
                }
                a.extend(args.native_args.iter().cloned());
                // Run FROM the workspace root, not the manifest dir: cargo
                // discovers `.cargo/config.toml` by walking up from the CWD,
                // and the leaf `[patch.crates-io]` redirects `nros sync` writes
                // live there. Building from build/<coord> would lose every one
                // of them and resolve message crates against the public
                // registry — issue 0378 by a different road.
                Some(Handoff::new("cargo", a).in_dir(&root))
            }
            Driver::CMake => {
                // Unlike cargo, cmake imposes no root/member hierarchy rule, so
                // this root DOES live under build/<coord> (RFC-0065 D8).
                let manifest_dir = root.join("build").join(coordinate(&platform, &image));
                let spec = crate::builder::cmake_root::CmakeRootSpec {
                    workspace: root.clone(),
                    system: bringup.clone(),
                    platform: platform.clone(),
                    board: image.board.clone(),
                    rmw: image.rmw.clone().unwrap_or_else(|| "zenoh".to_string()),
                    toolchain_file: descriptor.cmake.as_ref().map(|c| c.toolchain_file.clone()),
                    nano_ros_root: nano_ros_root.clone().unwrap_or_default(),
                    excluded: framework_entry_dirs(&found, &catalog),
                };
                crate::builder::cmake_root::write(&found, &manifest_dir, &spec)
                    .map_err(|e| eyre::eyre!("{e}"))?;
                let rel_src = manifest_dir
                    .strip_prefix(&root)
                    .unwrap_or(&manifest_dir)
                    .display()
                    .to_string();
                // Configure and build in one handoff: `cmake --build` on a
                // fresh tree needs the configure first, and splitting them
                // would need TWO execs — which stage 5 cannot do, since the
                // first replaces the process.
                let mut a = vec![
                    "-S".to_string(),
                    rel_src.clone(),
                    "-B".to_string(),
                    format!("{rel_src}/cmake"),
                ];
                // The preamble path is passed rather than discovered inside the
                // generated file, so the generated file stays workspace-agnostic.
                let preamble = bringup_dir.join("cmake/preamble.cmake");
                if preamble.is_file() {
                    a.push(format!("-DNROS_WS_PREAMBLE={}", preamble.display()));
                }
                a.extend(args.native_args.iter().cloned());
                Some(Handoff::new("cmake", a).in_dir(&root))
            }
            _ => Some(native_handoff(driver, &root, &bringup_dir, &board, args)),
        };

        out.push(ResolvedBuild {
            qualified: qual,
            board,
            platform,
            driver,
            handoff,
        });
    }
    Ok(out)
}

pub fn run(args: Args) -> Result<()> {
    let plans = plan_builds(&args)?;
    for p in &plans {
        eprintln!(
            "nros build: {} -> board {} (platform {}), driver {}",
            p.qualified,
            p.board,
            p.platform,
            p.driver.program()
        );
        let Some(hand) = &p.handoff else {
            eyre::bail!(
                "stage 4 (generate the root build file) is not implemented yet \
                 - phase-383 W3 (cargo) / W4 (cmake).\n\
                 `{}` needs a generated root, so it cannot be built through \
                 `nros build` today. Until W3/W4 land, build it the existing \
                 way (cargo build / cmake --build).\n\
                 Images on Zephyr and ESP32 boards work now - they need no \
                 generated root (RFC-0065 D3).",
                p.qualified
            );
        };
        if args.dry_run {
            println!("{}", hand.display());
            continue;
        }
        // Never returns on success: this process BECOMES the build.
        let err = crate::builder::handoff::exec(hand).unwrap_err();
        eyre::bail!("{err}");
    }
    Ok(())
}

/// The build-tree coordinate for an image — RFC-0070 R2's vocabulary
/// (platform, rmw), never a new ad-hoc suffix.
///
/// Used by the cmake root (W4). The cargo root cannot use it: cargo pins its
/// workspace manifest to the workspace root, so there is no per-coordinate
/// cargo root to name.
fn coordinate(platform: &str, image: &crate::orchestration::image::ImageBlock) -> String {
    match image.rmw.as_deref() {
        Some(rmw) => format!("{platform}-{rmw}"),
        None => platform.to_string(),
    }
}

/// Package directories a cargo root must NOT list as members.
///
/// A west or ESP-IDF entry is built by its own framework; listing it makes a
/// host `cargo build` try to compile a Zephyr staticlib.
/// `examples/workspaces/rust` excludes exactly these by hand today.
fn framework_entry_dirs(
    found: &crate::builder::discover::Discovered,
    catalog: &crate::orchestration::board_descriptor::BoardCatalog,
) -> std::collections::BTreeSet<PathBuf> {
    use crate::orchestration::board_descriptor::DeployResolution;
    let mut out = std::collections::BTreeSet::new();
    for pkg in &found.packages {
        let Ok(text) = std::fs::read_to_string(pkg.dir.join("Cargo.toml")) else {
            continue;
        };
        let Ok(doc) = text.parse::<toml::Value>() else {
            continue;
        };
        let deploy = doc
            .get("package")
            .and_then(|p| p.get("metadata"))
            .and_then(|m| m.get("nros"))
            .and_then(|n| n.get("entry"))
            .and_then(|e| e.get("deploy"))
            .and_then(|d| d.as_str());
        let Some(deploy) = deploy else { continue };
        if let DeployResolution::Board(d) = catalog.resolve_deploy(deploy)
            && !plan::driver_for(d.platform.kebab(), false).needs_generated_root()
        {
            out.insert(pkg.dir.clone());
        }
    }
    out
}

fn native_handoff(
    driver: Driver,
    root: &std::path::Path,
    bringup_dir: &std::path::Path,
    board: &str,
    args: &Args,
) -> Handoff {
    match driver {
        Driver::West => {
            let mut a = vec!["build".to_string(), "-b".to_string(), board.to_string()];
            a.push(bringup_dir.display().to_string());
            a.extend(args.native_args.iter().cloned());
            Handoff::new("west", a).in_dir(root)
        }
        Driver::IdfPy => {
            let mut a = vec!["build".to_string()];
            a.extend(args.native_args.iter().cloned());
            Handoff::new("idf.py", a).in_dir(bringup_dir)
        }
        // Unreachable today — the caller bails before here for these two.
        Driver::Cargo | Driver::CMake => {
            let mut a = vec!["build".to_string()];
            a.extend(args.native_args.iter().cloned());
            Handoff::new(driver.program(), a).in_dir(root)
        }
    }
}

/// Read every bringup's `[image.*]`.
///
/// Bringups are derived from the packages stage 1 ALREADY found — a bringup is
/// simply a package carrying a `system.toml`. Deliberately not
/// `cmd::bringup::discover_bringups`, which walks one level of the workspace
/// root and so cannot see the canonical `<root>/src/<name>_bringup/` layout;
/// and deliberately not a second walk of our own, which would be a third
/// opinion about what a package is (issue 0809's class).
fn collect_images(
    packages: &[cargo_nano_ros::provider_scan::WorkspacePackage],
) -> Result<Vec<(String, PathBuf, plan::ImageSet)>> {
    let mut out = Vec::new();
    for pkg in packages {
        let system_toml = pkg.dir.join("system.toml");
        if !system_toml.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&system_toml)
            .wrap_err_with(|| format!("reading {}", system_toml.display()))?;
        let sys: crate::orchestration::cargo_metadata_schema::SystemToml =
            toml::from_str(&text).wrap_err_with(|| format!("parsing {}", system_toml.display()))?;
        out.push((
            pkg.name.clone(),
            pkg.dir.clone(),
            plan::ImageSet {
                images: sys.image.clone(),
                defaults: sys.image_defaults.clone(),
                default_images: sys.system.default_images.clone(),
            },
        ));
    }
    Ok(out)
}
