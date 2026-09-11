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

    /// The west workspace holding Zephyr — the directory that contains
    /// `zephyr/`. Zephyr images only.
    ///
    /// The explicit rung of the resolution ladder, above `$ZEPHYR_BASE` and
    /// `$NROS_ZEPHYR_WORKSPACE`. An environment variable is ambient state that
    /// a user has to remember to set and cannot see in the command they ran;
    /// this makes the same fact reviewable in a script and in shell history.
    ///
    /// Pointing it at the `zephyr/` directory itself also works — that is the
    /// commonest way to get this wrong, and both spellings name one place.
    #[arg(long, value_name = "DIR")]
    pub zephyr_workspace: Option<PathBuf>,

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

    /// Build only these packages — no dependencies, no dependents
    /// (RFC-0087 D7, phase-420 W7).
    ///
    /// colcon's flag, with colcon's meaning, and two deliberate divergences
    /// documented on `builder::discover::select`: a name matching no package is
    /// an ERROR here rather than a warning, and a selection that drops a
    /// package another selected package depends on is REFUSED rather than left
    /// to resolve against an install prefix nano-ros does not have.
    ///
    /// The selection narrows what the build CONTAINS — the generated cargo
    /// root's member list and the generated CMake root's subdirectories. It
    /// does not narrow which images exist: an image is declared by a bringup's
    /// `system.toml`, which is a property of the workspace, not of the
    /// selection, so `nros build native --packages-select talker_pkg` still
    /// means the `native` image, built from a narrowed workspace.
    #[arg(long, value_name = "PKG", num_args = 1..)]
    pub packages_select: Vec<String>,

    /// Build these packages and everything they depend on, transitively, and
    /// nothing else (RFC-0087 D7, phase-420 W7).
    ///
    /// Composes with `--packages-select` as an INTERSECTION, which is colcon's
    /// composition too: each flag deselects independently, so adding one can
    /// only ever narrow the build.
    #[arg(long, value_name = "PKG", num_args = 1..)]
    pub packages_up_to: Vec<String>,

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
    /// The image's RMW, resolved the one way (RFC-0085 D2).
    ///
    /// The fact two derivations most easily disagree about: a west build reads
    /// `CONFIG_NROS_RMW_*` out of Kconfig, while `[image.*]` says `rmw`, and
    /// nothing made them agree. Exposed so `nros image-facts` can hand cmake
    /// the image's answer instead of cmake inferring its own.
    pub rmw: Option<String>,
    /// The entry package this image builds, when one was resolved.
    pub entry_package: Option<String>,
    /// The rustc target triple the board pins, if any.
    pub target: Option<String>,
    /// The cargo profile the image declares, if any.
    pub profile: Option<String>,

    /// A configure that must run BEFORE the handoff, for drivers that need one.
    ///
    /// cmake is the only such driver: `cmake --build` on an unconfigured tree
    /// fails, and configure+build is two invocations at our 3.22 floor
    /// (`--workflow` is 3.25+). Stage 5 execs ONE command and cannot do both,
    /// so the configure belongs to generation — which is what it is: writing
    /// the build system next to the root that was just written.
    ///
    /// Kept on the plan rather than performed during planning so `plan_builds`
    /// stays side-effect free and `--dry-run` can PRINT it. [`run`] performs it.
    pub configure: Option<Handoff>,
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
    // ABSOLUTE from here on. `--workspace .` is an ordinary invocation — the
    // fixture driver cd's into the workspace and passes exactly that — and
    // every generated file computes paths RELATIVE to this root against the
    // nano-ros checkout and the user's packages. `relative_or_err` needs two
    // absolute paths and correctly refuses otherwise, so a relative root
    // surfaced as "cannot express /abs/packages/api/nros relative to
    // ./build/posix-zenoh/native_entry" — an error about the wrong thing.
    //
    // `canonicalize` rather than `absolute`: symlinked checkouts are normal
    // here, and two spellings of one directory would produce two different
    // relative paths in generated files that are supposed to be byte-identical.
    let root = std::fs::canonicalize(&root)
        .wrap_err_with(|| format!("resolving workspace root {}", root.display()))?;

    // phase-445 W4b (RFC-0098 D1) — a single-package example is a workspace of
    // one: its own `Cargo.toml` is the cargo root, and its settings come from
    // `build/<image>/nros-cargo.toml`. Decided before discovery, because the
    // workspace road below would treat the package's own `[workspace]` marker
    // as a root build file to retire (D9) and generate an entry it does not need.
    if let Some(plans) = plan_single_package(args, &root)? {
        return Ok(plans);
    }

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
    // Images come from the FULL workspace, before any selection: an image is
    // declared by a bringup's `system.toml` and is a property of the workspace,
    // exactly as the generated cargo root's member list is. A selection that
    // happened to drop a bringup package must not make its images cease to
    // exist — that would answer `--packages-select` with "this workspace
    // declares no `[image.*]`", an error about the wrong thing.
    let bringups = collect_images(&found.packages)?;
    // Every bringup directory — where a workspace ENTRY's board is stated
    // (`leaf_system::for_entry`, phase-445 W5), and so where the entry
    // classification below reads it.
    let bringup_dirs: Vec<PathBuf> = bringups.iter().map(|(_, d, _)| d.clone()).collect();

    // ---- stage 1b — the selection verbs (RFC-0087 D7, phase-420 W7) -----
    //
    // Filters the topological order stage 1 already computed; it does not sort
    // again. `all_packages` keeps the unnarrowed set, because
    // `check_declared_depends` walks the whole tree itself and would read a
    // deliberately-dropped package as an unresolved `<depend>`.
    let all_packages = found.packages.clone();
    let found = discover::select(
        &found,
        &discover::Selection {
            select: args.packages_select.clone(),
            up_to: args.packages_up_to.clone(),
        },
    )
    .map_err(|e| eyre::eyre!("{e}"))?;

    let requested: Vec<String> = if args.all {
        plan::all_images(&bringups)
            .into_iter()
            .map(|(b, _, i, _)| plan::qualified(&b, &i))
            .collect()
    } else {
        args.images.clone()
    };
    // RFC-0065 D1 / RFC-0098 D9 (phase-445 W5) — package mode, decided HERE,
    // before stage 2: a workspace with no bringup declares no image, so
    // `plan::resolve` would refuse it ("declares no `[image.*]`") before the
    // package-mode branch below could ever be reached. Only when nothing was
    // asked for by name, and only for a directory of packages: a root that is
    // itself a package is a single-package example, whose `system.toml` image
    // is the unit (RFC-0098 D3).
    let package_mode = args.images.is_empty()
        && !args.all
        && !nros_orchestration_ir::leaf_system::is_package_dir(&root)
        && !all_packages.iter().any(|p| is_bringup_dir(&p.dir));
    let resolved = if package_mode {
        Vec::new()
    } else {
        plan::resolve(&bringups, &requested).map_err(|e| eyre::eyre!("{e}"))?
    };

    // The driver is chosen by the board's PLATFORM, never by its name - a
    // Zephyr board is spelled `native_sim/native/64`, which says nothing about
    // being Zephyr. Resolving it needs the board catalog, which lives in a
    // nano-ros checkout, NOT in the user's workspace.
    //
    // phase-447 A2 (RFC-0099 D3) — the ladder moved to
    // `orchestration::nano_ros_root`, and gained a fourth rung LAST: this
    // toolchain's own `share/nano-ros`. Before it, all three rungs were a
    // CHECKOUT, so a released `nros` reached the bail below on the first
    // release ever cut.
    let nano_ros_root =
        crate::orchestration::nano_ros_root::resolve(args.nano_ros_path.clone(), &root);
    // phase-398 W3 — every `<depend>` resolves, or the build stops.
    //
    // Runs once per invocation, before anything is generated, because an
    // undeclared prerequisite is cheapest to report before a toolchain is
    // touched (RFC-0065 D2's reasoning, applied to dependencies).
    check_declared_depends(&root, &all_packages, nano_ros_root.as_deref())?;
    // RFC-0094 D3 / phase-439 W3 — a package that PARTICIPATES (carries a build
    // file) must carry the file its declared `<build_type>` needs.
    //
    // Reported here for the same reason as the line above: before a toolchain is
    // touched, naming every offender at once. And loudly, because the pre-W3
    // behaviour was a SILENT skip — the site routed the package by whichever
    // build file it happened to have and never read the declaration, so a
    // `<build_type>nros_cargo</build_type>` over a directory with only a
    // `CMakeLists.txt` built as cmake and looked correct.
    crate::routing::check_declarations(&all_packages).map_err(|e| eyre::eyre!("{e}"))?;

    // RFC-0065 D1 / RFC-0098 D9 (phase-445 W5) — a workspace with NO bringup
    // builds like colcon: every package, in dependency order, each with its own
    // driver and its own `build/<pkg>/`. There is no image to resolve (an image
    // is declared by a bringup) — which is what `nros build` said to
    // `examples/templates/{local-msg-package,workspace-shadowing}` until their
    // hand-written roots, the only thing that built them, were deleted. After
    // the dependency and declaration preflights above, which hold for either
    // mode.
    if package_mode {
        return plan_packages(&root, &found, args, nano_ros_root.as_deref());
    }

    // The workspace's OWN packages can carry board descriptors, so a board is
    // declared where everything else about this workspace is declared.
    let pkg_dirs: Vec<PathBuf> = found.packages.iter().map(|p| p.dir.clone()).collect();
    let catalog = match &nano_ros_root {
        Some(r) => {
            crate::orchestration::board_descriptor::BoardCatalog::load_with_packages(r, &pkg_dirs)
                .map_err(|e| eyre::eyre!("loading board descriptors from {}: {e}", r.display()))?
        }
        None => eyre::bail!("{}", crate::orchestration::nano_ros_root::not_found_help()),
    };

    // Does the package graph cross languages? A CMakeLists is the signal — but
    // NOT the one a framework entry carries.
    //
    // phase-383 W8.a: `nano-ros-rt-eval` is pure Rust and holds exactly one
    // CMakeLists, `src/zephyr_entry/CMakeLists.txt`, which belongs to WEST.
    // Counting it routed every native image through cmake, which would have
    // failed on a workspace with no C or C++ in it at all. A framework entry's
    // build file is its framework's, not evidence about the graph.
    let framework_entries = framework_entry_dirs(&found, &catalog, &bringup_dirs);
    let ws_non_rust: Vec<&str> = found
        .packages
        .iter()
        .filter(|p| !framework_entries.contains(&p.dir))
        // RFC-0094 D3 (phase-439 W3) — the package's DECLARED build type, not
        // the presence of a `CMakeLists.txt`. The two differ for a package that
        // carries both build files: a Rust node whose `CMakeLists.txt` registers
        // it with the workspace's cmake build is cmake-driven and says so, while
        // a crate with a C harness beside it is not, and file presence cannot
        // tell them apart. This is the same refinement W8.a made for framework
        // entries ("a framework entry's build file is its framework's, not
        // evidence about the graph"), reached from the declaration instead of
        // from a second directory probe.
        .filter(|p| crate::routing::route(p).cmake_subdir)
        .map(|p| p.name.as_str())
        .collect();

    // Does THIS IMAGE's graph cross languages? Not the workspace's.
    //
    // `examples/workspaces/safety` is the case the workspace-wide answer got
    // wrong: it holds C, C++ AND Rust node packages against one bringup, so
    // `has_non_rust` was true for every image and the two Rust images were
    // emitted as `nano_ros_add_executable` calls. That failed one layer down,
    // in the typed-entry codegen, with the Rust package named:
    //
    //     typed entry: launch node pkg `rust_safety_listener_pkg` exec
    //     `safe_listener` has no matching component in nros-metadata.json
    //
    // — correct, and about the wrong thing. It is the same refinement W8.a
    // already made once for framework entries ("a framework entry's build file
    // is its framework's, not evidence about the graph"), taken one level
    // finer: a C package the image never links is not evidence either.
    //
    // Evidence, not assumption: an image whose launch names NO known package
    // (unreadable file, `<include>`-only, a pkg outside this workspace) falls
    // back to the workspace answer rather than guessing Rust, because guessing
    // wrong toward cargo drops the C half of a graph silently, while guessing
    // wrong toward cmake fails loudly at configure.
    let image_has_non_rust = |image: &crate::orchestration::image::ImageBlock,
                              bringup_dir: &std::path::Path| {
        let pkgs = crate::orchestration::image::launch_node_pkgs(image, bringup_dir);
        let known: Vec<&String> = pkgs
            .iter()
            .filter(|n| found.packages.iter().any(|p| &p.name == *n))
            .collect();
        if known.is_empty() {
            return !ws_non_rust.is_empty();
        }
        known.iter().any(|n| ws_non_rust.contains(&n.as_str()))
    };

    let mut out = Vec::new();
    for (bringup, bringup_dir, image_id, image) in resolved {
        let qual = plan::qualified(&bringup, &image_id);
        let want_entry = crate::builder::entry::package_name(&image_id);
        // A `launch` that names no file is a typo, and W9.a wrote three of
        // them as PROSE fragments that survived two waves because nothing
        // built from the declarations. Caught here, against the bringup, with
        // the available names in the message.
        crate::orchestration::image::validate_image_launch(&image_id, &image, &bringup_dir)
            .map_err(|e| eyre::eyre!("{e}"))?;
        let descriptor =
            crate::orchestration::image::resolve_image_board(&catalog, &image_id, &image)
                .map_err(|e| eyre::eyre!("{e}"))?;
        let platform = descriptor.platform.kebab().to_string();
        let board = image.board.clone().unwrap_or_default();
        let driver = plan::driver_for_board(
            &platform,
            descriptor.entry_kind,
            image_has_non_rust(&image, &bringup_dir),
        );

        // ---- stage 3 ----------------------------------------------------
        // Before anything is generated or compiled: a missing prerequisite
        // fails HERE, naming the command that fixes it (RFC-0065 D2).
        let missing = crate::builder::preflight::check(descriptor, &root, nano_ros_root.as_deref());
        if !missing.is_empty() {
            eyre::bail!("{}", crate::builder::preflight::report(&missing));
        }

        // ---- stage 3.5 — the RESOLVE phase (RFC-0094 D1, phase-439 W2) ---
        //
        // ONE PLACE DECIDES A KNOB, every other place reads it. Between "is the
        // toolchain present" and "emit a root build file", because that is the
        // last point at which nothing has been generated and the first at which
        // the image is fully identified.
        //
        // It reads DECLARATIONS and never a compiled artifact. That is the
        // whole point: `nros-rmw-zenoh` is a DEPENDENCY of the leaf, so the
        // crate that must know the entity counts compiles before the crate
        // whose source declares them, and no build script, proc macro or
        // manifest key reaches backwards across that edge (issue 0827).
        //
        // It cannot FAIL a build. An image whose declarations this phase cannot
        // reach gets a `resolved.toml` that says so and no CMake projection, so
        // every downstream lane behaves exactly as it does today. Making the
        // resolve phase a new way for a build to stop would be a regression
        // paid by every image for the benefit of the few that derive.
        let resolved = resolve_image(
            &root,
            &bringup,
            &bringup_dir,
            &image_id,
            &image,
            &platform,
            &board,
        );
        let resolved_dir = resolved.as_ref().map(|(d, _)| d.clone());

        // ---- stage 4 ----------------------------------------------------
        let mut cmake_configure: Option<Handoff> = None;
        let mut cargo_prepare: Option<Handoff> = None;
        let handoff = match driver {
            Driver::Cargo => {
                // RFC-0098 D9 — a workspace has no root build file. A root the
                // phase-383 builder left behind (gitignored, so every checkout
                // that ran it still has one) claims the entry below it and
                // cargo refuses the entry; it is our output, so it goes. An
                // AUTHORED `[workspace]` is the user's file and is refused.
                if let Some(p) =
                    crate::builder::cargo_root::retire(&root).map_err(|e| eyre::eyre!("{e}"))?
                {
                    eprintln!(
                        "nros build:   removed {} — the phase-383 generated workspace root; \
                         each image is its own cargo root now (RFC-0098 D9)",
                        p.display()
                    );
                }

                // W3.b — generate the entry package. This is D4's headline
                // claim: the entry stops being hand-written.
                let generated = generate_entry(
                    &root,
                    &bringup_dir,
                    &bringup,
                    &image_id,
                    &image,
                    descriptor,
                    &platform,
                    nano_ros_root.as_deref(),
                )?;
                if let Some(d) = &generated.dir {
                    eprintln!("nros build:   entry → {}", d.display());
                }

                // W7.a — the declarative escapes reach cargo here. `panic` is
                // forwarded to the ENTRY (the macro consumes it) rather than to
                // cargo; `profile` names a cargo profile.
                if let Some(p) = image.panic.as_deref() {
                    crate::orchestration::image::validate_panic(Some(p))
                        .map_err(|e| eyre::eyre!("`[image.{image_id}]`: {e}"))?;
                }

                // `rmw` reaches the build on this driver too, since issue 0831.
                //
                // It used to be inert here and the build REFUSED an image whose
                // rmw differed from `[system] rmw`, because the backend came
                // from the `<entry>_nros_selection` facade and nothing
                // consulted the image — so `[image.native_cyclonedds]` produced
                // `build/posix-cyclonedds/` holding a zenoh binary. The facade
                // now reads the image (`facade::image_rmw`), so the refusal is
                // gone and the coordinate directory names what it contains.
                // ---- which manifest is this image's cargo root -----------
                //
                // The generated entry, or a hand-written / materialised
                // `src/<entry>` that suppressed generation (RFC-0065 D13). Each
                // is its own root: there is no workspace to `-p` into
                // (RFC-0098 D9), so the build names the MANIFEST.
                let image_dir = root
                    .join("build")
                    .join(coordinate(&platform, &image))
                    .join(&want_entry);
                let hand_written = root.join("src").join(&want_entry);
                let lock_is_ours = generated.dir.is_some();
                let manifest_dir = match &generated.dir {
                    Some(d) => d.clone(),
                    None if hand_written.join("Cargo.toml").is_file() => hand_written.clone(),
                    // `--dry-run` answers "what would run" even when the launch
                    // cannot be resolved on this host, so it names the path the
                    // entry WILL be generated at.
                    None if args.dry_run => image_dir.clone(),
                    None => eyre::bail!(
                        "`{qual}` has no entry package: the generated one could not be \
                         written (see the warning above) and there is no hand-written \
                         `src/{want_entry}`. With no workspace root (RFC-0098 D9) there is \
                         nothing else for cargo to build."
                    ),
                };

                // ---- the one settings file (RFC-0098 D1/D7) --------------
                //
                // Everything cargo needs for this image: the board's
                // `cargo_config` and triple, this image's target dir, its
                // entity facts and derived pool knobs as `[env]`, and the in-repo
                // `[patch]` rows. It REPLACES three carriers: the tracked
                // `<ws>/.cargo/config.toml`, `--target` on this command line,
                // and the per-invocation `NROS_DECLARED_*` environment the
                // handoff used to carry (phase-392 W5).
                let Some(nros_root) = nano_ros_root.as_deref() else {
                    eyre::bail!("no nano-ros checkout; the board catalog could not have loaded");
                };
                let mut env = generated.entity_facts.clone();
                if let Some((_, r)) = &resolved {
                    env.extend(derived_pool_env(r));
                }
                let config_path = image_dir.join(crate::builder::cargo_config::FILE_NAME);
                crate::builder::cargo_config::write(
                    &crate::builder::cargo_config::CargoConfigSpec {
                        image_id: image_id.clone(),
                        board: board.clone(),
                        cargo_config: descriptor.cargo_config.clone(),
                        target: descriptor.target.clone(),
                        nano_ros_root: nros_root.to_path_buf(),
                        workspace: root.clone(),
                        target_dir: image_dir.join("target"),
                        env,
                        patches: registry_patches(&root, nros_root, &manifest_dir),
                        ..Default::default()
                    },
                    &config_path,
                )
                .map_err(|e| eyre::eyre!("writing the settings for `{image_id}`: {e}"))?;
                eprintln!("nros build:   settings → {}", config_path.display());

                // Relative to the workspace root, which is the handoff's cwd, so
                // the printed command is the one a user can retype.
                let shown = |p: &std::path::Path| {
                    p.strip_prefix(&root)
                        .map(|r| r.display().to_string())
                        .unwrap_or_else(|_| p.display().to_string())
                };
                let manifest_arg = shown(&manifest_dir.join("Cargo.toml"));
                let config_arg = shown(&config_path);
                let mut a = vec![
                    "build".to_string(),
                    "--manifest-path".to_string(),
                    manifest_arg.clone(),
                    "--config".to_string(),
                    config_arg.clone(),
                ];
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
                // A GENERATED entry has a GENERATED lock beside it, and the
                // PATH shim's project-wide `--locked` (or `--frozen` above)
                // forbids creating or moving one:
                //
                //   error: cannot update the lock file … because --frozen
                //   was passed to prevent this
                //
                // `--locked` exists to stop a build silently re-resolving an
                // AUTHORED lock — a promise that someone else's build resolves
                // what yours did. This lock is build output of a manifest this
                // process just wrote, so there is no promise to protect. So
                // resolve it ONCE before the build, which itself stays locked.
                //
                // `update --workspace`, not `generate-lockfile`: it creates a
                // missing lock and otherwise touches only the entry itself, so
                // an unchanged image keeps every version it already resolved
                // instead of re-resolving the world on each build.
                // `NROS_CARGO_FLAGS=` because the shim would forbid this step
                // too. A hand-written entry's EXISTING lock is the user's and is
                // left alone; one with NO lock gets one created, because each
                // entry is its own cargo root now (RFC-0098 D9) and the lock
                // that used to cover it was the deleted workspace root's —
                // `--frozen` cannot build a root that has none.
                if lock_is_ours || !manifest_dir.join("Cargo.lock").is_file() {
                    let mut u = vec![
                        "update".to_string(),
                        "--workspace".to_string(),
                        "--manifest-path".to_string(),
                        manifest_arg,
                        "--config".to_string(),
                        config_arg,
                    ];
                    if args.offline {
                        u.push("--offline".to_string());
                    }
                    cargo_prepare = Some(
                        Handoff::new("cargo", u)
                            .in_dir(&root)
                            .with_env("NROS_CARGO_FLAGS", ""),
                    );
                }
                a.extend(args.native_args.iter().cloned());
                // From the workspace root, which no longer carries anything: the
                // manifest and the settings are both NAMED. cargo still reads
                // `.cargo/config.toml` from the cwd's ancestors (it always does),
                // which is where a user's own toolchain preference belongs
                // (RFC-0098 D1) — and why the per-image settings are never put
                // in one.
                Some(Handoff::new("cargo", a).in_dir(&root))
            }
            Driver::CMake => {
                // Unlike cargo, cmake imposes no root/member hierarchy rule, so
                // this root DOES live under build/<coord> (RFC-0065 D8).
                let manifest_dir = root.join("build").join(cmake_coordinate(&platform, &image));
                // W4.b — every image that lands on THIS coordinate.
                //
                // They share `build/<coord>/`, so emitting only the image being
                // built means the root is rewritten on every image switch and
                // the workspace never declares more than one executable at a
                // time. Same shape as the cargo root's member list, same
                // answer: the root is a property of the WORKSPACE.
                //
                // An image still carrying a hand-written package contributes
                // nothing — it is a discovered SUBDIR, and a second target of
                // that name would collide. Delete the package and the next
                // build emits its call (D13, incremental).
                let coord = cmake_coordinate(&platform, &image);
                // A C++ source ANYWHERE in a package, not just at its top.
                //
                // These packages keep sources in `src/` — `talker_pkg/src/Talker.cpp` —
                // so a top-level scan called the pure-C++ workspace `c`, and
                // `nros codegen entry` refused with the right complaint from the
                // wrong layer: "node pkg `talker_pkg` exec `talker` is lang
                // `cpp`, not `c`". The model knows each exec's language; until
                // the emitter reads it, look where the sources actually are.
                let has_cpp = found.packages.iter().any(|p| {
                    [p.dir.clone(), p.dir.join("src")].iter().any(|d| {
                        d.read_dir()
                            .map(|rd| {
                                rd.flatten().any(|e| {
                                    let n = e.file_name();
                                    let n = n.to_string_lossy();
                                    n.ends_with(".cpp") || n.ends_with(".cc") || n.ends_with(".cxx")
                                })
                            })
                            .unwrap_or(false)
                    })
                });
                let cmake_entries = plan::all_images(&bringups)
                    .into_iter()
                    .filter(|(b, bd, _, img)| {
                        b == &bringup
                            && crate::orchestration::image::resolve_image_board(&catalog, "", img)
                                .map(|d| {
                                    plan::driver_for_board(
                                        d.platform.kebab(),
                                        d.entry_kind,
                                        image_has_non_rust(img, bd),
                                    ) == Driver::CMake
                                        && cmake_coordinate(d.platform.kebab(), img) == coord
                                })
                                .unwrap_or(false)
                    })
                    .filter_map(|(_, _, id, img)| {
                        let name = crate::builder::entry::package_name(&id);
                        if root
                            .join("src")
                            .join(&name)
                            .join("CMakeLists.txt")
                            .is_file()
                        {
                            return None;
                        }
                        let b = img.board.clone().unwrap_or_default();
                        Some(crate::builder::cmake_root::CmakeEntry {
                            launch: img.launch.clone().unwrap_or_else(|| "default".to_string()),
                            args: img
                                .args
                                .iter()
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect(),
                            // The workspace's own language: the generated TU has
                            // to compile against what it links.
                            lang: if has_cpp { "cpp" } else { "c" }.to_string(),
                            // The SAME candidate search the Rust entry uses:
                            // DEPLOY is what the macro looks up, and an image is
                            // not always named after a board.
                            // The BOARD, verbatim — not `macro_deploy_token`.
                            //
                            // That function answers for the RUST macro's board
                            // table, which is keyed on tokens like `freertos`
                            // and does not know `mps2-an385-freertos`.
                            // `nano_ros_add_executable(DEPLOY …)` resolves
                            // against the board CATALOG, which does, and the
                            // hand-written entry said exactly the board id.
                            // Routing it through the macro's table picked the
                            // GENERIC freertos board, and nothing failed until
                            // the link, where the mps2 board's lwIP glue was
                            // absent: `undefined reference to lwip_setsockopt`.
                            deploy: if b.is_empty() {
                                platform.clone()
                            } else {
                                b.clone()
                            },
                            panic: img.panic.clone(),
                            name,
                        })
                    })
                    .collect();

                let spec = crate::builder::cmake_root::CmakeRootSpec {
                    entries: cmake_entries,
                    workspace: root.clone(),
                    system: bringup.clone(),
                    platform: platform.clone(),
                    board: image.board.clone(),
                    rmw: image.rmw.clone().unwrap_or_else(|| "zenoh".to_string()),
                    toolchain_file: descriptor.cmake.as_ref().map(|c| c.toolchain_file.clone()),
                    nano_ros_root: nano_ros_root.clone().unwrap_or_default(),
                    excluded: {
                        let mut e = framework_entries.clone();
                        e.extend(entries_for_other_boards(
                            &found,
                            &board,
                            &platform,
                            &bringup_dirs,
                        ));
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
                // The configure. Its own step, not the handoff.
                //
                // The comment here used to say "configure and build in one
                // handoff", and the args only ever configured — so `nros build`
                // on a cmake workspace wrote a build system and produced no
                // binary. CMake cannot do both in one invocation at our 3.22
                // floor (`--workflow` is 3.25+), and stage 5 execs exactly one
                // command, so the configure moves to generation where it
                // belongs: it WRITES the build system, next to the root file
                // this stage just wrote. [`run`] performs it before the exec.
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
                // RFC-0094 D1 — where stage 3.5 put this image's answer. The
                // configure READS it; it does not re-derive it.
                //
                // Passed rather than discovered, for the reason the preamble
                // above is: a generated build file that computes a path into
                // the caller's tree stops being workspace-agnostic. It is also
                // what keeps the seam HONEST — a lane that did not run stage
                // 3.5 (a bare `west build`, `just zephyr build-fixtures`) sets
                // nothing, finds nothing, and behaves exactly as it does today.
                if let Some(d) = &resolved_dir {
                    a.push(format!("-DNROS_RESOLVED_DIR={}", d.display()));
                }
                a.extend(args.native_args.iter().cloned());
                cmake_configure = Some(Handoff::new("cmake", a).in_dir(&root));
                Some(
                    Handoff::new(
                        "cmake",
                        vec!["--build".to_string(), format!("{rel_src}/cmake")],
                    )
                    .in_dir(&root),
                )
            }
            Driver::West => {
                // W5 — overlays reach Zephyr through EXTRA_CONF_FILE and
                // APPLICATION_CONFIG_DIR. Never CONF_FILE: that suppresses
                // Zephyr's own boards/ and socs/ discovery entirely.
                // The application is resolved FIRST: its directory is where a
                // Zephyr app keeps its own `prj-*.conf`, so the overlay search
                // needs it (issue 0892).
                let app = west_application_dir(
                    &image_id,
                    &image,
                    descriptor,
                    &found,
                    &catalog,
                    &bringup_dirs,
                )?
                .unwrap_or_else(|| bringup_dir.clone());
                let overlays =
                    crate::builder::zephyr::resolve_in(&bringup_dir, Some(&app), &board, &image)
                        .map_err(|e| eyre::eyre!("{e}"))?;
                // issue 0892 — Zephyr is not like the other drivers, and the
                // handoff has to say so.
                //
                // cargo and cmake let us OWN the root: we generate it from the
                // images and hand the tool a directory we wrote. west does not
                // work that way. The user owns the west workspace (`.west/`,
                // `zephyr/`, `modules/nano-ros/`, `apps/`), the application is a
                // stock Zephyr app whose `prj.conf` and `CMakeLists.txt` carry
                // authored Kconfig no image declaration expresses (RFC-0065
                // D5), and `west` refuses to run outside that workspace.
                //
                // So: point at the real APPLICATION, run from the USER's
                // workspace, and when that workspace cannot be found, print the
                // command instead of emitting one that cannot work. That last
                // part is `nros setup --system`'s sudo boundary applied here —
                // compose the command, hand it over, do not pretend.
                // `west build` has two argument zones and our single `--` can
                // only name one, so the passthrough is SPLIT by west's own flag
                // list rather than dropped whole into the second zone: that put
                // `-- --pristine` in front of cmake, which failed as
                // `CMake Error: Unknown argument --pristine`, naming the wrong
                // tool for the user's mistake.
                let (west_extra, cmake_extra) =
                    crate::builder::zephyr::split_native_args(&args.native_args)
                        .map_err(|e| eyre::eyre!("{e}"))?;

                // The board id WEST knows, which is not always the name the
                // image authored — see `BoardDescriptor::west_board`.
                let west_board = descriptor
                    .west_board
                    .clone()
                    .unwrap_or_else(|| board.clone());
                let mut a = vec!["build".to_string(), "-b".to_string(), west_board];
                if overlays.sysbuild {
                    a.push("--sysbuild".to_string());
                }
                a.push(app.display().to_string());
                // AFTER the application path, which looks unusual and is the
                // point. `-p`/`--pristine` takes an OPTIONAL value
                // (`nargs='?'`), so argparse greedily reads whatever follows —
                // put it before the positional and west takes the application
                // path as the pristine mode:
                //
                //   west build: error: argument -p/--pristine: invalid choice:
                //   '…/src/zephyr_entry' (choose from 'auto', 'always', 'never')
                //
                // Placing the user's options last is correct for every flag
                // shape without this code having to model west's argparse
                // arities, and west accepts options after the positional.
                a.extend(west_extra);
                let west_opts = crate::builder::zephyr::west_args(&overlays);
                // RFC-0094 D1 — same seam as the cmake driver, in west's
                // second argument zone. See the note there.
                let resolved_opt: Vec<String> = resolved_dir
                    .as_ref()
                    .map(|d| vec![format!("-DNROS_RESOLVED_DIR={}", d.display())])
                    .unwrap_or_default();
                if !west_opts.is_empty() || !cmake_extra.is_empty() || !resolved_opt.is_empty() {
                    // Everything after `--` is a cmake option for the app.
                    a.push("--".to_string());
                    a.extend(west_opts);
                    a.extend(resolved_opt);
                    a.extend(cmake_extra);
                }
                // Resolved here, ENFORCED at exec. A plan is an answer to
                // "what would you run", and `--dry-run` must be able to answer
                // it from a machine with no west workspace at all — refusing
                // there withholds the very command the message tells the user
                // to run. `plan_builds` is also how the pipeline tests assert
                // driver selection, which needs no workspace either.
                let zbase = zephyr_base(&root, args.zephyr_workspace.as_deref());
                if zbase.is_none() && !args.dry_run {
                    let opts = crate::builder::zephyr::west_args(&overlays);
                    let line = format!(
                        "west {}{}{}",
                        a.join(" "),
                        if opts.is_empty() { "" } else { " -- " },
                        opts.join(" ")
                    );
                    eyre::bail!(
                        "no Zephyr found, so `west build` cannot be run for \
                         `{image_id}`.\n\n\
                         Zephyr differs from the other drivers: YOU own the west \
                         workspace and the application. `west build` needs a \
                         Zephyr — with `ZEPHYR_BASE` set it runs from anywhere, \
                         which is how a FREESTANDING application (one outside the \
                         west workspace) builds.\n\n\
                         Point nros at your workspace:\n\n    \
                         nros build {image_id} --zephyr-workspace <dir>\n\n\
                         where <dir> contains `zephyr/`. Or set it once for the \
                         shell:\n\n    \
                         export NROS_ZEPHYR_WORKSPACE=<dir>\n\n\
                         or run west yourself:\n\n    {line}\n\n\
                         (searched: --zephyr-workspace, $ZEPHYR_BASE, \
                         $NROS_ZEPHYR_WORKSPACE, <workspace>/zephyr-workspace, \
                         ../nano-ros-workspace[-4.4])"
                    );
                }
                // Run from the nros workspace with ZEPHYR_BASE set, which is
                // exactly what `west-fixtures.sh` does and what makes a
                // freestanding application work. No cwd gymnastics: the
                // application path is absolute.
                let h = Handoff::new("west", a).in_dir(&root);
                Some(match &zbase {
                    Some(z) => h.with_env("ZEPHYR_BASE", z.as_os_str()),
                    None => h,
                })
            }
            _ => Some(native_handoff(driver, &root, &bringup_dir, &board, args)),
        };

        out.push(ResolvedBuild {
            configure: cmake_configure.or(cargo_prepare),
            qualified: qual,
            board,
            platform,
            driver,
            handoff,
            // `image` is already merged with `[image_defaults]` by the
            // resolver, so this is the image's effective answer rather than
            // only what its own block spelled.
            rmw: image.rmw.clone(),
            entry_package: Some(want_entry.clone()),
            target: descriptor.target.clone(),
            profile: image.profile.clone(),
        });
    }
    Ok(out)
}

/// phase-445 W4b — stages 2–4 for a single-package Rust leaf, or `None` when
/// `root` is not one (`cmd::leaf_settings::resolve`), leaving every other shape
/// to the workspace road unchanged.
///
/// The image is the leaf's own `[image.<id>]`; the build is the leaf's own
/// manifest, handed its generated settings file:
///
/// ```text
///   cargo build --manifest-path <leaf>/Cargo.toml --config <leaf>/build/<image>/nros-cargo.toml
/// ```
///
/// run from the directory ABOVE the leaf — see `cmd::leaf_settings` for the
/// measured reason (the leaf's own `.cargo/` would double the board's link
/// flags until W6 deletes it).
fn plan_single_package(args: &Args, root: &std::path::Path) -> Result<Option<Vec<ResolvedBuild>>> {
    let Some(nros_root) = args
        .nano_ros_path
        .clone()
        .or_else(|| std::env::var_os("NROS_REPO_DIR").map(PathBuf::from))
        .or_else(|| crate::cmd::ws::autodetect_nano_ros_path(root))
    else {
        // The workspace road reports a missing checkout in its own words.
        return Ok(None);
    };
    let Some(img) = crate::cmd::leaf_settings::resolve(root, &nros_root)? else {
        return Ok(None);
    };
    let qual = plan::qualified(&img.package, &img.image_id);

    // ---- stage 2 — the image is the leaf's one image ---------------------
    for want in &args.images {
        if want != &img.image_id && want != &qual {
            eyre::bail!(
                "`{want}` is not an image of {}: it declares only `{}` (RFC-0098 D3). \
                 Switch board by editing `[image.{}] board` in {} and re-running `nros sync`.",
                root.display(),
                img.image_id,
                img.image_id,
                img.decl.origin_path().display()
            );
        }
    }
    for p in args.packages_select.iter().chain(&args.packages_up_to) {
        if p != &img.package {
            eyre::bail!(
                "`{p}` is not a package here: {} is a single-package example (`{}`).",
                root.display(),
                img.package
            );
        }
    }

    // ---- stage 3 — preflight ----------------------------------------------
    let catalog = crate::orchestration::board_descriptor::BoardCatalog::load(&nros_root)
        .map_err(|e| eyre::eyre!("board catalog under {}: {e}", nros_root.display()))?;
    let Some(descriptor) = crate::cmd::board_facts::resolve_board(&catalog, &img.board) else {
        eyre::bail!("board `{}` is claimed by no board descriptor", img.board);
    };
    let mut missing = crate::builder::preflight::check(descriptor, root, Some(&nros_root));
    // The toolchain half only. preflight's sync probe is a WORKSPACE heuristic
    // (a `src/` with neither `generated/` nor `build/nros/`), and a leaf's
    // `src/` is its crate's sources: it would refuse a leaf with no message
    // dependency and no component, which sync gives nothing to write. What this
    // build actually reads from sync is checked precisely just below.
    missing.retain(|m| m.remedy != "nros sync");
    if !missing.is_empty() {
        eyre::bail!("{}", crate::builder::preflight::report(&missing));
    }
    // RFC-0098 D2 — sync runs before the build. The two things it produces that
    // this build reads: the resolved model (the entity facts) and the generated
    // message crates the manifest path-depends on. Missing either is ONE line
    // naming `nros sync`, never cargo's manifest error four frames down.
    let unsynced: Vec<String> = unsynced_inputs(root, &img);
    if !unsynced.is_empty() {
        eyre::bail!(
            "{} has not been synced — missing {}.\n  Run `nros sync` in {} (RFC-0098 D2), then build again.",
            root.display(),
            unsynced.join(", "),
            root.display()
        );
    }

    // ---- stage 4 — the one settings file ------------------------------------
    let Some(img) = crate::cmd::leaf_settings::write(root, &nros_root, "nros build")? else {
        eyre::bail!(
            "{}: resolved as a single-package leaf, then not",
            root.display()
        );
    };
    eprintln!("nros build:   settings → {}", img.config_path.display());

    let (cwd, mut a) = crate::cmd::leaf_settings::build_command(&img);
    if args.offline {
        a.push("--frozen".to_string());
    }
    a.extend(args.native_args.iter().cloned());
    Ok(Some(vec![ResolvedBuild {
        qualified: qual,
        board: img.board.clone(),
        platform: img.platform.clone(),
        driver: Driver::Cargo,
        handoff: Some(Handoff::new("cargo", a).in_dir(&cwd)),
        rmw: img.decl.rmw.clone(),
        entry_package: Some(img.package.clone()),
        target: img.target.clone(),
        profile: None,
        configure: None,
    }]))
}

/// What a single-package build reads that only `nros sync` writes, and is
/// absent. See [`plan_single_package`].
fn unsynced_inputs(
    root: &std::path::Path,
    img: &crate::cmd::leaf_settings::LeafImage,
) -> Vec<String> {
    let mut out = Vec::new();
    // A leaf with no `[[component]]` has no synthesised launch and so no model;
    // the facts then stay absent by design and nothing is missing.
    if !img.decl.components.is_empty() && crate::leaf_entity_env::leaf_model_facts(root).is_none() {
        out.push("the resolved model under `build/nros/models/`".to_string());
    }
    let Ok(doc) = std::fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|e| e.to_string())
        .and_then(|t| t.parse::<toml::Table>().map_err(|e| e.to_string()))
    else {
        return out;
    };
    for key in ["dependencies", "build-dependencies"] {
        let Some(deps) = doc.get(key).and_then(|d| d.as_table()) else {
            continue;
        };
        for (name, spec) in deps {
            if let Some(p) = spec.get("path").and_then(|p| p.as_str())
                && p.split('/').next() == Some("generated")
                && !root.join(p).join("Cargo.toml").is_file()
            {
                out.push(format!("the generated message crate `{name}` ({p})"));
            }
        }
    }
    out
}

pub fn run(args: Args) -> Result<()> {
    // phase-429 W2 — the codegen version guard, BEFORE planning. `nros build`
    // is RFC-0065's front door and was the one codegen path with no guard on
    // it at all; planning is where the generated root is written, so the check
    // has to precede it rather than sit beside the handoff.
    //
    // Anchor: an explicitly-named nano-ros checkout if the caller gave one —
    // that IS the runtime this build links — else the workspace, which the
    // resolver walks up from.
    let guard_anchor = match (&args.nano_ros_path, &args.workspace) {
        (Some(p), _) => p.clone(),
        (None, Some(ws)) => ws.clone(),
        (None, None) => std::env::current_dir()?,
    };
    crate::abi_guard::check_workspace(&guard_anchor, crate::abi_guard::Verb::Build)?;

    let plans = plan_builds(&args)?;
    pin_this_project(&args)?;
    drive(&plans, args.dry_run, &mut perform)
}

/// RFC-0095 D9 — the first `nros build` writes the pin it used and says so.
///
/// AFTER planning and BEFORE the handoff, and both halves are deliberate. After,
/// because a directory that turns out not to be a workspace should not be left
/// carrying a toolchain pin; before, because stage 5 `exec`s and nothing can run
/// after it (issue 1206's lesson, one line over).
///
/// `--dry-run` writes nothing: a flag whose whole promise is "print, change
/// nothing" cannot be the thing that pins a project.
///
/// The workspace root is resolved the same way [`plan_builds`] resolves it. It
/// is deliberately NOT a second walk: the pin belongs beside the workspace the
/// build actually ran on, and `pin::find` then walks UP from there, which is how
/// `nros build` from a subdirectory finds a root-level pin rather than writing a
/// second one.
fn pin_this_project(args: &Args) -> Result<()> {
    if args.dry_run {
        return Ok(());
    }
    let root = match &args.workspace {
        Some(w) => w.clone(),
        None => std::env::current_dir().wrap_err("resolving cwd as the workspace root")?,
    };
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let Ok(exe) = std::env::current_exe() else {
        return Ok(());
    };
    // RFC-0097 D11 — a pin is a source edit, so an automated session does not
    // make one. The session is DETECTED here and passed in, because
    // `pin_on_first_build` is a pure function of its arguments and its tests
    // must not depend on whether the machine running them happens to set `$CI`
    // (which, in this repo's own CI, it does).
    let session = nros_launcher::session::Session::detect();
    let outcome = crate::orchestration::pin::pin_on_first_build(&root, &exe, &session)?;
    if let Some(line) = report_pin_outcome(&outcome)? {
        eprintln!("{line}");
    }
    Ok(())
}

/// Turn a [`PinOutcome`](crate::orchestration::pin::PinOutcome) into what the
/// build does about it: a line to print, nothing, or an ERROR.
///
/// Split out from [`pin_this_project`] so RFC-0097 D11's *build* half is
/// reachable by a test. It is the half that is easy to get wrong and impossible
/// to see: the refusal already exists as a value one crate over
/// (`PinOutcome::RefusedInCi`) and `toolchain_pin_dispatch.rs` covers it
/// thoroughly — but a caller that printed it as a warning and carried on would
/// pass every one of those tests while doing exactly the thing D11 forbids.
/// Without this seam the only way to that code path is a full `nros build`
/// against a real store, which is not a test anyone would keep.
pub(crate) fn report_pin_outcome(
    outcome: &crate::orchestration::pin::PinOutcome,
) -> Result<Option<String>> {
    let line = crate::orchestration::pin::describe(outcome);
    if outcome.is_refusal() {
        // Not a warning. Proceeding would build against a toolchain nobody
        // recorded, which is precisely the unreproducible build the pin exists
        // to prevent — and a CI job that goes green on it has hidden the
        // problem rather than found it.
        eyre::bail!(
            "{}",
            line.unwrap_or_else(|| "refusing to write a toolchain pin".to_string())
        );
    }
    Ok(line)
}

/// How one plan's native command reaches the operating system.
///
/// `exec` is RFC-0065 D1's guarantee and it can be spent exactly once per
/// invocation, because after it this process no longer exists. That is the
/// whole of issue 1206: [`run`]'s loop execed inside itself, so a plan set of
/// N produced one build and an exit code that was image 1's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Handover {
    /// Run it as a child and wait. Stdio is INHERITED, never piped.
    Wait,
    /// Replace this process. Never returns on success — so this is the LAST
    /// thing the invocation does, and only the last plan may get it.
    Exec,
}

/// The handover the plan at `index` of `total` gets.
///
/// Stated as a function of position rather than inlined, so "only the last
/// plan execs" is a thing a test can ask about directly.
#[must_use]
fn handover_for(index: usize, total: usize) -> Handover {
    if index + 1 == total {
        Handover::Exec
    } else {
        Handover::Wait
    }
}

/// Perform one handover for real.
///
/// The seam [`drive`] is tested through: a test substitutes a recorder here so
/// it can observe every plan's handover without a compiler, an exec, or a
/// process that stops existing halfway through the assertions.
fn perform(hand: &Handoff, mode: Handover) -> Result<()> {
    match mode {
        Handover::Wait => {
            let st = crate::builder::handoff::wait(hand).map_err(|e| eyre::eyre!("{e}"))?;
            if !st.success() {
                eyre::bail!("`{}` exited {}", hand.display(), st);
            }
            Ok(())
        }
        // Never returns on success: this process BECOMES the build.
        Handover::Exec => {
            let err = crate::builder::handoff::exec(hand).unwrap_err();
            eyre::bail!("{err}")
        }
    }
}

/// Stage 5 for every plan: announce it, then hand it over.
///
/// Split out of [`run`] for one reason — the loop is the thing that was wrong
/// (issue 1206) and it was the one part of `nros build` no test could reach.
/// `plan_builds` is pure and heavily tested; `run` needs a real workspace on
/// disk; this needs neither.
///
/// `handover` performs one plan's command. Production passes [`perform`].
fn drive(
    plans: &[ResolvedBuild],
    dry_run: bool,
    handover: &mut dyn FnMut(&Handoff, Handover) -> Result<()>,
) -> Result<()> {
    let total = plans.len();
    for (i, p) in plans.iter().enumerate() {
        // The counter is the ONLY progress a multi-image build can report.
        // The last plan execs, so no epilogue of any kind can run after it —
        // a user who sees `[3/7]` as the last line knows where it stopped, and
        // before this there was nothing that distinguished "built 7" from
        // "built 1 and said nothing about the other 6".
        let where_ = if total > 1 {
            format!("[{}/{total}] ", i + 1)
        } else {
            String::new()
        };
        eprintln!(
            "nros build: {where_}{} -> board {} (platform {}), driver {}",
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
        if dry_run {
            if let Some(cfg) = &p.configure {
                println!("{}", cfg.display());
            }
            println!("{}", hand.display());
            continue;
        }
        // The configure, for drivers that need one (cmake). Runs HERE rather
        // than during planning so `plan_builds` stays side-effect free — the
        // property that makes `--dry-run` trivially correct instead of a second
        // code path. It is a subprocess, not an exec: the exec below has to
        // survive it.
        if let Some(cfg) = &p.configure {
            run_configure(cfg)?;
        }
        let outcome = handover(hand, handover_for(i, total));
        if total > 1 {
            // Which image failed, and that the rest are NOT built — the two
            // facts a single-image invocation never had to say and a
            // multi-image one cannot leave to the reader.
            outcome.wrap_err_with(|| {
                format!(
                    "building `{}` (image {} of {total}) failed; nothing after \
                     it was attempted",
                    p.qualified,
                    i + 1
                )
            })?;
        } else {
            outcome?;
        }
    }
    Ok(())
}

/// Run a configure step, escalating an `--offline` failure to one
/// online retry.
///
/// `--offline` is an OPTIMIZATION on this path, never a semantic
/// choice. The step resolves a lock for a root this process just
/// generated, and issue 0676's frozen property belongs to the BUILD,
/// which stays `--locked` either way — so offline buys "do not touch
/// the network when the answer is already local", and nothing else.
///
/// Offline resolution fails for exactly one reason a retry fixes: the
/// registry cache does not hold some member's dependency yet. On CI
/// that cache is cold every run, which is why all three nightly
/// platform lanes died here — on `esp-backtrace` and
/// `panic-semihosting`, dependencies of workspace MEMBERS the built
/// platform never uses, reached only because `examples/workspaces/rust`
/// is one cargo workspace and resolving it resolves every member
/// (issue 0873). Reported as five OVERCLAIMED platforms, which is a
/// claim about the platforms rather than about our registry cache.
///
/// The retry cannot change WHAT resolves: the offline cache is a subset
/// of the registry, so a lock that resolves offline resolves identically
/// online. It changes only whether resolution can happen at all. A
/// genuinely missing package still fails, now with the registry's own
/// error instead of cargo's "offline mode (via `--offline`) can
/// sometimes cause surprising resolution failures" note.
fn run_configure(cfg: &Handoff) -> Result<()> {
    let st = cfg
        .command()
        .status()
        .wrap_err_with(|| format!("running `{}`", cfg.display()))?;
    if st.success() {
        return Ok(());
    }

    let Some(online) = cfg.without_offline() else {
        eyre::bail!("configure failed: `{}` exited {}", cfg.display(), st);
    };
    eprintln!(
        "nros build: warning: `{}` failed against a cold registry cache; \
         retrying online (issue 0873)",
        cfg.display()
    );
    let st = online
        .command()
        .status()
        .wrap_err_with(|| format!("running `{}`", online.display()))?;
    if !st.success() {
        eyre::bail!("configure failed: `{}` exited {}", online.display(), st);
    }
    Ok(())
}

/// The capability axes a bringup's `system.toml` turns on — the axes a missing
/// selection facade would silently drop (phase-413 W2).
///
/// Empty when the file is absent or unparseable ON PURPOSE: this decides whether
/// to ESCALATE a warning into a hard error, and a system that cannot be read is
/// not evidence that a capability is declared. Whatever is wrong with it will be
/// reported by the code whose job that is, with better words than this has.
fn declared_capabilities(bringup_dir: &std::path::Path) -> Vec<&'static str> {
    let Ok(raw) = std::fs::read_to_string(bringup_dir.join("system.toml")) else {
        return Vec::new();
    };
    let Ok(sys) = toml::from_str::<crate::orchestration::cargo_metadata_schema::SystemToml>(&raw)
    else {
        return Vec::new();
    };
    cargo_nano_ros::capability_resolver::CAPABILITIES
        .iter()
        .filter(|c| sys.capability_enabled(c.declared))
        .map(|c| c.declared)
        .collect()
}

/// A Path A bringup: a `system.toml` in a directory with no build file of its
/// own. A PACKAGE with a `system.toml` beside it is a single-package example
/// (RFC-0098 D3), not a bringup of the workspace around it.
fn is_bringup_dir(dir: &std::path::Path) -> bool {
    dir.join(nros_orchestration_ir::leaf_system::SYSTEM_TOML)
        .is_file()
        && !nros_orchestration_ir::leaf_system::is_package_dir(dir)
}

/// Package mode (phase-445 W5): one plan per package of a bringup-less
/// workspace, in the dependency order stage 1 already computed — the colcon
/// shape RFC-0065 D1 names, with RFC-0098 D9's "no root build file".
///
/// * a CMake package gets a generated root at `build/<pkg>/CMakeLists.txt`
///   (the same emitter an image uses, `builder::cmake_root`, with this package
///   as its only subdir and no SYSTEM) — the nano-ros setup, the workspace's
///   interface search path and the rclcpp compat surface that a hand-written
///   umbrella used to spell out — configured into `build/<pkg>/cmake`;
/// * a cargo package is built by cargo FROM ITS OWN DIRECTORY, so its own
///   `.cargo/config.toml` and `[patch]` rows apply exactly as `cd <pkg> &&
///   cargo build` would, into `build/<pkg>/target`;
/// * an interface package is not built on its own: its bindings are generated
///   into the packages that use it (`nros sync`, and the Find-stub on the cmake
///   side), which is why the hand-written roots never built it either.
fn plan_packages(
    root: &std::path::Path,
    found: &crate::builder::discover::Discovered,
    args: &Args,
    nano_ros_root: Option<&std::path::Path>,
) -> Result<Vec<ResolvedBuild>> {
    let mut out = Vec::new();
    let shown = |p: &std::path::Path| {
        p.strip_prefix(root)
            .map(|r| r.display().to_string())
            .unwrap_or_else(|_| p.display().to_string())
    };
    for pkg in &found.packages {
        if crate::interface_package::dir_is_interface_package(&pkg.dir) {
            eprintln!(
                "nros build: {} — interface package; its bindings are generated into the \
                 packages that use it",
                pkg.name
            );
            continue;
        }
        let routing = crate::routing::route(pkg);
        let build_dir = root.join("build").join(&pkg.name);
        let (driver, configure, handoff) = if routing.cmake_subdir {
            let Some(nros_root) = nano_ros_root else {
                eyre::bail!(
                    "no nano-ros checkout found, so `{}`'s cmake root cannot be generated. \
                     Pass --nano-ros-path, or set NROS_REPO_DIR.",
                    pkg.name
                );
            };
            // Every other package is excluded: this root builds ONE package
            // (its interface dependencies resolve through the Find-stub, and
            // its cmake dependencies are its own `find_package` calls).
            let excluded = found
                .packages
                .iter()
                .filter(|p| p.dir != pkg.dir)
                .map(|p| p.dir.clone())
                .collect();
            let spec = crate::builder::cmake_root::CmakeRootSpec {
                entries: Vec::new(),
                workspace: root.to_path_buf(),
                system: String::new(),
                platform: "posix".to_string(),
                board: None,
                rmw: "zenoh".to_string(),
                toolchain_file: None,
                nano_ros_root: nros_root.to_path_buf(),
                excluded,
            };
            crate::builder::cmake_root::write(found, &build_dir, &spec)
                .map_err(|e| eyre::eyre!("`{}`: {e}", pkg.name))?;
            let rel = shown(&build_dir);
            let mut a = vec![
                "-S".to_string(),
                rel.clone(),
                "-B".to_string(),
                format!("{rel}/cmake"),
            ];
            a.extend(args.native_args.iter().cloned());
            (
                Driver::CMake,
                Some(Handoff::new("cmake", a).in_dir(root)),
                Handoff::new("cmake", vec!["--build".to_string(), format!("{rel}/cmake")])
                    .in_dir(root),
            )
        } else if routing.cargo_member {
            let target_dir = build_dir.join("target").display().to_string();
            let mut a = vec!["build".to_string(), "--target-dir".to_string(), target_dir];
            if args.offline {
                a.push("--frozen".to_string());
            }
            a.extend(args.native_args.iter().cloned());
            // A package with no lock of its own gets one resolved first, as a
            // generated entry does: the PATH shim's project-wide `--locked`
            // cannot create one, and a lock-less leaf is legitimate (its
            // `generated/` message crates are per host, so its lock is not
            // committed — CLAUDE.md, `check-leaf-lockfiles`).
            let configure = (!pkg.dir.join("Cargo.lock").is_file()).then(|| {
                let mut u = vec!["update".to_string(), "--workspace".to_string()];
                if args.offline {
                    u.push("--offline".to_string());
                }
                Handoff::new("cargo", u)
                    .in_dir(&pkg.dir)
                    .with_env("NROS_CARGO_FLAGS", "")
            });
            (
                Driver::Cargo,
                configure,
                Handoff::new("cargo", a).in_dir(&pkg.dir),
            )
        } else {
            continue;
        };
        out.push(ResolvedBuild {
            qualified: pkg.name.clone(),
            board: "native".to_string(),
            platform: "posix".to_string(),
            driver,
            handoff: Some(handoff),
            rmw: None,
            entry_package: None,
            target: None,
            profile: None,
            configure,
        });
    }
    if out.is_empty() {
        eyre::bail!(
            "no buildable package under {} — a workspace with no bringup builds each \
             package with its own build file, and none carries a Cargo.toml or \
             CMakeLists.txt it can build",
            root.display()
        );
    }
    Ok(out)
}

/// Generate the entry package for a cargo image (W3.b), returning its
/// directory. `None` when the launch tree cannot be resolved — reported as a
/// warning rather than a failure, because a workspace whose entries are still
/// hand-written must keep building through the migration (RFC-0065 D13).
//
// Eight arguments: this is the entry generator's whole input, every parameter is
// a distinct fact about the image being generated, and bundling them into a
// struct would move the argument list rather than shorten it.
//
// This allow is not new. It has been here since the eighth argument arrived, and
// phase-413 W2 inserted `declared_capabilities` BETWEEN it and this function, so
// both it and the doc comment above re-attached to that one-argument function —
// silently, because a detached attribute is still valid syntax on whatever
// follows it. Clippy then reported `too_many_arguments` against `generate_entry`,
// naming the item that LOST the attribute and never the insertion that took it.
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
) -> Result<GeneratedEntry> {
    use crate::{
        builder::entry::{BoardFacts, EntrySpec},
        orchestration::model_location,
    };

    let Some(nros_root) = nano_ros_root else {
        return Ok(GeneratedEntry::default());
    };

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
            return Ok(GeneratedEntry::default());
        }
    };
    let model_path = match model_location::ensure_model(bringup_dir, &model_rel) {
        Ok((p, _inputs)) => p,
        Err(e) => {
            eprintln!("nros build: warning: cannot resolve the model for `{image_id}`: {e}");
            return Ok(GeneratedEntry::default());
        }
    };

    // phase-392 W5 — the ENTITY facts, from the SAME model, ONE read. Computed
    // BEFORE the hand-written check below, because a hand-written entry is
    // built with the image's settings file too (RFC-0098 D7): the facts belong
    // to the IMAGE, whoever wrote its `main`. The fixture lane used to fetch
    // them for exactly those entries with a second `nros ws entity-facts`
    // invocation and export them per build — a second carrier, now retired.
    let entity_facts = entity_facts_at(&model_path);

    // A workspace that still has its hand-written entry keeps it. Generating a
    // second one would be redundant at best and a conflicting `[[bin]]` name at
    // worst — and D13's migration is a DELETION: remove the hand-written entry
    // and the next build generates it. This is what makes the migration
    // incremental, one entry at a time.
    // Keyed on the MANIFEST, not the directory. `git rm -r src/<entry>` leaves
    // gitignored residue behind — `.cargo/` holds the sync-written sidecar —
    // so a directory-existence check reads a deleted entry as still present and
    // silently generates nothing. phase-383 W10 tripped over exactly that on
    // the first workspace it tried.
    let want = crate::builder::entry::package_name(image_id);
    let hand_written = root.join("src").join(&want);
    if hand_written.join("Cargo.toml").is_file() || hand_written.join("CMakeLists.txt").is_file() {
        return Ok(GeneratedEntry {
            dir: None,
            entity_facts,
        });
    }

    let plan = match crate::codegen::entry::plan_from_model(&model_path, image.board.clone()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("nros build: warning: cannot plan `{image_id}`: {e}");
            return Ok(GeneratedEntry {
                dir: None,
                entity_facts,
            });
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
    // DEFERRED, not immediate — the fatal below fires AFTER the entry package is
    // written. See the block comment there.
    let mut facade_missing_fatal: Option<String> = None;
    let facade_dir = {
        let d = root
            .join("generated/nros-selection")
            .join(crate::builder::entry::package_name(image_id));
        if d.is_dir() {
            Some(d)
        } else {
            // SAY SO. The facade carries the RMW, the ROS edition and the
            // capability features, and the Entry is emitted without any of them
            // when it is missing — silently, until the build fails somewhere
            // that names none of it. Issue 0937 was one of these: the NuttX
            // Entry reached the tree's single `#[global_allocator]` only through
            // the facade, so a missing facade surfaced as
            //
            //     error: no global memory allocator found but one is required
            //
            // several hundred lines into a cross build, and the nightly cell
            // stayed red while the same Entry built fine in the workspace next
            // door, which happened to have one.
            //
            // WARN or FAIL, decided by what the facade would have CARRIED —
            // phase-413 W2.
            //
            // "An unsynced workspace is a tolerable state" is true exactly when
            // the facade adds nothing the Entry cannot do without. It is false
            // the moment the system declares a CAPABILITY: those reach the
            // Entry only through the facade's `nros` feature list, so building
            // without it produces an Entry that provably cannot be correct, and
            // the failure lands somewhere that names neither sync nor the
            // facade. `host-tests` spent four runs on the far end of that:
            //
            //   error[E0080]: evaluation panicked: this system declares
            //   `[param_services]` but this `nros` build does not carry the
            //   `param-services` feature
            //     --> build/posix-zenoh/native_rust_qos_entry/src/main.rs:9:1
            //
            // — a const-eval panic in a generated `main.rs`, six lines below
            // this very warning in the same log. The warning was right and
            // nobody read it, which is what a warning is worth on a build that
            // continues.
            //
            // The RMW and ROS edition USED to stay a warning, on the grounds
            // that "both have defaults the Entry can build against". Issue 0831
            // took that away and the warning did not follow.
            //
            // The board crate's `default = ["rmw-zenoh"]` was that default. It
            // is gone: 0831 made the facade the ONE place that names the RMW,
            // so `builder::entry` now emits the board dep
            // `default-features = false` (cargo cannot subtract a default —
            // issue 0270 — so both declarations have to be silent about it).
            // Measured on this tree, `[image.native_cyclonedds] rmw =
            // "cyclonedds"` with no facade:
            //
            //     nros-board-linux = { path = "…", default-features = false }
            //
            // No `rmw-*` at all, and no `ethernet` / `image-runtime` either —
            // the facade is what re-supplies the board's non-RMW defaults. So
            // the Entry that warning lets through has NO backend, NO ROS
            // edition and none of the board's own defaults, while the message
            // says "nothing else is lost". That is 0831's shape exactly, one
            // door over: `[image.<id>].rmw` is declared, cannot take effect,
            // and the build says so in a line that reads like reassurance.
            //
            // So there is no tolerable case left, and the split goes away. The
            // heal below is what makes that affordable — it WRITES the facade
            // from the entry just generated, so nothing is refused that can be
            // repaired, and only a facade that cannot be written is fatal.
            // `declares_capability` survives because it makes the refusal name
            // the right remedy.
            let declares_capability = declared_capabilities(bringup_dir);

            if !declares_capability.is_empty() {
                // RECORDED, and raised after `builder::entry::write` below.
                //
                // Bailing HERE deadlocks a fresh checkout, and it did: this
                // check shipped as an immediate `bail!` and `host-tests` went
                // from a const-eval panic to
                //
                //   Error: nros build: `native_rust_qos` needs a selection
                //   facade and there is none at …
                //
                // which is this message — correct, and unescapable. `nros sync`
                // writes a facade per ENTRY PACKAGE, and the entry package is
                // GENERATED BY THIS FUNCTION: the workspace root lists
                // `build/posix-zenoh/<entry>` as a cargo member, but the
                // directory is untracked, so on a clone sync finds no
                // `Cargo.toml` there and skips it (`ws.rs`: `if
                // !cargo_toml.is_file() { continue; }`). Bail before writing
                // the entry and sync can never see it, so the facade can never
                // exist, so the build can never proceed.
                //
                // The pre-existing WARNING was self-healing for exactly this
                // reason — it wrote a featureless Entry, the next sync saw the
                // package and produced the facade, and the build after that was
                // correct. That is what "an unsynced workspace is a state the
                // build is documented to tolerate" bought, and turning the
                // warning into an immediate error spent it without noticing.
                //
                // So: generate the entry (cheap, and it is what unblocks sync),
                // THEN refuse to go further. Loud at the point of decision, and
                // the remedy it names actually works.
                facade_missing_fatal = Some(format!(
                    "nros build: `{image_id}` needs a selection facade and there is none at {}.\n\
                     \n\
                     Its system declares {} — capabilities reach the Entry ONLY through the\n\
                     facade's `nros` feature list, so building without it yields an Entry that\n\
                     cannot be correct. The failure would surface later as a const-eval panic\n\
                     in a generated `main.rs` naming neither sync nor this facade (issue 0937).\n\
                     \n\
                     The entry package HAS been generated, so `nros sync` can see it now.\n\
                     Run `nros sync` in the workspace, then build again (or `just build\n\
                     <scope>`, which runs codegen for you).",
                    d.display(),
                    declares_capability
                        .iter()
                        .map(|c| format!("`[{c}]`"))
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
            } else {
                // Issue 0831. Same deferral, same self-heal, different reason:
                // the facade carries the RMW this image DECLARED and the ROS
                // edition, and since 0831 the Entry has no other way to get
                // either.
                facade_missing_fatal = Some(format!(
                    "nros build: `{image_id}` needs a selection facade and there is none at {}.\n\
                     \n\
                     The facade is the one place that names {} and the ROS edition: the\n\
                     generated entry declares its board crate `default-features = false`\n\
                     precisely so the facade can name the backend once (issue 0831), so\n\
                     without it the entry links NO RMW backend, no ROS edition and none of\n\
                     the board's own defaults. It would compile and then refuse to select a\n\
                     backend at run time, in a build directory named for the one it declared.\n\
                     \n\
                     The entry package HAS been generated, so `nros sync` can see it now.\n\
                     Run `nros sync` in the workspace, then build again (or `just build\n\
                     <scope>`, which runs codegen for you).",
                    d.display(),
                    match image.rmw.as_deref() {
                        Some(r) => format!("this image's `rmw = \"{r}\"`"),
                        None => "the bringup's `[system] rmw`".to_string(),
                    },
                ));
            }
            None
        }
    };

    // Most specific first: the image id IS the deploy key when an image is
    // named after a board, but `[image.native_service_server]` is not, and
    // `[image.robot1]` is not — so the board and platform back it up. ONE list,
    // consumed by both the deploy token and the board crate, because the macro
    // resolves the crate FROM the token: two searches could disagree, and the
    // disagreement is a generated entry that does not compile.
    let board_name = image.board.clone().unwrap_or_default();
    // BOARD first, then the image id, then the platform.
    //
    // The board is what the user DECLARED; the image id is a label that may or
    // may not happen to be a board token. Taking the id first resolved
    // `[image.freertos] board = "mps2-an385-freertos"` to the generic
    // `freertos` board — a real board, so nothing failed until the link, where
    // the mps2 board's lwIP glue was simply absent:
    //
    //   undefined reference to `lwip_setsockopt' … `lwip_socket_thread_init'
    //
    // The hand-written entry said `DEPLOY mps2-an385-freertos`. A generated one
    // must not quietly pick a different board than the image names — that is
    // issue 0798 with the roles reversed.
    let candidates = [board_name.as_str(), image_id, platform];

    // Where the entry will be written — needed to make its dependency paths
    // relative, and computed before the spec because the spec carries them.
    let entry_dir_for_deps = root
        .join("build")
        .join(coordinate(platform, image))
        .join(crate::builder::entry::package_name(image_id));

    let mut spec = EntrySpec {
        image_id: image_id.to_string(),
        launch,
        args: image.args.clone(),
        panic: image.panic.clone(),
        nodes,
        nano_ros_root: nros_root.to_path_buf(),
        facade_dir,
        // A `[[bridge]]` in the bringup makes the macro emit a call into
        // `nros_bridge`; nothing in the package graph implies it, and the
        // hand-written bridge entries listed it by hand.
        bringup_deps: {
            let mut v = Vec::new();
            let toml_path = bringup_dir.join("system.toml");
            if std::fs::read_to_string(&toml_path)
                .ok()
                .and_then(|t| t.parse::<toml::Value>().ok())
                .and_then(|d| {
                    d.get("bridge")
                        .map(|b| b.as_array().is_some_and(|a| !a.is_empty()))
                })
                .unwrap_or(false)
            {
                let rel = crate::builder::paths::relative_or_err(
                    &entry_dir_for_deps,
                    &nros_root.join("packages/rmw/bridge"),
                )
                .map_err(|e| eyre::eyre!("{e}"))?;
                v.push(format!(
                    "nros-bridge = {{ path = \"{rel}\", features = [\"std\", \"config\"] }}"
                ));
            }
            v.extend(
                bridge_entry_deps(bringup_dir, &entry_dir_for_deps, nros_root, &candidates)
                    .map_err(|e| eyre::eyre!("{e}"))?,
            );
            v
        },
    };
    let facts = BoardFacts::from_descriptor_for(descriptor, &candidates);
    let parent = root.join("build").join(coordinate(platform, image));
    let dir = crate::builder::entry::write(&spec, &facts, &parent)
        .map_err(|e| eyre::eyre!("generating the entry for `{image_id}`: {e}"))?;

    // NOW refuse — the entry package exists, so `nros sync` can produce the
    // facade this build needed. Raising it earlier deadlocks a clone: sync
    // keys facades off the entry package, and this function is what writes it.
    if let Some(msg) = facade_missing_fatal {
        // SELF-HEAL, because CI only ever gets one pass.
        //
        // Deferring the refusal until after the entry was written (the previous
        // fix) made a DEVELOPER's tree recoverable: sync could finally see the
        // entry package, so the next `nros build` succeeded. It did nothing for
        // CI, and `host-tests` proved it — the run does
        //
        //     just _codegen        (sync: no entry yet, so no facade)
        //     nros build           (writes the entry, then refuses)
        //
        // and there is no second sync, so the lane failed on the same message
        // with the fix in place. A fresh checkout every run means the two-pass
        // recovery never happens.
        //
        // But nothing here actually needs sync. `write_facade` wants the entry
        // name, its directory, its manifest, the system and the facade root —
        // and we hold all five, because we just generated the entry. The one
        // input sync was missing is the thing this function produces.
        //
        // So: write the facade, then regenerate the entry with it. The second
        // write is not optional — the first ran with `facade_dir = None`, so
        // the entry carries no dependency on the facade crate, and a facade no
        // entry depends on selects nothing (cargo feature unification is the
        // whole mechanism). Two file writes, one build, no second invocation.
        //
        // Still fatal if the facade cannot be written: that is a real problem
        // and the message already names it.
        //
        // Issue 0831 — this now runs for EVERY missing facade, not only the
        // capability case. Once the board dep is emitted
        // `default-features = false`, a facade-less entry has no backend at
        // all, so there is nothing left for a warning to be tolerant of; and
        // healing is two file writes against inputs this function already
        // holds, so nothing that can be repaired is refused.
        //
        // The heal's own failure is REPORTED, not swallowed. It used to be
        // `.ok().flatten()`, which was tolerable while this ran only for a
        // workspace that declares capabilities; now that every missing facade
        // comes through here, "could not write it" is a message a user will
        // actually meet, and `resolve_rmw` refusing an unknown `rmw` is one of
        // the ways to get it. Naming sync when the real answer is a typo in
        // `[image.<id>] rmw` is the diagnostic this issue is about.
        let facade_root = root.join("generated/nros-selection");
        let entry_name = crate::builder::entry::package_name(image_id);
        let healed = std::fs::read_to_string(bringup_dir.join("system.toml"))
            .map_err(|e| eyre::eyre!("reading {}: {e}", bringup_dir.join("system.toml").display()))
            .and_then(|raw| {
                toml::from_str::<crate::orchestration::cargo_metadata_schema::SystemToml>(&raw)
                    .map_err(|e| eyre::eyre!("parsing the bringup's system.toml: {e}"))
            })
            .and_then(|sys| {
                crate::orchestration::facade::write_facade(
                    &entry_name,
                    &dir,
                    &dir.join("Cargo.toml"),
                    &sys,
                    &facade_root,
                )
            });

        let healed = match healed {
            Ok(Some(f)) => Some(f),
            Ok(None) => None,
            Err(e) => eyre::bail!("{msg}\n\nWriting it here failed too: {e}"),
        };
        let Some(_) = healed else {
            eyre::bail!("{msg}");
        };
        eprintln!(
            "nros build: wrote the missing selection facade for `{image_id}` from the entry \
             just generated, and regenerated the entry against it."
        );
        spec.facade_dir = Some(facade_root.join(&entry_name));
        crate::builder::entry::write(&spec, &facts, &parent)
            .map_err(|e| eyre::eyre!("regenerating the entry for `{image_id}`: {e}"))?;
    }

    Ok(GeneratedEntry {
        dir: Some(dir),
        entity_facts,
    })
}

/// What [`generate_entry`] produced for one image.
#[derive(Debug, Default)]
struct GeneratedEntry {
    /// The generated entry package, or `None` when a hand-written one
    /// suppressed generation or the launch could not be resolved here.
    dir: Option<PathBuf>,
    /// The image's entity facts (`NROS_DECLARED_*`), for its settings file.
    entity_facts: std::collections::BTreeMap<String, String>,
}

/// phase-392 W5 — the ENTITY facts of the model at `model_path`.
///
/// Until phase-392 only `cmake/NanoRosEntityFacts.cmake` delivered them, so the
/// CARGO path silently kept the undeclared fallback (32 hosted / 8 embedded) —
/// a hard failure on a small target: `esp32_entry` overflowed DRAM by 8,804 B
/// carrying `SERVICE_BUFFERS` sized for 8 service servers it does not have.
/// The consumer (`nros-zpico-build`) derives `ZPICO_MAX_QUERYABLES` from these;
/// they are CARRIED, never the count (issue 0460). Since RFC-0098 D7 they go in
/// the image's settings file rather than the handoff's environment.
fn entity_facts_at(model_path: &std::path::Path) -> std::collections::BTreeMap<String, String> {
    match std::fs::read_to_string(model_path)
        .map_err(|e| e.to_string())
        .and_then(|y| {
            ros_launch_manifest_model::SystemModel::from_yaml_str(&y).map_err(|e| e.to_string())
        }) {
        Ok(m) => crate::cmd::entity_facts::facts_from_model(&m),
        Err(e) => {
            // Not fatal: the undeclared fallback is the historical behaviour.
            // Say so rather than silently sizing for 8.
            eprintln!(
                "nros build: warning: cannot read `{}` for entity facts, so the \
                 backend keeps its undeclared table budget: {e}",
                model_path.display()
            );
            std::collections::BTreeMap::new()
        }
    }
}

/// Issue 0827 — the pool knobs stage 3.5 DERIVED for this image, as `[env]`
/// rows for its settings file. Empty when the resolve refused (the common case:
/// only images whose launch carries a contract sidecar describe wiring).
///
/// Rendered by `leaf_entity_env::render_env_sidecar` and read back, not
/// re-spelled here: that renderer is where the knob NAMES, the consumer floors
/// (issue 1015) and the deliberate omission of `ZPICO_MAX_QUERYABLES` live, and
/// a second list would be the drift this repository keeps paying for. Payload
/// classes and the take buffer are not derived on this road (both need a
/// leaf's message-bound inventory), so the crate defaults stand for them.
fn derived_pool_env(
    resolved: &crate::resolve::Resolved,
) -> std::collections::BTreeMap<String, String> {
    let Some(knobs) = resolved.knobs() else {
        return std::collections::BTreeMap::new();
    };
    const NOT_DERIVED: &str = "not derived for a workspace image (no leaf message-bound inventory)";
    let body = crate::leaf_entity_env::render_env_sidecar(
        knobs,
        &crate::leaf_payload_classes::PayloadClasses::Refused {
            reason: NOT_DERIVED.to_string(),
        },
        &crate::leaf_take_buffer::TakeBuffer::Refused {
            reason: NOT_DERIVED.to_string(),
        },
        &resolved.source,
    );
    body.parse::<toml::Value>()
        .ok()
        .and_then(|v| v.get("env").and_then(|e| e.as_table()).cloned())
        .map(|t| {
            t.into_iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// The `[patch.crates-io]` rows an image's cargo graph needs, as crate name →
/// absolute crate root.
///
/// Walks the entry manifest and, through PATH dependencies, every package it
/// reaches inside the workspace (`src/*`, `generated/*`), collecting the deps
/// each names REGISTRY-style. A name that is a generated message crate of this
/// workspace, or an in-repo nano-ros crate, gets a row; anything else is a real
/// registry crate and gets none. Only names the graph actually spells
/// registry-style, so cargo never warns about an unused patch — the same rule
/// sync's managed block follows (RFC-0067 D1).
///
/// The board descriptor's own `[patch]` rows (the NuttX `libc` fork) are merged
/// by `cargo_config::render`, not here.
pub(crate) fn registry_patches(
    ws_root: &std::path::Path,
    nano_ros_root: &std::path::Path,
    entry_dir: &std::path::Path,
) -> std::collections::BTreeMap<String, PathBuf> {
    let mut out = std::collections::BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut queue = vec![entry_dir.to_path_buf()];
    while let Some(dir) = queue.pop() {
        let dir = dir.canonicalize().unwrap_or(dir);
        if !seen.insert(dir.clone()) {
            continue;
        }
        let Ok(body) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
            continue;
        };
        for name in crate::cmd::ws::registry_style_dep_names(&body) {
            let generated = ws_root.join("generated").join(&name);
            if generated.join("Cargo.toml").is_file() {
                out.insert(name, generated);
            } else if let Some(sub) = crate::cmd::ws::nros_crate_subpath(&name) {
                let root = nano_ros_root.join(sub);
                if root.join("Cargo.toml").is_file() {
                    out.insert(name, root);
                }
            }
        }
        // Follow path deps, but only inside the workspace: the nano-ros crates
        // path-depend on each other and never registry-name an in-repo crate
        // a leaf would need patched.
        let Ok(doc) = body.parse::<toml::Value>() else {
            continue;
        };
        let mut tables: Vec<&toml::Value> = ["dependencies", "build-dependencies"]
            .iter()
            .filter_map(|k| doc.get(*k))
            .collect();
        if let Some(targets) = doc.get("target").and_then(|t| t.as_table()) {
            for t in targets.values() {
                tables.extend(
                    ["dependencies", "build-dependencies"]
                        .iter()
                        .filter_map(|k| t.get(*k)),
                );
            }
        }
        for deps in tables.into_iter().filter_map(|t| t.as_table()) {
            for spec in deps.values() {
                let Some(p) = spec.get("path").and_then(|p| p.as_str()) else {
                    continue;
                };
                let next = dir.join(p);
                let next = next.canonicalize().unwrap_or(next);
                if next.starts_with(ws_root) {
                    queue.push(next);
                }
            }
        }
    }
    out
}

/// The crates a BRIDGE entry must name directly, derived from the bringup.
///
/// `nros::main!` emits `::<crate>::register()` for every backend the bridge
/// spans and `::nros_rmw_cyclonedds::register::<M>()` for each non-flat egress
/// type, so those crates have to be in the ENTRY's dependency list — a feature
/// on the board crate compiles and links them but does not put their names in
/// scope. The two hand-written bridge entries listed them by hand; this is the
/// same list, derived.
///
/// **Backends.** The set is the image's own RMW plus every `[[domain]].rmw` —
/// the same derivation `facade::image_backends` and the MACRO's `bridge_rmws`
/// both make. The crate name is then read out of the BOARD crate's own
/// `rmw-<x> = ["dep:<crate>"]` feature rather than from a table here. That is
/// deliberate: the macro carries its own three-arm `rmw_crate_ident`, and a
/// fourth copy would be the extra spelling that drifts. The board's answers
/// agree with the macro's today (`zenoh` → `nros_rmw_zenoh`, `cyclonedds` →
/// `nros_rmw_cyclonedds_sys`, `xrce` → `nros_rmw_xrce_cffi`), and if one ever
/// stops agreeing the build fails with the missing name rather than silently
/// linking the wrong backend.
///
/// **Message crates.** `nros sync` writes `<bringup>/nros-bridge.toml` with a
/// `[[register_type]] rust_path = "std_msgs::msg::Header"` per non-flat egress
/// type; the crate is the first segment, and it lives in `generated/`.
fn bridge_entry_deps(
    bringup_dir: &std::path::Path,
    entry_dir: &std::path::Path,
    nros_root: &std::path::Path,
    candidates: &[&str],
) -> Result<Vec<String>> {
    let Ok(text) = std::fs::read_to_string(bringup_dir.join("system.toml")) else {
        return Ok(Vec::new());
    };
    let Ok(doc) = text.parse::<toml::Value>() else {
        return Ok(Vec::new());
    };
    // Only a bringup that declares a bridge needs any of this.
    if doc
        .get("bridge")
        .and_then(|b| b.as_array())
        .is_none_or(|a| a.is_empty())
    {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();

    // ---- backends, via the board crate's own feature table -----------------
    let board_dir = crate::builder::entry::macro_board_crate(candidates)
        .map(|k| nros_root.join("packages/boards").join(k));
    let board_manifest = board_dir
        .as_ref()
        .and_then(|d| std::fs::read_to_string(d.join("Cargo.toml")).ok())
        .and_then(|t| t.parse::<toml::Value>().ok());

    let mut rmws: Vec<String> = doc
        .get("domain")
        .and_then(|d| d.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|d| d.get("rmw").and_then(|r| r.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    rmws.sort();
    rmws.dedup();

    for rmw in &rmws {
        let feature = format!("rmw-{rmw}");
        let Some(krate) = board_manifest
            .as_ref()
            .and_then(|m| m.get("features")?.get(&feature)?.as_array().cloned())
            .and_then(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .find_map(|v| v.strip_prefix("dep:").map(str::to_string))
            })
        else {
            // The board does not carry this backend. Not an error here: a board
            // that cannot host a domain is the SYSTEM's problem and is reported
            // where the system resolves, with the board named.
            continue;
        };
        // `packages/rmw/<family>/<crate>` — located rather than assumed, since
        // the family directory is not the crate name (`nros-rmw-xrce-cffi`
        // lives under `xrce/`).
        let Some(path) = ["zenoh", "cyclonedds", "xrce", "uorb", "dds"]
            .iter()
            .map(|fam| nros_root.join("packages/rmw").join(fam).join(&krate))
            .find(|p| p.join("Cargo.toml").is_file())
        else {
            continue;
        };
        let rel = crate::builder::paths::relative_or_err(entry_dir, &path)
            .map_err(|e| eyre::eyre!("{e}"))?;
        out.push(format!("{krate} = {{ path = \"{rel}\" }}"));
    }

    // ---- message crates named by the generated bridge config ---------------
    if let Ok(cfg) = std::fs::read_to_string(bringup_dir.join("nros-bridge.toml"))
        && let Ok(cfg) = cfg.parse::<toml::Value>()
    {
        let rows = cfg
            .get("register_type")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();

        // A cyclonedds `register_type` makes the macro emit
        // `::nros_rmw_cyclonedds::register::<M>()` — the WRAPPER crate, not the
        // `-sys` one the board feature pulls. `-sys` depends on it, so it is
        // already linked; what is missing is only the NAME in the entry's
        // scope. Keyed on the row's own `rmw` field, which is what the macro
        // filters on, rather than on "cyclonedds is among the domains": a
        // bridge can span cyclonedds and still register no non-flat type, and
        // then the macro emits no such call.
        if rows
            .iter()
            .any(|t| t.get("rmw").and_then(|r| r.as_str()) == Some("cyclonedds"))
            && let Some(path) = ["cyclonedds"]
                .iter()
                .map(|fam| {
                    nros_root
                        .join("packages/rmw")
                        .join(fam)
                        .join("nros-rmw-cyclonedds")
                })
                .find(|p| p.join("Cargo.toml").is_file())
        {
            let rel = crate::builder::paths::relative_or_err(entry_dir, &path)
                .map_err(|e| eyre::eyre!("{e}"))?;
            out.push(format!("nros-rmw-cyclonedds = {{ path = \"{rel}\" }}"));
        }

        let mut msg_crates: Vec<String> = rows
            .iter()
            .filter_map(|t| t.get("rust_path")?.as_str())
            .filter_map(|p| p.split("::").next().map(str::to_string))
            .collect();
        msg_crates.sort();
        msg_crates.dedup();
        for m in msg_crates {
            let path = bringup_dir
                .parent()
                .and_then(|p| p.parent())
                .map(|ws| ws.join("generated").join(&m));
            let Some(path) = path.filter(|p| p.join("Cargo.toml").is_file()) else {
                continue;
            };
            let rel = crate::builder::paths::relative_or_err(entry_dir, &path)
                .map_err(|e| eyre::eyre!("{e}"))?;
            out.push(format!(
                "{m} = {{ path = \"{rel}\", default-features = false }}"
            ));
        }
    }

    Ok(out)
}

/// The deployment of a RUST entry package: its own `system.toml`, else the
/// bringup image that claims it (`leaf_system::for_entry`, phase-445 W5).
/// `None` for a package that is not a Rust entry, or one no image claims.
///
/// This is where the Rust half of "which board is this entry for" is answered
/// since RFC-0098 D5 retired `[package.metadata.nros.entry] deploy` — the same
/// reader `nros::main!` and `nros sync` use, so the three cannot disagree.
fn rust_entry_system(
    dir: &std::path::Path,
    bringup_dirs: &[PathBuf],
) -> Option<nros_orchestration_ir::leaf_system::LeafSystem> {
    use nros_orchestration_ir::leaf_system;
    let pkg = crate::cmd::ws::entry_package_name_of(&dir.join("Cargo.toml"))?;
    if let Ok(Some(own)) = leaf_system::read(dir) {
        return Some(own);
    }
    bringup_dirs
        .iter()
        .find_map(|b| leaf_system::for_entry(dir, &pkg, b).ok().flatten())
}

/// Deploy tokens a package's entry declaration names, if it is an entry.
///
/// Two spellings, because the two languages declare it in different files:
/// Rust in the `system.toml` image that claims the entry ([`rust_entry_system`]),
/// C/C++ in the `nano_ros_add_executable(… DEPLOY <token>…)` call. Both are read;
/// a package that is not an entry yields an empty list.
fn entry_deploy_tokens(dir: &std::path::Path, bringup_dirs: &[PathBuf]) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(board) = rust_entry_system(dir, bringup_dirs).and_then(|l| l.board) {
        out.push(board);
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
    bringup_dirs: &[PathBuf],
) -> std::collections::BTreeSet<PathBuf> {
    let mut out = std::collections::BTreeSet::new();
    for pkg in &found.packages {
        let tokens = entry_deploy_tokens(&pkg.dir, bringup_dirs);
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

/// Stage 3.5 for ONE image — RFC-0094 D1, phase-439 W2.
///
/// Reads the declarations, runs [`EntityInventory::derive`] once through
/// [`crate::resolve::write`], and returns the directory the artifacts landed in
/// so stage 4 can hand it to a configure. `None` means the phase wrote nothing
/// AT ALL, which happens only when the filesystem refused — a declaration this
/// phase cannot READ still produces a `resolved.toml` saying so.
///
/// # Why this never bails
///
/// A `nros build` that worked yesterday must not stop working because a new
/// phase could not answer for an image. Every failure mode here is a WARNING
/// plus a refusal recorded in the artifact; the downstream lanes then behave
/// exactly as they did before this phase existed, because the CMake projection
/// — the only thing a build READS — is written only for a derived answer.
///
/// # What it does NOT read
///
/// `${CMAKE_BINARY_DIR}/nros-metadata.json`, which today's
/// `nros ws entity-inventory` call takes as its component set. That file is
/// written DURING a configure by `nano_ros_node_register()`, so reading it here
/// would recreate the producer-after-reader lag this phase exists to remove.
/// The declaration this phase reads instead is the contract sidecar beside the
/// launch file (`<bringup>/launch/<stem>.contract.yaml`), folded into the
/// resolved SystemModel — which is where phase-412 already put the statement of
/// what an image creates when it retired the `ENTITIES` argument.
fn resolve_image(
    root: &std::path::Path,
    bringup: &str,
    bringup_dir: &std::path::Path,
    image_id: &str,
    image: &crate::orchestration::image::ImageBlock,
    platform: &str,
    board: &str,
) -> Option<(std::path::PathBuf, crate::resolve::Resolved)> {
    use crate::{
        entity_inventory::EntityInventory,
        orchestration::model_location,
        resolve::{ImageIdent, write as write_resolved},
    };

    let ident = ImageIdent {
        bringup: bringup.to_string(),
        entry: image_id.to_string(),
        board: board.to_string(),
        platform: platform.to_string(),
        rmw: image.rmw.clone().unwrap_or_default(),
    };
    let dir = root.join("build").join(ident.dir_name());

    // (launch, args) → the resolved model, exactly as `generate_entry` reaches
    // it. `ensure_model` resolves from `system.toml` + the launch file when no
    // build has produced one, so this needs no build system to have run.
    let args_vec: Vec<(String, String)> = image
        .args
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let inventory: Result<EntityInventory, String> = (|| {
        let model_rel =
            model_location::launch_to_model_rel(bringup_dir, image.launch.as_deref(), &args_vec)
                .map_err(|e| format!("cannot resolve the launch for this image: {e}"))?;
        let (model_path, _inputs) = model_location::ensure_model(bringup_dir, &model_rel)
            .map_err(|e| format!("cannot resolve the SystemModel: {e}"))?;
        let raw = std::fs::read_to_string(&model_path)
            .map_err(|e| format!("reading {}: {e}", model_path.display()))?;
        let model: ros_launch_manifest_model::SystemModel = serde_yaml_ng::from_str(&raw)
            .map_err(|e| format!("parsing {}: {e}", model_path.display()))?;
        // `None` is "no wiring described", which is a DECLARATION GAP and not
        // an error: 5 of the tree's 114 resolvable models describe wiring, and
        // they are exactly the 5 with a contract sidecar (issue 0973).
        //
        // phase-446 W4 -- the contract's `params:` ride along, exactly as the
        // configure-time producer attaches them, so the two fragments agree
        // byte for byte (issue 1228).
        EntityInventory::from_model(model_path.display().to_string(), &model)
            .map(|mut inv| {
                inv.set_param_declarations(crate::entity_inventory::ParamDeclarations::from_model(
                    &model,
                ));
                inv
            })
            .ok_or_else(|| {
                format!(
                    "the launch tree resolved, and it describes no wiring. Nothing here can \
                 derive a count. State what each node creates in the contract sidecar \
                 beside the launch file ({}/launch/<stem>.contract.yaml); until then this \
                 image keeps its configured pool knobs.",
                    bringup_dir.display()
                )
            })
    })();

    let written = match inventory {
        Ok(inv) => write_resolved(&dir, ident, &inv),
        Err(reason) => {
            let r = crate::resolve::Resolved::unresolvable(
                ident,
                bringup_dir.join("launch").display().to_string(),
                reason,
            );
            // Same two writes as the derived path, minus the projection — kept
            // inline rather than routed through `write` because there is no
            // inventory to render one from.
            match std::fs::create_dir_all(&dir)
                .and_then(|()| {
                    let stale = dir.join(crate::resolve::RESOLVED_CMAKE_NAME);
                    if stale.exists() {
                        std::fs::remove_file(&stale)?;
                    }
                    Ok(())
                })
                .and_then(|()| {
                    let p = dir.join(crate::resolve::RESOLVED_TOML_NAME);
                    crate::atomic_file::atomic_write(&p, &r.to_toml())
                        .map_err(|e| std::io::Error::other(e.to_string()))
                        .map(|()| p)
                }) {
                Ok(toml_path) => Ok(crate::resolve::Written {
                    resolved: r,
                    toml_path,
                    cmake_path: None,
                }),
                Err(e) => Err(e),
            }
        }
    };

    match written {
        Err(e) => {
            eprintln!("nros build: warning: stage 3.5 could not write `{image_id}`'s resolve: {e}");
            None
        }
        Ok(w) => {
            match w.resolved.knobs() {
                Some(k) => eprintln!(
                    "nros build:   resolved → {} (max_cbs {}, nodes {}, digest {})",
                    w.toml_path.display(),
                    k.max_cbs,
                    k.max_nodes,
                    w.resolved.digest()
                ),
                // Says WHY, once, at the same volume as the success line. A
                // refusal that only lives in a file nobody opens is the silent
                // shape RFC-0094 exists to remove.
                None => eprintln!(
                    "nros build:   resolved → {} (no count derived; see [provenance].refused)",
                    w.toml_path.display()
                ),
            }
            Some((dir, w.resolved))
        }
    }
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

/// The coordinate for a CMAKE root, which must also separate BOARDS.
///
/// A CMake workspace is one board per configure — CMake pins the compiler at
/// the first configure and will not swap it on reconfigure, which is issue
/// 0391's whole subject. `examples/workspaces/c` declares `freertos`
/// (mps2-an385-freertos, cross arm-none-eabi) and `freertos_posix`
/// (freertos-posix, host cc) on the SAME platform token, so a platform-only
/// coordinate put two toolchains in one `build/freertos-zenoh/` and whichever
/// configured first would poison the cache for the other.
///
/// Cargo needs no such split: it separates by `--target` inside one dir, and
/// widening its coordinate would rename every generated entry directory for no
/// gain. So this is the cmake driver's own rule, not a change to
/// [`coordinate`].
fn cmake_coordinate(platform: &str, image: &crate::orchestration::image::ImageBlock) -> String {
    let base = coordinate(platform, image);
    match image.board.as_deref() {
        Some(b) if b != platform => format!("{base}-{}", b.replace(['/', '.'], "-")),
        _ => base,
    }
}

/// The west APPLICATION for an image — the entry package, not the bringup.
///
/// issue 0892. The bringup is `launch/ package.xml system.toml`; it has no
/// `CMakeLists.txt`, so `west build <bringup>` cannot work. The application is
/// the framework entry package for this image's board — the one thing in the
/// workspace that IS a Zephyr app, with `find_package(Zephyr)`, `prj.conf` and
/// its RMW overlays.
///
/// Matched by BOARD, through the same `[package.metadata.nros.entry] deploy`
/// resolution `framework_entry_dirs` uses, so a workspace with several zephyr
/// entries (`zephyr_entry`, `zephyr_entry_robot1`) picks the one whose deploy
/// resolves to the board the image asked for.
///
/// `None` when no entry claims the board: the caller then keeps its previous
/// target, and the failure is west's own "not an application", which names the
/// directory. Inventing an application here would be worse — RFC-0065 D5 is
/// explicit that a framework entry's authored Kconfig is not derivable.
/// The workspace package that IS the framework application for `image_id`.
///
/// Two ways a package says it serves a deploy target, and BOTH are load-bearing
/// because the two languages declare it in different files:
///
/// * Rust — the bringup image that claims the entry (`entry = "<pkg>"`, or the
///   `<id>_entry` name) — `leaf_system::for_entry`, phase-445 W5; before that,
///   `[package.metadata.nros.entry] deploy = "zephyr"` in `Cargo.toml`
/// * C/C++ — `nano_ros_add_executable(... DEPLOY zephyr)` in `CMakeLists.txt`
///
/// Reading only the Rust one was a silent Rust-only restriction: the C, C++,
/// mixed, realtime-c and realtime-cpp workspaces all have a `zephyr_entry`
/// declaring `DEPLOY zephyr`, none of them has a `Cargo.toml` for it, so all
/// five fell through to the bringup directory. That fallback is a real
/// directory, so nothing errored — west was simply pointed at the wrong place,
/// and the first thing to notice was a conf fragment "not found" in two paths
/// that were the same path twice.
///
/// Ambiguity is an ERROR, not a first match. `realtime-cpp` has `zephyr_entry`
/// and `fvp_entry`, both `DEPLOY zephyr`, both on `native_sim/native/64`, for
/// two images that differ in payload rather than board. Whichever one a scan
/// returns first is right half the time and silently wrong the other half, so
/// the image names it (`entry = "fvp_entry"`) and this refuses until it does.
fn west_application_dir(
    image_id: &str,
    image: &crate::orchestration::image::ImageBlock,
    descriptor: &crate::orchestration::board_descriptor::BoardDescriptor,
    found: &crate::builder::discover::Discovered,
    catalog: &crate::orchestration::board_descriptor::BoardCatalog,
    bringup_dirs: &[PathBuf],
) -> eyre::Result<Option<PathBuf>> {
    use crate::orchestration::board_descriptor::DeployResolution;

    // An explicit `entry` wins outright — it is the answer to the ambiguity
    // below, so it must not be re-derived or second-guessed.
    if let Some(name) = &image.entry {
        let hit = found.packages.iter().find(|p| {
            p.name.as_str() == name.as_str()
                || p.dir.file_name().and_then(|n| n.to_str()) == Some(name.as_str())
        });
        return match hit {
            Some(p) => Ok(Some(p.dir.clone())),
            None => Err(eyre::eyre!(
                "[image.{image_id}] names entry `{name}`, which is not a package in this \
                 workspace.\n  Known packages: {}",
                found
                    .packages
                    .iter()
                    .map(|p| p.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        };
    }

    let mut hits: Vec<PathBuf> = Vec::new();
    for pkg in &found.packages {
        // A Rust entry claimed by an image says WHICH image, not just which
        // board — so it is this image's application exactly when it is this
        // image's entry, and never a candidate for a sibling image on the same
        // board (the ambiguity below is a C/C++ question).
        if let Some(l) = rust_entry_system(&pkg.dir, bringup_dirs) {
            if l.image.as_deref() == Some(image_id) && pkg.dir.join("CMakeLists.txt").is_file() {
                hits.push(pkg.dir.clone());
            }
            continue;
        }
        let Some(deploy) = package_deploy_token(&pkg.dir) else {
            continue;
        };
        if let DeployResolution::Board(d) = catalog.resolve_deploy(&deploy)
            // Identity is the NAME SET: a descriptor has several spellings
            // (`native_sim/native/64`, `zephyr`, …) and two descriptors are the
            // same board when their name lists are.
            && d.names == descriptor.names
            && pkg.dir.join("CMakeLists.txt").is_file()
        {
            hits.push(pkg.dir.clone());
        }
    }

    match hits.len() {
        0 => Ok(None),
        1 => Ok(Some(hits.remove(0))),
        _ => Err(eyre::eyre!(
            "[image.{image_id}] matches {} entry packages, so the application cannot be \
             derived:\n{}\n  Name the one this image builds:\n\n    [image.{image_id}]\n    \
             entry = \"{}\"",
            hits.len(),
            hits.iter()
                .map(|h| format!("  {}", h.display()))
                .collect::<Vec<_>>()
                .join("\n"),
            hits[0]
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("<pkg>")
        )),
    }
}

/// The deploy token a package declares, from whichever file its language uses.
///
/// The CMake side is matched textually rather than by evaluating cmake: the
/// call is authored by hand in a fixed shape (`nano_ros_add_executable(` …
/// `DEPLOY <token>`), and the alternative — configuring the project to ask it —
/// costs a cmake run per candidate package during a plan that is supposed to be
/// cheap enough for `--dry-run`.
///
/// C/C++ only since phase-445 W5: a Rust entry states no token of its own, and
/// its board is read through [`rust_entry_system`] by the callers.
fn package_deploy_token(dir: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("CMakeLists.txt")).ok()?;
    cmake_deploy_token(&text)
}

/// `DEPLOY <token>` out of a `nano_ros_add_executable`/`nano_ros_entry` call.
///
/// Scoped to the call, not to the file: `DEPLOY` also appears in comments
/// ("`nano_ros_node_register` has no DEPLOY → component-only") and a
/// file-wide regex would read those as declarations.
fn cmake_deploy_token(text: &str) -> Option<String> {
    let mut rest = text;
    while let Some(i) = rest
        .find("nano_ros_add_executable(")
        .or_else(|| rest.find("nano_ros_entry("))
    {
        let open = rest[i..].find('(')? + i;
        let close = rest[open..].find(')')? + open;
        let call = &rest[open + 1..close];
        let mut it = call.split_whitespace();
        while let Some(tok) = it.next() {
            if tok == "DEPLOY" {
                if let Some(v) = it.next() {
                    return Some(v.trim_matches('"').to_string());
                }
            }
        }
        rest = &rest[close + 1..];
    }
    None
}

/// `ZEPHYR_BASE` for a west build — the thing that actually makes `west build`
/// runnable.
///
/// **Not a `.west/` search.** Measured: with `ZEPHYR_BASE` exported, `west
/// build --help` works from any directory; without it, west says
/// `unknown command "build"; do you need to run this inside a workspace?` even
/// standing in the repo. So the requirement is a Zephyr, not a workspace —
/// which is also why Zephyr's FREESTANDING application works, and why every
/// zephyr fixture in this tree builds from the repo root rather than from
/// `zephyr-workspace/`.
///
/// The ladder is `scripts/build/west-fixtures.sh`'s, verbatim, and
/// `NROS_ZEPHYR_WORKSPACE` is the established spelling — 60 references across
/// the tree. An earlier version of this invented `NROS_WEST_WORKSPACE`, which
/// would have been a 61st name for one thing.
fn zephyr_base(root: &std::path::Path, flag: Option<&std::path::Path>) -> Option<PathBuf> {
    zephyr_base_with(
        flag,
        std::env::var_os("ZEPHYR_BASE"),
        std::env::var_os("NROS_ZEPHYR_WORKSPACE"),
        root,
    )
}

/// Is this directory a Zephyr tree rather than the workspace above one?
///
/// `Kconfig.zephyr` is the file Zephyr's own `find_package` machinery keys on,
/// so it is the marker rather than a name match — a workspace is free to check
/// Zephyr out under any directory name.
fn is_zephyr_tree(p: &std::path::Path) -> bool {
    p.join("Kconfig.zephyr").is_file()
}

/// [`zephyr_base`] with the flag and the two env reads passed IN, so the resolution is
/// testable without touching process-global state.
fn zephyr_base_with(
    flag: Option<&std::path::Path>,
    base: Option<std::ffi::OsString>,
    workspace: Option<std::ffi::OsString>,
    root: &std::path::Path,
) -> Option<PathBuf> {
    // `--zephyr-workspace` first: it is the only rung the user states in the
    // command itself, so an env left over from another project must not win
    // over what this invocation says.
    //
    // It names a WORKSPACE, like `$NROS_ZEPHYR_WORKSPACE` — but a user who
    // passes the `zephyr/` directory has named the same place by its other
    // name, and refusing that would be pedantry over a distinction only this
    // code cares about. So both resolve.
    if let Some(f) = flag {
        let nested = f.join("zephyr");
        if nested.is_dir() {
            return Some(nested);
        }
        if is_zephyr_tree(f) {
            return Some(f.to_path_buf());
        }
    }
    if let Some(b) = base {
        let p = PathBuf::from(b);
        if p.is_dir() {
            return Some(p);
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(w) = workspace {
        candidates.push(PathBuf::from(w));
    }
    candidates.push(root.join("zephyr-workspace"));
    if let Some(parent) = root.parent() {
        candidates.push(parent.join("nano-ros-workspace"));
        candidates.push(parent.join("nano-ros-workspace-4.4"));
    }
    candidates
        .into_iter()
        .map(|ws| ws.join("zephyr"))
        .find(|z| z.is_dir())
}

/// Package directories a cargo root must NOT list as members.
///
/// A west or ESP-IDF entry is built by its own framework; listing it makes a
/// host `cargo build` try to compile a Zephyr staticlib.
/// `examples/workspaces/rust` excludes exactly these by hand today.
fn framework_entry_dirs(
    found: &crate::builder::discover::Discovered,
    catalog: &crate::orchestration::board_descriptor::BoardCatalog,
    bringup_dirs: &[PathBuf],
) -> std::collections::BTreeSet<PathBuf> {
    entry_dirs_where(found, catalog, bringup_dirs, |d| !d.needs_generated_root())
}

/// Rust entry package directories whose resolved driver satisfies `want`.
fn entry_dirs_where(
    found: &crate::builder::discover::Discovered,
    catalog: &crate::orchestration::board_descriptor::BoardCatalog,
    bringup_dirs: &[PathBuf],
    want: impl Fn(Driver) -> bool,
) -> std::collections::BTreeSet<PathBuf> {
    use crate::orchestration::board_descriptor::DeployResolution;
    let mut out = std::collections::BTreeSet::new();
    for pkg in &found.packages {
        let Some(deploy) = rust_entry_system(&pkg.dir, bringup_dirs).and_then(|l| l.board) else {
            continue;
        };
        if let DeployResolution::Board(d) = catalog.resolve_deploy(&deploy)
            && want(plan::driver_for_board(
                d.platform.kebab(),
                d.entry_kind,
                false,
            ))
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

/// `(bringup name, bringup dir, its images)` per bringup — the shape every
/// stage after DISCOVER passes around.
type Bringups = Vec<(String, PathBuf, plan::ImageSet)>;

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
) -> Result<Bringups> {
    let (out, warnings) = collect_images_with_warnings(
        packages,
        crate::orchestration::image::deprecation_suppressed(),
    )?;
    for w in warnings {
        eprintln!("nros build: {w}");
    }
    Ok(out)
}

/// phase-398 W3 — resolve every `<depend>` a workspace declares, or fail.
///
/// The ladder is RFC-0062's, amended 2026-08-29: workspace package → generated
/// message → `[prereq.*]` key → ROS package (ament index) → error.
///
/// Fails by default. The alternative is what shipped for years: a name matching
/// nothing was dropped in silence, and the first run of this check over the
/// tree found three `<exec_depend>` entries naming packages that do not exist,
/// stale since a rename, in workspaces that build green.
///
/// `NROS_ALLOW_UNRESOLVED_DEPS=1` downgrades it to a warning. That is an escape
/// hatch for a tree mid-migration, not a mode — it names itself in the output
/// so a build that used it cannot be mistaken for one that passed.
fn check_declared_depends(
    root: &std::path::Path,
    packages: &[cargo_nano_ros::provider_scan::WorkspacePackage],
    nano_ros_root: Option<&std::path::Path>,
) -> Result<()> {
    use crate::orchestration::prereq_resolve as pr;

    let declared = pr::declared_depends(root);
    if declared.is_empty() {
        return Ok(());
    }

    let ws: std::collections::BTreeSet<String> = packages.iter().map(|p| p.name.clone()).collect();

    // Generated message crates: whatever `nros sync` has already written, plus
    // the core pre-generated set. A message package that has not been generated
    // YET must not read as unresolved — that is a `nros sync` away, not a
    // missing declaration.
    let mut generated: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for dir in [root.join("generated"), root.join("build/nros/generated")] {
        if let Ok(rd) = std::fs::read_dir(dir) {
            generated.extend(
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned()),
            );
        }
    }
    if let Some(nr) = nano_ros_root
        && let Ok(rd) = std::fs::read_dir(nr.join("packages/interfaces"))
    {
        // The committed `nros-`prefixed msg crates are reached by their ROS
        // name in a `package.xml`, so strip the prefix the crate carries.
        generated.extend(rd.flatten().map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.strip_prefix("nros-").unwrap_or(&n).to_string()
        }));
    }

    let prereq_map: std::collections::BTreeMap<String, crate::orchestration::sdk_index::PrereqDep> =
        nano_ros_root
            .map(|nr| nr.join("nros-sdk-index.toml"))
            .filter(|p| p.is_file())
            .and_then(|p| crate::orchestration::sdk_index::SdkIndex::load(&p).ok())
            .map(|i| i.prereqs())
            .unwrap_or_default();
    let prereq_keys: std::collections::BTreeSet<String> = prereq_map.keys().cloned().collect();

    let ros = pr::ros_packages();
    // Off-ROS safety: a package's own buildtool is satisfied by the builder that
    // is building it. Without this, adding the `<buildtool_depend>` that rosdep
    // expects would hard-fail every host with no `AMENT_PREFIX_PATH`.
    let self_buildtools = pr::self_satisfied_buildtools(root);

    let mut unresolved: Vec<pr::Unresolved> = Vec::new();
    // phase-422 W8 — a dep that RESOLVES but names infrastructure. `role` says
    // who may name a key; `infra` (emulators, cross toolchains, debug probes)
    // comes from WHERE you deploy, not from what your code needs, so
    // `<depend>qemu-system-arm</depend>` is a category error rather than a
    // missing package. Refused separately because the remedy is different:
    // nothing to install, the declaration itself is wrong.
    //
    // Scoped to `infra` DELIBERATELY. `workspace` and `vendor` are not
    // obviously category errors from a user's side — a package that builds
    // against a vendored source tree naming it is arguable — so refusing them
    // would risk more than it buys. Measured before landing: ZERO packages in
    // this tree name a non-`package` key, so this breaks nothing here and the
    // blast radius is entirely out-of-tree.
    let mut wrong_role: Vec<(pr::Unresolved, &'static str)> = Vec::new();

    // phase-447 D3 — the pinned rosdep snapshot, consulted ONLY for names the
    // ladder above left unresolved, and only if there are any. RFC-0099 D8.
    let snapshot = pr::rosdep_fallback(
        &declared,
        &ws,
        &generated,
        &prereq_keys,
        &ros,
        &self_buildtools,
        nano_ros_root,
    )?;
    let rosdep_keys = snapshot
        .as_ref()
        .map(crate::orchestration::rosdep_snapshot::RosdepSnapshot::keys)
        .unwrap_or_default();
    let mut from_snapshot: Vec<String> = Vec::new();

    for (name, files) in &declared {
        let res = pr::classify(
            name,
            &ws,
            &generated,
            &prereq_keys,
            &ros,
            &self_buildtools,
            &rosdep_keys,
        );
        if res == pr::Resolution::RosdepSnapshot {
            from_snapshot.push(name.clone());
        }
        if res == pr::Resolution::Unknown {
            unresolved.push(pr::Unresolved {
                name: name.clone(),
                declared_by: files.clone(),
            });
        } else if res == pr::Resolution::Prereq
            && let Some(dep) = prereq_map.get(name)
            && dep.role == crate::orchestration::sdk_index::PrereqRole::Infra
        {
            wrong_role.push((
                pr::Unresolved {
                    name: name.clone(),
                    declared_by: files.clone(),
                },
                "infra",
            ));
        }
    }

    if !wrong_role.is_empty() {
        let mut m = format!(
            "{} <depend> name(s) declare INFRASTRUCTURE, not a dependency:\n",
            wrong_role.len()
        );
        for (u, role) in &wrong_role {
            m.push_str(&format!(
                "  {} (role = {role}) — declared by {}\n",
                u.name,
                u.declared_by.join(", ")
            ));
        }
        m.push_str(
            "\nA package.xml declares what the package's CONTENT needs. An emulator, \
             cross toolchain or debug probe comes from WHERE the package deploys — \
             declare that instead, in the `system.toml` that names the image \
             (RFC-0098 D3):\n\
             \n  [image.<id>]\n  board = \"<board>\"\n\
             \nand provision it with `nros setup <board>` (see `nros setup --workspace`, \
             which reports what a workspace needs).\n\
             \n  NROS_ALLOW_INFRA_DEPS=1  to continue with a warning.",
        );
        if std::env::var_os("NROS_ALLOW_INFRA_DEPS").is_some() {
            eprintln!("nros build: WARNING (NROS_ALLOW_INFRA_DEPS=1): {m}");
        } else {
            eyre::bail!("{m}")
        }
    }

    // phase-447 D3 — a snapshot resolution is REPORTED, never silent.
    //
    // These names resolved, so the build proceeds; but the snapshot carries no
    // `check`, so nothing here has asked whether the package is installed. That
    // is the one RFC-0062 objection a vendored snapshot does not answer, and an
    // unanswered objection that leaves no trace in the output is
    // indistinguishable from one nobody had. Naming the pin makes the answer
    // reproducible: two hosts reading the same ref got the same list.
    if let (false, Some(snap)) = (from_snapshot.is_empty(), snapshot.as_ref()) {
        eprintln!(
            "nros build: {} <depend> name(s) resolved from the pinned rosdep \
             snapshot (rosdistro {}), {}:\n  {}\n  \
             Declare a `[prereq.*]` key with a `check` probe to make presence \
             diagnosable instead.",
            from_snapshot.len(),
            snap.short_ref(),
            crate::orchestration::rosdep_snapshot::RosdepSnapshot::PROVENANCE_NOTE,
            from_snapshot.join("\n  "),
        );
    }

    if unresolved.is_empty() {
        return Ok(());
    }

    let mut msg = format!(
        "{} <depend> name(s) resolve to nothing:\n",
        unresolved.len()
    );
    for u in &unresolved {
        msg.push_str(&format!(
            "  {} — declared by {}\n",
            u.name,
            u.declared_by.join(", ")
        ));
    }
    msg.push_str(
        "\nEach must be one of: a package in this workspace, a message package \
         `nros sync` generates, a `[prereq.*]` key in nros-sdk-index.toml whose \
         `role` is `package`, a package the ambient ROS install provides \
         (source its setup.bash so AMENT_PREFIX_PATH is set), or a key in the \
         pinned rosdep snapshot `nros-rosdep-snapshot.toml`.\n\
         \nNOTE the role: a key for an emulator, cross toolchain or vendored \
         source tree is NOT declarable here — that comes from the board the \
         image names in `system.toml` (`[image.<id>] board`). Adding a \
         `[prereq.*]` entry to make this resolve is the wrong fix if the thing \
         is infrastructure.\n\
         \n  NROS_ALLOW_UNRESOLVED_DEPS=1  to continue with a warning.",
    );

    if std::env::var_os("NROS_ALLOW_UNRESOLVED_DEPS").is_some() {
        eprintln!("nros build: WARNING (NROS_ALLOW_UNRESOLVED_DEPS=1): {msg}");
        return Ok(());
    }
    eyre::bail!("{msg}")
}

/// [`collect_images`], with the deprecation warnings RETURNED rather than
/// printed and the suppression flag passed in.
///
/// Split for the reason the lint's own doc comment gives for taking
/// `suppressed` as a parameter: a warning that can only be observed on stderr,
/// under an ambient env var, cannot be tested deterministically — and the
/// previous shape is exactly how W1.f shipped a correct, well-tested lint that
/// no production path called. The test below asserts the WIRING, not the lint.
fn collect_images_with_warnings(
    packages: &[cargo_nano_ros::provider_scan::WorkspacePackage],
    suppressed: bool,
) -> Result<(Bringups, Vec<String>)> {
    let mut warnings: Vec<String> = Vec::new();
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

        // W1.f's deprecation lint, ACTUALLY REACHED.
        //
        // It shipped with four passing tests and no production caller, so no
        // user has ever seen the warning it promised ("warn on every invocation
        // while still working"). That is the shape `check-no-vacuous-tests`
        // exists for, one level up: the function is correct and its tests are
        // honest, and the feature is still absent because nothing calls it.
        //
        // Here rather than in `nros doctor` alone, because this is where a
        // build reads the declaration it is warning about — the user is looking
        // at the output already, and it costs one pass over a table that is
        // typically empty.
        //
        // Field PRESENCE comes from the raw document, not from `sys.deploy`:
        // `DeployTarget` is upstream's typed struct, so an absent key and a key
        // set to its default are the same value once parsed, and a lint about
        // "you wrote this key" must see what was written.
        if let Ok(raw) = text.parse::<toml::Value>()
            && let Some(deploys) = raw.get("deploy").and_then(|d| d.as_table())
        {
            let present: std::collections::BTreeMap<String, Vec<String>> = deploys
                .iter()
                .filter_map(|(id, block)| {
                    let t = block.as_table()?;
                    Some((id.clone(), t.keys().cloned().collect()))
                })
                .collect();
            for w in crate::orchestration::image::deprecated_deploy_build_field_warnings(
                &present, &sys.image, suppressed,
            ) {
                warnings.push(format!("{}: {w}", system_toml.display()));
            }
        }

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
    Ok((out, warnings))
}

#[cfg(test)]
mod facade_absence_tests {
    use super::*;

    fn bringup_with(system_toml: &str) -> tempfile::TempDir {
        let d = tempfile::tempdir().expect("tempdir");
        std::fs::write(d.path().join("system.toml"), system_toml).expect("write system.toml");
        d
    }

    /// Every REQUIRED field of `[system]`. `domain_id` is one of them, and the
    /// first draft of these tests omitted it: `SystemToml` then failed to parse,
    /// `declared_capabilities` returned empty for both spellings, and the tests
    /// looked like they had caught a production bug. They had caught a bad
    /// fixture. Keep this complete.
    const BASE: &str = r#"
[system]
name = "t"
rmw = "zenoh"
domain_id = 0
"#;

    /// The form that produced the `host-tests` red: capabilities declared with
    /// the phase-261 `[system].features` list and no typed block in sight.
    /// Reading only the typed blocks would return empty here and re-open the bug.
    #[test]
    fn phase_261_features_list_is_seen() {
        let d = bringup_with(&format!(
            "{BASE}features = [\"param_services\", \"lifecycle\"]\n"
        ));
        let mut got = declared_capabilities(d.path());
        got.sort_unstable();
        assert_eq!(got, vec!["lifecycle", "param_services"]);
    }

    /// The deprecated typed block still counts — both spellings flip the axis.
    #[test]
    fn typed_block_is_seen() {
        let d = bringup_with(&format!("{BASE}\n[param_services]\nenabled = true\n"));
        assert_eq!(declared_capabilities(d.path()), vec!["param_services"]);
    }

    /// A system that declares nothing keeps the WARNING path: a missing facade
    /// there costs the RMW and the ROS edition, both of which have defaults, and
    /// escalating it would fail builds that are documented to tolerate it.
    #[test]
    fn no_capabilities_is_not_fatal() {
        let d = bringup_with(BASE);
        assert!(declared_capabilities(d.path()).is_empty());
    }

    /// Absent or unparseable input must NOT read as "capabilities declared".
    /// This predicate only escalates an existing warning, so silence is the
    /// safe answer; the malformed file is someone else's error to report.
    #[test]
    fn unreadable_system_is_empty_not_fatal() {
        let empty = tempfile::tempdir().expect("tempdir");
        assert!(declared_capabilities(empty.path()).is_empty());

        let junk = bringup_with("this is not toml {{{");
        assert!(declared_capabilities(junk.path()).is_empty());
    }
}

#[cfg(test)]
mod deprecation_wiring_tests {
    use super::*;

    /// The lint W1.f promised must be REACHED by the build path.
    ///
    /// It shipped with four passing unit tests and no production caller, so the
    /// warning it specified ("warn on every invocation while still working")
    /// reached nobody: `nros build` was silent and `nros doctor` never grew the
    /// check either. The unit tests could not catch that — they call the lint
    /// directly, which is precisely what nothing else did.
    ///
    /// So this asserts the WIRING rather than the lint: parse a bringup through
    /// the real `collect_images` path and require the warning to come back.
    fn pkg(dir: &std::path::Path) -> Vec<cargo_nano_ros::provider_scan::WorkspacePackage> {
        vec![cargo_nano_ros::provider_scan::WorkspacePackage {
            name: "demo_bringup".to_string(),
            dir: dir.to_path_buf(),
            build_type: None,
            depends: Default::default(),
        }]
    }

    fn write_system(dir: &std::path::Path, body: &str) {
        std::fs::create_dir_all(dir).expect("mkdir");
        std::fs::write(dir.join("system.toml"), body).expect("write");
    }

    #[test]
    fn a_deploy_build_field_warns_through_the_build_path() {
        let d = std::env::temp_dir().join(format!("nros-w10b-{}-{}", std::process::id(), line!()));
        write_system(
            &d,
            "[system]\nname = \"s\"\nrmw = \"zenoh\"\ndomain_id = 0\n\n\
             [deploy.legacy]\nkind = \"embedded\"\nboard = \"mps2-an385-freertos\"\n",
        );

        let (_, warnings) = collect_images_with_warnings(&pkg(&d), false).expect("collects");
        assert_eq!(warnings.len(), 1, "expected one warning, got {warnings:?}");
        assert!(
            warnings[0].contains("[deploy.legacy]") && warnings[0].contains("board"),
            "warning must name the block and the field: {}",
            warnings[0]
        );

        // Suppression reaches the same path.
        let (_, quiet) = collect_images_with_warnings(&pkg(&d), true).expect("collects");
        assert!(quiet.is_empty(), "suppressed run must be silent: {quiet:?}");

        std::fs::remove_dir_all(&d).ok();
    }

    /// A block that HAS its `[image.*]` is migrated, and a placement-only block
    /// was never in scope — `[deploy.*]` keeps `kind`/`nodes`/`launch` and is
    /// not being retired. Without this the test above would pass on a lint that
    /// warned about everything.
    #[test]
    fn migrated_and_placement_only_blocks_stay_silent() {
        let d = std::env::temp_dir().join(format!("nros-w10b-{}-{}", std::process::id(), line!()));
        write_system(
            &d,
            "[system]\nname = \"s\"\nrmw = \"zenoh\"\ndomain_id = 0\n\n\
             [deploy.migrated]\nkind = \"embedded\"\nboard = \"mps2-an385-freertos\"\n\n\
             [deploy.placement]\nkind = \"self\"\nnodes = [\"a\"]\n\n\
             [image.migrated]\nboard = \"mps2-an385-freertos\"\n",
        );

        let (_, warnings) = collect_images_with_warnings(&pkg(&d), false).expect("collects");
        assert!(warnings.is_empty(), "expected silence, got {warnings:?}");

        std::fs::remove_dir_all(&d).ok();
    }
}

#[cfg(test)]
mod zephyr_base_tests {
    use super::*;

    fn zdir(base: &std::path::Path, name: &str) -> PathBuf {
        let ws = base.join(name);
        std::fs::create_dir_all(ws.join("zephyr")).expect("mkdir");
        ws
    }

    /// The ladder is `west-fixtures.sh`'s, and what it produces is a
    /// ZEPHYR_BASE — not a `.west/` directory. Measured: with `ZEPHYR_BASE`
    /// exported, `west build` runs from anywhere; without it, west refuses even
    /// inside the repo. That is also why a FREESTANDING application works
    /// (issue 0892 / RFC-0085).
    #[test]
    fn the_ladder_resolves_a_zephyr_not_a_workspace_marker() {
        let base = std::env::temp_dir().join(format!("nros-zb-{}-{}", std::process::id(), line!()));
        let root = base.join("nano-ros");
        std::fs::create_dir_all(&root).expect("mkdir");

        // Nothing anywhere.
        assert_eq!(zephyr_base_with(None, None, None, &root), None);

        // In-repo `zephyr-workspace/` — the common contributor layout.
        let inrepo = zdir(&root, "zephyr-workspace");
        assert_eq!(
            zephyr_base_with(None, None, None, &root),
            Some(inrepo.join("zephyr"))
        );

        // `NROS_ZEPHYR_WORKSPACE` — the ESTABLISHED spelling (60 references in
        // the tree), not a new one — outranks the in-repo default.
        let explicit = zdir(&base, "elsewhere");
        assert_eq!(
            zephyr_base_with(None, None, Some(explicit.clone().into_os_string()), &root),
            Some(explicit.join("zephyr")),
        );

        // An already-exported ZEPHYR_BASE wins outright: the user has chosen.
        let direct = base.join("chosen");
        std::fs::create_dir_all(&direct).expect("mkdir");
        assert_eq!(
            zephyr_base_with(
                None,
                Some(direct.clone().into_os_string()),
                Some(explicit.into_os_string()),
                &root
            ),
            Some(direct),
        );

        std::fs::remove_dir_all(&base).ok();
    }

    /// A pointer at something that is not a Zephyr must not be honoured — it is
    /// a typo, and taking it moves the failure into west with a worse message.
    #[test]
    fn a_pointer_without_a_zephyr_dir_is_not_honoured() {
        let base = std::env::temp_dir().join(format!("nros-zb-{}-{}", std::process::id(), line!()));
        let root = base.join("nano-ros");
        let empty = base.join("not-a-workspace");
        std::fs::create_dir_all(&root).expect("mkdir");
        std::fs::create_dir_all(&empty).expect("mkdir");

        assert_eq!(
            zephyr_base_with(None, None, Some(empty.clone().into_os_string()), &root),
            None,
            "a workspace with no zephyr/ is not a workspace",
        );
        assert_eq!(
            zephyr_base_with(None, Some(empty.join("nope").into_os_string()), None, &root),
            None,
            "a ZEPHYR_BASE that does not exist is not a Zephyr",
        );
        std::fs::remove_dir_all(&base).ok();
    }
}

#[cfg(test)]
mod west_application_tests {
    use super::*;

    /// The CMake half of the deploy declaration must be READ, not just the
    /// Cargo half.
    ///
    /// Reading only `Cargo.toml` made the west application resolver silently
    /// Rust-only: five workspaces (`c`, `cpp`, `mixed`, `realtime-c`,
    /// `realtime-cpp`) declare their entry with
    /// `nano_ros_add_executable(... DEPLOY zephyr)` and have no `Cargo.toml`
    /// for it, so every one of them fell through to the bringup directory —
    /// a real directory, so nothing errored and west was simply pointed at the
    /// wrong tree.
    #[test]
    fn a_cmake_entry_declares_its_deploy_token() {
        let text = r#"
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
nano_ros_add_executable(zephyr_entry
    BOARD   zephyr
    LANG    c
    DEPLOY  zephyr)
"#;
        assert_eq!(cmake_deploy_token(text).as_deref(), Some("zephyr"));
    }

    /// `DEPLOY` in prose is not a declaration.
    ///
    /// `examples/workspaces/cpp/src/zephyr_entry/CMakeLists.txt` contains the
    /// comment "nano_ros_node_register has no DEPLOY → component-only", one
    /// line above the real call. A file-wide regex reads that as the answer;
    /// scoping to the call arguments is what makes the difference, so it is
    /// what gets tested.
    #[test]
    fn deploy_in_a_comment_is_not_a_declaration() {
        let text = r#"
# nano_ros_node_register has no DEPLOY -> component-only; the sidecar links them.
nano_ros_add_executable(entry
    BOARD zephyr
    DEPLOY zephyr)
"#;
        assert_eq!(cmake_deploy_token(text).as_deref(), Some("zephyr"));

        // …and a file with ONLY the comment declares nothing.
        let comment_only = "# has no DEPLOY zephyr, component-only\n";
        assert_eq!(cmake_deploy_token(comment_only), None);
    }

    /// A file with no entry call at all resolves to nothing rather than
    /// panicking on the `?` chain.
    #[test]
    fn a_plain_cmakelists_declares_nothing() {
        assert_eq!(
            cmake_deploy_token("project(foo)\nadd_executable(a a.c)\n"),
            None
        );
    }

    /// A quoted token is the same token.
    #[test]
    fn a_quoted_deploy_token_is_unquoted() {
        let text = "nano_ros_entry(e\n    DEPLOY \"zephyr\")\n";
        assert_eq!(cmake_deploy_token(text).as_deref(), Some("zephyr"));
    }
}

#[cfg(test)]
mod zephyr_workspace_flag_tests {
    use super::*;

    /// `--zephyr-workspace` outranks both environment variables.
    ///
    /// An env is ambient: it survives a shell, it is not visible in the command
    /// that ran, and one left over from another project would otherwise decide
    /// this build. What the invocation says has to win over what the shell
    /// remembers.
    #[test]
    fn the_flag_outranks_both_env_vars() {
        let tmp = tempfile::tempdir().unwrap();
        let flagged = tmp.path().join("flagged");
        let env_ws = tmp.path().join("from-env");
        std::fs::create_dir_all(flagged.join("zephyr")).unwrap();
        std::fs::create_dir_all(env_ws.join("zephyr")).unwrap();
        let stale_base = tmp.path().join("stale");
        std::fs::create_dir_all(&stale_base).unwrap();

        assert_eq!(
            zephyr_base_with(
                Some(&flagged),
                Some(stale_base.into_os_string()),
                Some(env_ws.into_os_string()),
                tmp.path(),
            ),
            Some(flagged.join("zephyr")),
        );
    }

    /// Passing the `zephyr/` directory itself resolves too.
    ///
    /// The flag names a WORKSPACE, but "the directory containing zephyr" and
    /// "the Zephyr directory" are one place under two descriptions, and
    /// confusing them is the commonest way to get this wrong. Refusing would
    /// be pedantry about a distinction only the resolver cares about.
    #[test]
    fn the_flag_also_accepts_the_zephyr_tree_itself() {
        let tmp = tempfile::tempdir().unwrap();
        let zephyr = tmp.path().join("ws").join("zephyr");
        std::fs::create_dir_all(&zephyr).unwrap();
        // `Kconfig.zephyr` is the marker, not the directory NAME — a workspace
        // may check Zephyr out anywhere.
        std::fs::write(zephyr.join("Kconfig.zephyr"), "").unwrap();

        assert_eq!(
            zephyr_base_with(Some(&zephyr), None, None, tmp.path()),
            Some(zephyr),
        );
    }

    /// A directory that is neither a workspace nor a Zephyr resolves to
    /// nothing, so the caller reports the miss instead of running west against
    /// a path that cannot work.
    #[test]
    fn a_flag_naming_neither_resolves_to_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let empty = tmp.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(zephyr_base_with(Some(&empty), None, None, tmp.path()), None);
    }
}

/// phase-420 W7 — the selection verbs, asserted where they are WIRED.
///
/// `builder::discover` unit-tests the filter itself. These assert the two
/// things a unit test of a pure function cannot: that clap spells the flags the
/// way colcon does, and that `plan_builds` actually applies them. This crate
/// has shipped a lint with four passing unit tests and no production caller
/// before (`deprecation_wiring_tests`, below), which is why the wiring gets its
/// own assertions rather than being assumed.
#[cfg(test)]
mod selection_wiring_tests {
    use super::*;
    use clap::Parser;

    fn args_for(root: &std::path::Path, select: &[&str], up_to: &[&str]) -> Args {
        Args {
            images: Vec::new(),
            workspace: Some(root.to_path_buf()),
            nano_ros_path: None,
            zephyr_workspace: None,
            all: false,
            dry_run: true,
            offline: true,
            packages_select: select.iter().map(|s| (*s).to_string()).collect(),
            packages_up_to: up_to.iter().map(|s| (*s).to_string()).collect(),
            native_args: Vec::new(),
        }
    }

    fn pkg(root: &std::path::Path, name: &str, depends: &[&str]) {
        let dir = root.join("src").join(name);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let deps: String = depends
            .iter()
            .map(|d| format!("  <depend>{d}</depend>\n"))
            .collect();
        std::fs::write(
            dir.join("package.xml"),
            format!(
                "<?xml version=\"1.0\"?>\n<package format=\"3\">\n  \
                 <name>{name}</name>\n  <version>0.0.0</version>\n  \
                 <description>t</description>\n  \
                 <maintainer email=\"a@b.c\">m</maintainer>\n  \
                 <license>Apache-2.0</license>\n{deps}</package>\n"
            ),
        )
        .expect("write package.xml");
    }

    /// colcon's spelling, and both flags taking several names.
    #[test]
    fn the_flags_parse_with_colcons_spelling() {
        let a = Args::try_parse_from([
            "build",
            "--packages-select",
            "talker_pkg",
            "msgs_pkg",
            "--packages-up-to",
            "entry",
        ])
        .expect("parses");
        assert_eq!(a.packages_select, vec!["talker_pkg", "msgs_pkg"]);
        assert_eq!(a.packages_up_to, vec!["entry"]);
        assert!(a.images.is_empty(), "no image was named: {:?}", a.images);
    }

    /// A selection composes with the flags the verb already has — here the
    /// positional image, which must not be swallowed by the multi-valued flag.
    #[test]
    fn an_image_argument_survives_beside_a_selection() {
        let a = Args::try_parse_from(["build", "--packages-up-to", "entry", "--", "-j4"])
            .expect("parses");
        assert_eq!(a.packages_up_to, vec!["entry"]);
        assert_eq!(a.native_args, vec!["-j4"]);

        let b = Args::try_parse_from(["build", "native", "--packages-select", "talker_pkg"])
            .expect("parses");
        assert_eq!(b.images, vec!["native"]);
        assert_eq!(b.packages_select, vec!["talker_pkg"]);
    }

    /// The wiring: an unknown name must be refused by `plan_builds` itself.
    ///
    /// It is also refused EARLY — before the board catalog, which needs a
    /// nano-ros checkout this temp workspace does not have. If the selection
    /// ran later, this test would see "no nano-ros checkout found" instead, so
    /// the assertion doubles as the ordering check.
    #[test]
    fn plan_builds_refuses_an_unknown_selected_package() {
        let tmp = tempfile::tempdir().expect("tempdir");
        pkg(tmp.path(), "talker_pkg", &[]);
        pkg(tmp.path(), "msgs_pkg", &[]);

        let e = plan_builds(&args_for(tmp.path(), &["talkr_pkg"], &[]))
            .expect_err("an unknown package must not be warned past");
        let msg = format!("{e:#}");
        assert!(msg.contains("no such package"), "{msg}");
        assert!(msg.contains("talker_pkg"), "lists what exists: {msg}");
    }

    /// The other wiring direction: an incomplete `--packages-select` is refused
    /// by `plan_builds`, not left to fail later as a missing artifact.
    #[test]
    fn plan_builds_refuses_a_selection_that_drops_a_dependency() {
        let tmp = tempfile::tempdir().expect("tempdir");
        pkg(tmp.path(), "entry", &["talker_pkg"]);
        pkg(tmp.path(), "talker_pkg", &[]);

        let e = plan_builds(&args_for(tmp.path(), &["entry"], &[]))
            .expect_err("an open selection must not build");
        let msg = format!("{e:#}");
        assert!(msg.contains("entry needs talker_pkg"), "{msg}");
        assert!(msg.contains("--packages-up-to entry"), "{msg}");
    }

    /// And a well-formed selection is NOT refused here — it passes stage 1b and
    /// fails later, which is a different message. Without this, the two tests
    /// above would pass on a `select` that refused everything.
    ///
    /// "Later" is package mode (RFC-0065 D1, phase-445 W5): this workspace has
    /// no bringup, so `nros build` builds each package with its own build file
    /// — and these fixture packages carry only a `package.xml`, so that is the
    /// refusal. It used to be "declares no [image.*]", from the time a
    /// bringup-less workspace was an error rather than colcon's shape.
    #[test]
    fn a_closed_selection_passes_stage_1b() {
        let tmp = tempfile::tempdir().expect("tempdir");
        pkg(tmp.path(), "entry", &["talker_pkg"]);
        pkg(tmp.path(), "talker_pkg", &[]);

        let e = plan_builds(&args_for(tmp.path(), &[], &["entry"]))
            .expect_err("these packages carry no build file");
        let msg = format!("{e:#}");
        assert!(
            !msg.contains("no such package") && !msg.contains("needs talker_pkg"),
            "the selection is closed and must not be the complaint: {msg}"
        );
        assert!(msg.contains("no buildable package"), "{msg}");
    }
}

/// Issue 1206 — the driver loop, which had no test at all.
///
/// `Args { all: false, … }` was the only construction in this file's tests, so
/// every multi-plan path was reached exclusively through `--dry-run`, which
/// `continue`s past the handover and therefore cannot see this class. These
/// assert the non-dry-run path.
#[cfg(test)]
mod multi_image_drive_tests {
    use super::*;
    use crate::builder::handoff::Handoff;

    fn plan(id: &str, hand: Handoff) -> ResolvedBuild {
        ResolvedBuild {
            qualified: format!("demo:{id}"),
            board: "native".to_string(),
            platform: "posix".to_string(),
            driver: Driver::Cargo,
            handoff: Some(hand),
            rmw: None,
            entry_package: None,
            target: None,
            profile: None,
            configure: None,
        }
    }

    fn nullary(program: &str) -> Handoff {
        Handoff::new(program, Vec::<String>::new())
    }

    /// The bug, stated as the invariant it violated: EVERY plan reaches a
    /// handover, and only the last one is allowed to be the `exec`.
    ///
    /// Before the fix this recorded ONE entry — the loop called
    /// `handoff::exec` on every iteration, and the first one never returned,
    /// so plans 2..N were unreachable by construction.
    #[test]
    fn every_plan_reaches_a_handover_and_only_the_last_execs() {
        let plans = vec![
            plan("one", nullary("true")),
            plan("two", nullary("true")),
            plan("three", nullary("true")),
        ];
        let mut seen: Vec<(String, Handover)> = Vec::new();
        drive(&plans, false, &mut |h, mode| {
            seen.push((h.display(), mode));
            Ok(())
        })
        .expect("three plans, three handovers");

        assert_eq!(
            seen.len(),
            3,
            "one handover per plan, not one per invocation: {seen:?}"
        );
        assert_eq!(
            seen.iter().map(|(_, m)| *m).collect::<Vec<_>>(),
            vec![Handover::Wait, Handover::Wait, Handover::Exec],
            "exec is spent once, on the LAST plan: {seen:?}"
        );
    }

    /// A single image is unchanged: it execs, which is RFC-0065 D1's
    /// guarantee and the invocation the RFC's prose describes.
    #[test]
    fn a_lone_plan_still_execs() {
        let plans = vec![plan("only", nullary("true"))];
        let mut seen = Vec::new();
        drive(&plans, false, &mut |_, mode| {
            seen.push(mode);
            Ok(())
        })
        .expect("one plan");
        assert_eq!(seen, vec![Handover::Exec]);
    }

    /// `--dry-run` prints and hands over to nothing — the property that keeps
    /// it a print of the real plan rather than a second code path.
    #[test]
    fn a_dry_run_performs_no_handover() {
        let plans = vec![plan("one", nullary("true")), plan("two", nullary("true"))];
        let mut count = 0usize;
        drive(&plans, true, &mut |_, _| {
            count += 1;
            Ok(())
        })
        .expect("dry run");
        assert_eq!(count, 0, "a dry run must not run anything");
    }

    /// A failing non-final image stops the run and names itself.
    ///
    /// The exit code used to be image 1's verdict whatever happened later,
    /// because nothing later ever happened.
    #[test]
    fn a_failing_earlier_image_stops_the_run_and_is_named() {
        let plans = vec![plan("one", nullary("true")), plan("two", nullary("true"))];
        let mut seen = 0usize;
        let e = drive(&plans, false, &mut |_, _| {
            seen += 1;
            eyre::bail!("boom")
        })
        .expect_err("the first handover failed");
        assert_eq!(seen, 1, "nothing after the failure is attempted");
        let msg = format!("{e:#}");
        assert!(msg.contains("demo:one"), "{msg}");
        assert!(msg.contains("image 1 of 2"), "{msg}");
    }

    /// The `Wait` handover really runs the command — no compiler, and the
    /// artifact asserted rather than the intention.
    ///
    /// This is as close as a unit test gets to "assert both artifacts exist":
    /// the project bans compiling inside a test, so the end-to-end form of
    /// that assertion lives in issue 1206's reproduction rather than here.
    /// What is checkable here is that a non-final plan's command executes and
    /// has effects — exactly what did not happen before.
    #[test]
    fn a_waited_handover_actually_runs_the_command() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let marker = tmp.path().join("built");
        perform(
            &Handoff::new("touch", [marker.clone().into_os_string()]),
            Handover::Wait,
        )
        .expect("touch runs");
        assert!(marker.is_file(), "the command ran and had its effect");
    }

    /// And a non-zero exit is a failure, not a silent success — the other half
    /// of "the exit code carries the first image's verdict".
    #[test]
    fn a_waited_handover_fails_on_a_non_zero_exit() {
        let e = perform(&nullary("false"), Handover::Wait).expect_err("`false` exits 1");
        assert!(format!("{e:#}").contains("exited"), "{e:#}");
    }
}

/// RFC-0097 D11's BUILD half — phase-443 W4.
///
/// `toolchain_pin_dispatch.rs` covers the decision (which outcome each session
/// produces). This covers what `nros build` DOES with it, which is a separate
/// claim and the one that carries the acceptance: a refusal that reaches the
/// user as a printed warning is not a refusal, and every test one crate over
/// would still pass.
#[cfg(test)]
mod pin_report_tests {
    use super::report_pin_outcome;
    use crate::orchestration::pin::{Pin, PinOutcome};
    use std::path::PathBuf;

    /// The refusal STOPS the build, and the error the user sees is the full
    /// diagnostic — the variable that decided, the file to write, the escape
    /// hatch — not a summary that sends them looking for it.
    #[test]
    fn a_ci_refusal_becomes_an_error_carrying_the_whole_diagnostic() {
        let outcome = PinOutcome::RefusedInCi {
            version: "0.5.0-nros1".to_string(),
            dir: PathBuf::from("/w/my-robot"),
            var: "CI",
        };
        let err = report_pin_outcome(&outcome).expect_err("a refusal must fail the build");
        let text = format!("{err:#}");
        assert!(
            text.contains("$CI"),
            "name the variable that decided:\n{text}"
        );
        assert!(
            text.contains("version = \"0.5.0-nros1\""),
            "print the pin to write:\n{text}"
        );
        assert!(
            text.contains("NROS_ALLOW_PIN_WRITE_IN_CI"),
            "name the escape hatch:\n{text}"
        );
    }

    /// The negative control, and the reason the refusal is a single arm rather
    /// than "is this CI?" asked here: every other outcome is advisory. A
    /// `report_pin_outcome` that failed on all of them would pass the test
    /// above and break every build in this repository's own CI, which builds
    /// inside a checkout.
    #[test]
    fn every_other_outcome_lets_the_build_proceed() {
        let wrote = PinOutcome::Wrote {
            path: PathBuf::from("/w/my-robot/nros-toolchain.toml"),
            version: "0.5.0-nros1".to_string(),
        };
        assert!(
            report_pin_outcome(&wrote)
                .expect("writing a pin is not a failure")
                .is_some_and(|l| l.contains("0.5.0-nros1")),
            "a written pin is reported"
        );

        let already = PinOutcome::Already(Pin {
            path: PathBuf::from("/w/my-robot/nros-toolchain.toml"),
            version: "0.5.0-nros1".to_string(),
        });
        assert!(
            report_pin_outcome(&already)
                .expect("a pinned project builds")
                .is_some()
        );

        // A contributor is told nothing at all — their nano-ros is the clone
        // they are standing in.
        assert_eq!(
            report_pin_outcome(&PinOutcome::InCheckout(PathBuf::from("/src/nano-ros")))
                .expect("a checkout builds"),
            None
        );

        assert!(
            report_pin_outcome(&PinOutcome::NoRunningVersion)
                .expect("an unpinnable binary still builds")
                .is_some(),
            "the user is warned that the project is still unpinned"
        );
    }
}

/// phase-445 W4b — a single-package example is a workspace of one.
#[cfg(test)]
mod single_package_tests {
    use super::*;

    /// The nano-ros checkout this crate is built from — its board catalog is
    /// what resolves `board = "native"`.
    fn nros_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap()
    }

    /// A leaf with no `[[component]]`: no model is expected, so the plan needs
    /// no prior `nros sync` and the test needs no resolver.
    fn leaf(parent: &std::path::Path) -> PathBuf {
        let leaf = parent.join("talker");
        std::fs::create_dir_all(leaf.join("src")).unwrap();
        std::fs::write(
            leaf.join("Cargo.toml"),
            "[package]\nname = \"demo_talker\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .unwrap();
        std::fs::write(leaf.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(
            leaf.join("system.toml"),
            "[system]\nname = \"demo\"\nrmw = \"zenoh\"\ndomain_id = 0\n\n\
             [image.native]\nboard = \"native\"\n",
        )
        .unwrap();
        leaf
    }

    fn args(leaf: &std::path::Path, images: &[&str]) -> Args {
        Args {
            images: images.iter().map(|s| (*s).to_string()).collect(),
            workspace: Some(leaf.to_path_buf()),
            nano_ros_path: Some(nros_root()),
            zephyr_workspace: None,
            all: false,
            dry_run: true,
            offline: false,
            packages_select: Vec::new(),
            packages_up_to: Vec::new(),
            native_args: Vec::new(),
        }
    }

    #[test]
    fn the_leafs_own_manifest_is_built_with_its_generated_settings_from_above() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = leaf(tmp.path());
        let plans = plan_builds(&args(&leaf, &[])).expect("a single-package leaf plans");
        assert_eq!(plans.len(), 1);
        let p = &plans[0];
        assert_eq!(p.driver, Driver::Cargo);
        assert_eq!(p.qualified, "demo_talker:native");
        let shown = p.handoff.as_ref().unwrap().display();
        assert!(
            shown.contains(
                "cargo build --manifest-path talker/Cargo.toml --config \
                 talker/build/native/nros-cargo.toml"
            ),
            "{shown}"
        );
        // The leaf's `[workspace]` marker is its own root, not a root build
        // file to retire (RFC-0098 D9 is about workspaces).
        assert!(leaf.join("Cargo.toml").is_file());

        let settings = leaf.join("build/native/nros-cargo.toml");
        let v: toml::Value = std::fs::read_to_string(&settings)
            .unwrap()
            .parse()
            .expect("the settings file is TOML");
        assert_eq!(v["build"]["target-dir"].as_str(), Some("native/target"));
        assert_eq!(v["env"]["NROS_BOARD"].as_str(), Some("native"));
        assert!(
            v["profile"].get("nros-minsizerel").is_some(),
            "the presets travel with the file"
        );
    }

    #[test]
    fn an_image_the_leaf_does_not_declare_is_refused_naming_the_one_it_does() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = leaf(tmp.path());
        let e = plan_builds(&args(&leaf, &["esp32"]))
            .expect_err("wrong image")
            .to_string();
        assert!(e.contains("`native`"), "{e}");
    }

    #[test]
    fn a_leaf_whose_components_were_never_synced_names_nros_sync() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = leaf(tmp.path());
        let mut sys = std::fs::read_to_string(leaf.join("system.toml")).unwrap();
        sys.push_str("\n[[component]]\npkg = \"demo_talker\"\nname = \"talker\"\n");
        std::fs::write(leaf.join("system.toml"), sys).unwrap();
        let e = plan_builds(&args(&leaf, &[]))
            .expect_err("no model yet")
            .to_string();
        assert!(e.contains("nros sync"), "{e}");
    }
}
