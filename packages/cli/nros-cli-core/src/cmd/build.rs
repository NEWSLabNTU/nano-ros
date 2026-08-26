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

    // Does the package graph cross languages? A CMakeLists is the signal — but
    // NOT the one a framework entry carries.
    //
    // phase-383 W8.a: `nano-ros-rt-eval` is pure Rust and holds exactly one
    // CMakeLists, `src/zephyr_entry/CMakeLists.txt`, which belongs to WEST.
    // Counting it routed every native image through cmake, which would have
    // failed on a workspace with no C or C++ in it at all. A framework entry's
    // build file is its framework's, not evidence about the graph.
    let framework_entries = framework_entry_dirs(&found, &catalog);
    let has_non_rust = found
        .packages
        .iter()
        .filter(|p| !framework_entries.contains(&p.dir))
        .any(|p| p.dir.join("CMakeLists.txt").is_file());

    let mut out = Vec::new();
    for (bringup, bringup_dir, image_id, image) in resolved {
        let qual = plan::qualified(&bringup, &image_id);
        let want_entry = crate::builder::entry::package_name(&image_id);
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
                // W3.b — generate the entry package. This is D4's headline
                // claim: the entry stops being hand-written.
                let entry_dir = generate_entry(
                    &root,
                    &bringup_dir,
                    &bringup,
                    &image_id,
                    &image,
                    descriptor,
                    &platform,
                    nano_ros_root.as_deref(),
                )?;
                if let Some(d) = &entry_dir {
                    eprintln!("nros build:   entry → {}", d.display());
                }

                // W7.a — the declarative escapes reach cargo here. `panic` is
                // forwarded to the ENTRY (the macro consumes it) rather than to
                // cargo; `profile` names a cargo profile.
                if let Some(p) = image.panic.as_deref() {
                    crate::orchestration::image::validate_panic(Some(p))
                        .map_err(|e| eyre::eyre!("`[image.{image_id}]`: {e}"))?;
                }
                // The cargo root lives at the WORKSPACE root, not under
                // build/ — cargo requires members to sit below their root and
                // resolves a package's workspace by walking up. An existing
                // hand-written root is used as-is, never overwritten.
                let excluded = framework_entry_dirs(&found, &catalog);
                let extra: Vec<PathBuf> = entry_dir.clone().into_iter().collect();
                crate::builder::cargo_root::ensure(&found, &root, &excluded, &extra)
                    .map_err(|e| eyre::eyre!("{e}"))?;
                let mut a = vec!["build".to_string()];
                // Build ONLY this image's entry. A bare `cargo build` at the
                // root builds every member, and nano-ros-rt-eval's own manifest
                // records why that is wrong: a cross-target member "would try
                // [it] for the host and fail".
                if let Some(d) = &entry_dir
                    && let Some(name) = d.file_name().and_then(|n| n.to_str())
                {
                    a.push("-p".to_string());
                    a.push(name.to_string());
                } else if root.join("src").join(&want_entry).is_dir() {
                    a.push("-p".to_string());
                    a.push(want_entry.clone());
                }
                // A cross board pins a triple, and dropping it builds the image
                // for the HOST — silently, since cargo is happy to. phase-383
                // W9 caught this on the freertos image, whose board declares
                // thumbv7m-none-eabi.
                if let Some(triple) = descriptor.target.as_deref() {
                    a.push("--target".to_string());
                    a.push(triple.to_string());
                }
                if let Some(profile) = image.profile.as_deref() {
                    // `--profile` rather than `--release`: a named profile is
                    // what `[image.<id>].profile` declares, and `release` is
                    // just one of its legal values.
                    a.push("--profile".to_string());
                    a.push(profile.to_string());
                }
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
                    excluded: {
                        let mut e = framework_entries.clone();
                        e.extend(entries_for_other_boards(&found, &board, &platform));
                        e
                    },
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
            Driver::West => {
                // W5 — overlays reach Zephyr through EXTRA_CONF_FILE and
                // APPLICATION_CONFIG_DIR. Never CONF_FILE: that suppresses
                // Zephyr's own boards/ and socs/ discovery entirely.
                let overlays = crate::builder::zephyr::resolve(&bringup_dir, &board, &image)
                    .map_err(|e| eyre::eyre!("{e}"))?;
                let mut a = vec!["build".to_string(), "-b".to_string(), board.clone()];
                if overlays.sysbuild {
                    a.push("--sysbuild".to_string());
                }
                a.push(bringup_dir.display().to_string());
                let west_opts = crate::builder::zephyr::west_args(&overlays);
                if !west_opts.is_empty() || !args.native_args.is_empty() {
                    // Everything after `--` is a cmake option for the app.
                    a.push("--".to_string());
                    a.extend(west_opts);
                    a.extend(args.native_args.iter().cloned());
                }
                Some(Handoff::new("west", a).in_dir(&root))
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

/// Generate the entry package for a cargo image (W3.b), returning its
/// directory. `None` when the launch tree cannot be resolved — reported as a
/// warning rather than a failure, because a workspace whose entries are still
/// hand-written must keep building through the migration (RFC-0065 D13).
#[allow(clippy::too_many_arguments)]
fn generate_entry(
    root: &std::path::Path,
    bringup_dir: &std::path::Path,
    bringup: &str,
    image_id: &str,
    image: &crate::orchestration::image::ImageBlock,
    descriptor: &crate::orchestration::board_descriptor::BoardDescriptor,
    platform: &str,
    nano_ros_root: Option<&std::path::Path>,
) -> Result<Option<PathBuf>> {
    use crate::{
        builder::entry::{BoardFacts, EntrySpec},
        orchestration::model_location,
    };

    let Some(nros_root) = nano_ros_root else {
        return Ok(None);
    };

    // A workspace that still has its hand-written entry keeps it. Generating a
    // second one would be redundant at best and a conflicting `[[bin]]` name at
    // worst — and D13's migration is a DELETION: remove the hand-written entry
    // and the next build generates it. This is what makes the migration
    // incremental, one entry at a time.
    let want = crate::builder::entry::package_name(image_id);
    if root.join("src").join(&want).is_dir() {
        return Ok(None);
    }

    // (launch, args) → model → plan → the node packages the launch names.
    let args_vec: Vec<(String, String)> = image
        .args
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let model_rel = match model_location::launch_to_model_rel(
        bringup_dir,
        image.launch.as_deref(),
        &args_vec,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("nros build: warning: cannot resolve launch for `{image_id}`: {e}");
            return Ok(None);
        }
    };
    let model_path = match model_location::ensure_model(bringup_dir, &model_rel) {
        Ok((p, _inputs)) => p,
        Err(e) => {
            eprintln!("nros build: warning: cannot resolve the model for `{image_id}`: {e}");
            return Ok(None);
        }
    };
    let plan = match crate::codegen::entry::plan_from_model(&model_path, image.board.clone()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("nros build: warning: cannot plan `{image_id}`: {e}");
            return Ok(None);
        }
    };

    // A launch file may name one package several times; cargo needs it once.
    let mut seen = std::collections::BTreeSet::new();
    let mut nodes = Vec::new();
    for n in &plan.nodes {
        if !seen.insert(n.pkg.clone()) {
            continue;
        }
        let dir = root.join("src").join(&n.pkg);
        if dir.is_dir() {
            nodes.push((n.pkg.clone(), dir));
        }
    }

    let launch = match image.launch.as_deref() {
        Some(f) => format!("{bringup}:{f}"),
        None => bringup.to_string(),
    };
    let facade_dir = {
        let d = root
            .join("generated/nros-selection")
            .join(crate::builder::entry::package_name(image_id));
        d.is_dir().then_some(d)
    };

    let spec = EntrySpec {
        image_id: image_id.to_string(),
        // The deploy token is the IMAGE id — that is what `[deploy.<id>]`
        // always keyed on and what `nros check` reads.
        deploy: image_id.to_string(),
        launch,
        args: image.args.clone(),
        panic: image.panic.clone(),
        nodes,
        nano_ros_root: nros_root.to_path_buf(),
        facade_dir,
    };
    // Most specific first: the image id IS the deploy key, but an image named
    // `robot1` is not a board token, so the board and platform back it up.
    let board_name = image.board.clone().unwrap_or_default();
    let facts = BoardFacts::from_descriptor_for(descriptor, &[image_id, &board_name, platform]);
    let parent = root.join("build").join(coordinate(platform, image));
    let dir = crate::builder::entry::write(&spec, &facts, &parent)
        .map_err(|e| eyre::eyre!("generating the entry for `{image_id}`: {e}"))?;
    Ok(Some(dir))
}

/// Deploy tokens a package's entry declaration names, if it is an entry.
///
/// Two spellings, because the two languages declare it in different files:
/// Rust in `[package.metadata.nros.entry] deploy`, C/C++ in the
/// `nano_ros_add_executable(… DEPLOY <token>…)` call. Both are read; a package
/// that is not an entry yields an empty list.
fn entry_deploy_tokens(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(text) = std::fs::read_to_string(dir.join("Cargo.toml"))
        && let Ok(doc) = text.parse::<toml::Value>()
        && let Some(d) = doc
            .get("package")
            .and_then(|p| p.get("metadata"))
            .and_then(|m| m.get("nros"))
            .and_then(|n| n.get("entry"))
            .and_then(|e| e.get("deploy"))
            .and_then(|d| d.as_str())
    {
        out.push(d.to_string());
    }
    if let Ok(text) = std::fs::read_to_string(dir.join("CMakeLists.txt")) {
        for line in text.lines() {
            // Comments explain the keyword constantly, so only a line whose
            // FIRST token is DEPLOY is a declaration.
            let t = line.trim();
            if t.starts_with('#') {
                continue;
            }
            if let Some(rest) = t.strip_prefix("DEPLOY") {
                out.extend(
                    rest.trim_end_matches(')')
                        .split_whitespace()
                        .filter(|w| !w.starts_with("${"))
                        .map(|w| w.trim_matches('"').to_string()),
                );
            }
        }
    }
    out
}

/// Entry packages that belong to a DIFFERENT board than the one being built.
///
/// RFC-0065's Problem statement names this as one of the four jobs a
/// hand-written root does by hand: *"which entries belong to the active
/// platform, by hand"*. phase-383 W8.b caught the emitter skipping it —
/// `autoware-safety-island` has three FreeRTOS entries (an536, posix, s32z2)
/// and a `freertos-posix` build listed all three.
///
/// An entry naming NO deploy token is kept: it has expressed no opinion, and
/// silently dropping a package is the failure this whole phase exists to
/// remove.
fn entries_for_other_boards(
    found: &crate::builder::discover::Discovered,
    board: &str,
    platform: &str,
) -> std::collections::BTreeSet<PathBuf> {
    let mut out = std::collections::BTreeSet::new();
    for pkg in &found.packages {
        let tokens = entry_deploy_tokens(&pkg.dir);
        if tokens.is_empty() {
            continue;
        }
        // The same three spellings `nano_ros_entry` itself accepts.
        let mine = tokens
            .iter()
            .any(|t| t == board || t == platform || t.is_empty());
        if !mine {
            out.insert(pkg.dir.clone());
        }
    }
    out
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
