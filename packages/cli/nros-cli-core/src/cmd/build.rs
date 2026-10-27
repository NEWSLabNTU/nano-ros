//! `nros build` — the workspace build entry point (RFC-0065, phase-383 W2.c).
//!
//! Five stages, and the last one replaces this process:
//!
//! ```text
//!   1. DISCOVER   package.xml ∪ cargo members → topological order
//!   2. RESOLVE    the image: argument > default_images > list and fail
//!   3. PREFLIGHT  toolchains / SDKs / sources present?
//!   4. GENERATE   msg bindings + model + the ROOT BUILD FILE  (W3/W4)
//!   5. EXEC       cargo / cmake / west — stderr untouched
//! ```
//!
//! Stage 4 is not wired yet. That is deliberate and shippable: RFC-0065 D3 says
//! a root is emitted only where one would otherwise be hand-written, and west
//! and ESP-IDF apps ship their own, so those targets go 1→2→3→5 today and work.
//! A cargo/cmake image reports what stage 4 will do and stops, rather than
//! silently building the wrong thing.

use std::path::{Path, PathBuf};

use clap::Parser;
use eyre::{Result, WrapErr};

use crate::builder::{
    discover,
    handoff::Handoff,
    plan::{self, Driver},
};

#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// Image(s) to build — `linux`, or `<bringup>:linux` when two bringups
    /// declare the same id. Empty uses `[system] default_images`.
    pub images: Vec<String>,

    /// Workspace root. Defaults to the current directory.
    #[arg(long)]
    pub workspace: Option<PathBuf>,

    /// nano-ros SDK root holding `packages/boards`. Defaults to a walk-up
    /// from the workspace, then this `nros` binary's own SDK root.
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

    /// Build the image whose `[image.*] board` is this board (phase-517 W6).
    ///
    /// Matches the authored word (any case), any alias of the same board
    /// descriptor, or the `nros setup` index key the board provisions through
    /// (`nros build --board mps2-an385-baremetal` finds a `baremetal`
    /// image). One match builds exactly that image, as if it were named; several
    /// or none is refused, listing them — the image id stays the unit
    /// (RFC-0065). Not with image arguments or `--all`.
    #[arg(long, value_name = "BOARD")]
    pub board: Option<String>,

    /// Print the stages and the command that would run, then stop.
    ///
    /// Safe by construction: a `Handoff` performs no I/O until `exec`. It
    /// still GENERATES (stage 4 writes each image's generated files), which
    /// the Zephyr fixture runner relies on to regenerate an application
    /// without building it (issue 1707); a no-write answer is `nros
    /// image-facts` (issue 1716).
    #[arg(long)]
    pub dry_run: bool,

    /// Also print the explanation behind each configure note: why a pass
    /// re-runs, why a size was not derived (phase-517 W5). Sets
    /// `NROS_VERBOSE=1` for every native command; a correct build without it
    /// prints one line per fact.
    #[arg(short, long)]
    pub verbose: bool,

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
    /// selection, so `nros build linux --packages-select talker_pkg` still
    /// means the `linux` image, built from a narrowed workspace.
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

    /// Type-check the image instead of building it: `cargo check`, or
    /// `cargo clippy` with `--check=clippy`.
    ///
    /// Everything before the cargo command is the same as a build — the
    /// generated message crates, the `<pkg>_nros_selection` crate, the
    /// image's settings file and its lock — because none of it is optional
    /// for a check either: a leaf whose manifest names the selection crate
    /// cannot even be LOADED until sync has written it, so a bare
    /// `cargo check` in a fresh checkout fails at manifest parse with an
    /// error that never says why. Only the final verb changes, so a check
    /// here and a build here see one dependency graph.
    ///
    /// Cargo images only; a cmake or west image is refused, since neither
    /// tool has a check that is not a build.
    #[arg(
        long,
        value_enum,
        value_name = "TOOL",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "check"
    )]
    pub check: Option<CheckTool>,

    /// Arguments after `--` go to the native tool verbatim.
    #[arg(last = true)]
    pub native_args: Vec<String>,
}

/// The cargo command `nros build --check` runs in place of `cargo build`.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckTool {
    /// `cargo check`.
    Check,
    /// `cargo clippy`. Lint flags go after a second `--`:
    /// `nros build --check=clippy -- -- -D warnings`.
    Clippy,
}

impl CheckTool {
    fn verb(self) -> &'static str {
        match self {
            CheckTool::Check => "check",
            CheckTool::Clippy => "clippy",
        }
    }
}

/// `nros build --check`: swap the final `cargo build` of every plan for
/// `tool`, and refuse an image no cargo command drives.
///
/// One pass over the finished plans rather than a branch at each of the three
/// places a cargo handoff is assembled (generated entry, single-package leaf,
/// workspace member): every one of them starts its arguments with `build`,
/// so the rewrite is the same everywhere, and a fourth road gets it for free.
fn retarget_to_check(plans: &mut [ResolvedBuild], tool: CheckTool) -> Result<()> {
    for p in plans.iter_mut() {
        if p.driver != Driver::Cargo {
            eyre::bail!(
                "`--check` type-checks a cargo image, and `{}` is built by {},                  which has no check that is not a build. Run `nros build {}`                  without `--check`.",
                p.qualified,
                p.driver.program(),
                p.qualified
            );
        }
        let Some(hand) = p.handoff.as_mut() else {
            continue;
        };
        match hand.args.first() {
            Some(a) if a == "build" => hand.args[0] = tool.verb().into(),
            other => eyre::bail!(
                "`{}`: expected the cargo handoff to start with `build`, found                  {other:?} — the `--check` rewrite does not know this road",
                p.qualified
            ),
        }
    }
    Ok(())
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
    /// phase-481 W4 — where a CMAKE-driver workspace image builds,
    /// `build/<coord>/cmake`, from [`cmake_coordinate`]: the one derivation,
    /// handed to readers (`nros image-facts`, the fixture-row check) so none
    /// restates it. `None` for every other driver.
    pub cmake_build_dir: Option<String>,
    /// The capability axes the image's `system.toml` declares
    /// ([`declared_capabilities`]) — what codegen generates calls for.
    ///
    /// Issue 1261: a west build picks nros-c/nros-cpp's cargo features from
    /// `NROS_FEATURES` during `find_package(Zephyr)`, and a hand-written
    /// application had no way to learn them but restating them. Exposed so
    /// `nros image-facts` hands the module the image's answer.
    pub capabilities: Vec<String>,

    /// A configure that must run BEFORE the handoff, for drivers that need one.
    ///
    /// cmake is the only such driver: `cmake --build` on an unconfigured tree
    /// fails, and configure+build is two invocations at our 3.22 floor
    /// (`--workflow` is 3.25+). Stage 5 execs ONE command and cannot do both,
    /// so the configure belongs to generation — which is what it is: writing
    /// the build system next to the root that was just written.
    ///
    /// Kept on the plan rather than performed during planning so planning
    /// never RUNS a native tool and `--dry-run` can PRINT it. [`run`] performs it.
    pub configure: Option<Handoff>,

    /// phase-485 W3 (RFC-0104 D4) — what the finished build leaves to read
    /// for the project lock: the cargo roots it resolved and the build
    /// directories whose records say which sources it read.
    pub record: crate::orchestration::project_lock::BuildRecord,

    /// phase-517 W6 — where the build puts its artifact, recorded from the
    /// values the native command was given (target dir, triple, profile, bin;
    /// cmake binary dir + executable; west `-d`). `None` for a query and for
    /// package mode. Read by [`crate::cmd::build_next::next_steps`].
    pub output: Option<crate::cmd::build_next::BuildOutput>,
    /// phase-517 W6 — the board descriptor the image resolved to, for the
    /// follow-up command (`runner`, triple, platform). `None` for a query.
    pub descriptor: Option<crate::orchestration::board_descriptor::BoardDescriptor>,
}

/// What a plan is FOR — issue 1716.
///
/// Planning a build GENERATES: stage 4 writes each image's entry package, its
/// settings file, the generated west application, the resolved model and the
/// sizing descriptor, and retires a stale generated root. That is the point of
/// stage 4, and `--dry-run` relies on it — "generate, then print instead of
/// run" is how the Zephyr fixture runner regenerates an application before
/// `ninja` (issue 1707). A QUERY must not: `nros image-facts` runs inside a
/// Zephyr configure, after cmake has read the very application stage 4
/// rewrites, and its answer (board, platform, driver, rmw, entry, triple,
/// profile) is fixed by stage 3 anyway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Planning {
    /// Stages 1–4 — `nros build` (with or without `--dry-run`),
    /// `nros materialize`.
    Generate,
    /// Stages 1–3 — identify every image and stop. Writes nothing, deletes
    /// nothing; the plans carry no handoff and no configure.
    Query,
}

/// Stages 1-4: everything up to the handoff, which runs NOTHING — no native
/// tool is spawned and nothing is exec'd.
///
/// It is NOT free of side effects, and this comment said it was until issue
/// 1716: stage 4 writes every planned image's generated files (and stage 3.5
/// its resolved model) — the generation `--dry-run` deliberately keeps. For an
/// answer with no writes at all, use [`plan_query`].
///
/// Separated from [`run`] so the composition is testable without a built
/// binary and without exec'ing anything. That separation is also why
/// `--dry-run` is trivially correct rather than a second code path.
pub fn plan_builds(args: &Args) -> Result<Vec<ResolvedBuild>> {
    plan_for(args, Planning::Generate)
}

/// Stages 1–3 only: every image identified exactly as [`plan_builds`]
/// identifies it, and NOTHING written, generated, resolved to disk or retired
/// (issue 1716). Each plan's `handoff` and `configure` are `None` — a query
/// names the image, it does not say how to build it.
///
/// The one caller is `nros image-facts`, which a Zephyr configure runs; see
/// [`Planning`] for why it must not regenerate what that configure just read.
pub fn plan_query(args: &Args) -> Result<Vec<ResolvedBuild>> {
    plan_for(args, Planning::Query)
}

fn plan_for(args: &Args, planning: Planning) -> Result<Vec<ResolvedBuild>> {
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
    // ./build/posix-zenoh/linux_entry" — an error about the wrong thing.
    //
    // `canonicalize` rather than `absolute`: symlinked checkouts are normal
    // here, and two spellings of one directory would produce two different
    // relative paths in generated files that are supposed to be byte-identical.
    let root = std::fs::canonicalize(&root)
        .wrap_err_with(|| format!("resolving workspace root {}", root.display()))?;

    // phase-517 W6 — `--board` is a way of NAMING an image, so it is resolved
    // to one here and the rest of planning sees exactly `nros build <image>`.
    if let Some(want) = &args.board {
        if let Some(why) = crate::cmd::build_next::refuse_board_with(&args.images, args.all) {
            eyre::bail!("{why}");
        }
        let nros_root =
            crate::orchestration::nano_ros_root::resolve(args.nano_ros_path.clone(), &root);
        let image = crate::cmd::build_next::images_for_board(&root, nros_root.as_deref(), want)?;
        eprintln!("nros build: --board {want} selects `{image}`");
        let mut named = args.clone();
        named.board = None;
        named.images = vec![image];
        named.workspace = Some(root);
        return plan_for(&named, planning);
    }

    // phase-445 W4b (RFC-0098 D1) — a single-package example is a workspace of
    // one: its own `Cargo.toml` is the cargo root, and its settings come from
    // `build/<image>/nros-cargo.toml`. Decided before discovery, because the
    // workspace road below would treat the package's own `[workspace]` marker
    // as a root build file to retire (D9) and generate an entry it does not need.
    if let Some(plans) = plan_single_package(args, &root, planning)? {
        return Ok(plans);
    }
    // Issues 1296/1308 — and its C/C++ twin: the leaf's OWN `CMakeLists.txt`.
    if let Some(plans) = plan_single_package_cmake(args, &root)? {
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
        // Issue 1716 — a bringup-less workspace declares no image, so a query
        // has nothing to name; `plan_packages` would WRITE each package's
        // generated cmake root to answer a question nobody asked.
        if planning == Planning::Query {
            return Ok(Vec::new());
        }
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

    // issue 1582 — before ANY image is generated, and over EVERY image the
    // workspace declares rather than the ones requested: building one image is
    // enough to overwrite another's generated output, so the refusal cannot
    // wait for the second to be asked for. An image whose board or road does
    // not resolve claims nothing here; building it fails on its own terms.
    let claims: Vec<(String, Vec<PathBuf>)> = plan::all_images(&bringups)
        .into_iter()
        .filter_map(|(b, bd, id, img)| {
            let d = crate::orchestration::image::resolve_image_board(&catalog, &id, &img).ok()?;
            let platform = d.platform.kebab().to_string();
            let driver =
                plan::driver_for_board(&platform, d.entry_kind, image_has_non_rust(&img, &bd))
                    .ok()?;
            Some((
                plan::qualified(&b, &id),
                generated_outputs(&root, &id, &img, &platform, driver, &bd),
            ))
        })
        .collect();
    if let Some(msg) = generated_output_collision_message(&generated_output_collisions(&claims)) {
        eyre::bail!("{msg}");
    }

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
        // phase-463 W7 / RFC-0078 amendment 2026-10-07 — a HOST WCET profile
        // (`host-<arch>-<profile>`, what `$NROS_PROFILE_OUT` measures) is a fact
        // about the build host, never about a target's CPU. Only the native
        // image may select one; the bake of any other refuses, here, before
        // anything is generated. An unreadable `system.toml` is left to the
        // code whose job it is to report it.
        if let Ok(raw) = std::fs::read_to_string(bringup_dir.join("system.toml"))
            && let Ok(mut sys) =
                toml::from_str::<crate::orchestration::cargo_metadata_schema::SystemToml>(&raw)
        {
            // RFC-0109 D4 (phase-514 W2) — the board network files first.
            sys.fold_os_config(&bringup_dir)
                .map_err(|e| eyre::eyre!("`{qual}` cannot be built: {e}"))?;
            sys.wcet_profile_for_image(&image_id, &platform)
                .map_err(|e| eyre::eyre!("`{qual}` cannot be built: {e}"))?;
            // RFC-0103 D10b (phase-497 W1) — the address against its network.
            sys.validate_image_networks()
                .map_err(|e| eyre::eyre!("`{qual}` cannot be built: {e}"))?;
        }
        let board = image.board.clone().unwrap_or_default();
        // RFC-0103 D10 (phase-497 W2) — this image's network, resolved by the
        // one resolver over EVERY image of its bringup, so a duplicate address
        // or MAC on a shared network is refused before anything is built.
        let image_net =
            workspace_image_net(&catalog, &bringup_dir, &image_id, nano_ros_root.as_deref())
                .map_err(|e| eyre::eyre!("`{qual}` cannot be built: {e}"))?;
        // A refusal here is RFC-0065 D3's third outcome, not an internal error:
        // the board and the language mix together name a combination nano-ros
        // has no road for, and the message says which. Bailing beats answering
        // with a road that would fail one stage later for a reason about a tool
        // rather than about the image.
        let driver = plan::driver_for_board(
            &platform,
            descriptor.entry_kind,
            image_has_non_rust(&image, &bringup_dir),
        )
        .map_err(|e| eyre::eyre!("`{qual}` cannot be built: {e}"))?;

        // ---- stage 3 ----------------------------------------------------
        // Before anything is generated or compiled: a missing prerequisite
        // fails HERE, naming the command that fixes it (RFC-0065 D2).
        let missing = crate::builder::preflight::check(descriptor, &root, nano_ros_root.as_deref());
        if !missing.is_empty() {
            eyre::bail!("{}", crate::builder::preflight::report(&missing));
        }
        crate::cmd::setup::expose_store_tools(descriptor, nano_ros_root.as_deref());

        // Issue 1716 — a QUERY stops here. Everything it answers is fixed by
        // now (the push below reads only the image, its board descriptor and
        // its driver), and everything after this line WRITES: the resolved
        // model (3.5), then the generated entry, settings file, west
        // application and sizing descriptor, and the retirement of a stale
        // generated root (4).
        if planning == Planning::Query {
            out.push(ResolvedBuild {
                configure: None,
                cmake_build_dir: cmake_build_dir_for(driver, &platform, &image, &bringup_dir),
                qualified: qual,
                board,
                platform,
                driver,
                handoff: None,
                rmw: image.rmw.clone(),
                entry_package: Some(want_entry.clone()),
                target: descriptor.target.clone(),
                profile: image.profile.clone(),
                record: Default::default(),
                capabilities: owned_caps(&bringup_dir),
                output: None,
                descriptor: None,
            });
            continue;
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
            nano_ros_root.as_deref(),
            descriptor.rx,
            // phase-439 W7 -- the road whose configure reads the resolve
            // instead of recomposing: a west image crossing out of Rust, i.e.
            // the generated C/C++ application.
            driver == Driver::West && image_has_non_rust(&image, &bringup_dir),
        );
        let resolved_dir = resolved.as_ref().map(|r| r.dir.clone());

        // ---- stage 4 ----------------------------------------------------
        let mut cmake_configure: Option<Handoff> = None;
        // phase-485 W3 — the cargo roots this image resolves, for the project
        // lock's `[cargo.*]` record.
        let mut cargo_roots: Vec<std::path::PathBuf> = Vec::new();
        // ...and the directories THIS image builds into. Never the whole
        // `<ws>/build/`: that holds every other image's build too, and a
        // native image would be recorded as reading FreeRTOS.
        let mut record_dirs: Vec<std::path::PathBuf> = Vec::new();
        let mut cargo_prepare: Option<Handoff> = None;
        // phase-517 W6 — where this road puts the artifact, from the values
        // the native command below is given.
        let output: Option<crate::cmd::build_next::BuildOutput>;
        let handoff = match driver {
            Driver::Cargo => {
                // Issue 1419 — the cargo road's census check, BEFORE anything is
                // generated or compiled. A cross image's pools come from its
                // contract; the cmake road compares that contract with the code
                // at configure, and a cargo image never configures, so this is
                // its one point of comparison. A HOST image is the census
                // producer and checks nothing (the configure's `native` rule).
                if !args.dry_run
                    && stage4_checks_census(
                        Driver::Cargo,
                        descriptor.platform,
                        image_has_non_rust(&image, &bringup_dir),
                    )
                {
                    crate::cmd::entity_census::check_cargo_image(
                        &root,
                        &bringup_dir,
                        &image_id,
                        &image,
                    )
                    .wrap_err_with(|| {
                        format!(
                            "`{qual}`: the census and the contract disagree, or the census no \
                             longer describes the code (issue 1419)"
                        )
                    })?;
                }
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
                    // The cargo driver is chosen only when the graph does NOT
                    // cross languages (`driver_for`), so this is `false` by
                    // construction here — passed as the predicate rather than
                    // as a literal so the two cannot drift apart.
                    image_has_non_rust(&image, &bringup_dir),
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
                // consulted the image — so `[image.linux_cyclonedds]` produced
                // `build/posix-cyclonedds/` holding a zenoh binary. The facade
                // now reads the image (`facade::image_rmw`), so the refusal is
                // gone and the coordinate directory names what it contains.
                // ---- which manifest is this image's cargo root -----------
                //
                // The generated entry, or a hand-written / materialised
                // `src/<entry>` that suppressed generation (RFC-0065 D13). Each
                // is its own root: there is no workspace to `-p` into
                // (RFC-0098 D9), so the build names the MANIFEST.
                let image_dir = generated_entry_dir(&root, &platform, &image, &image_id);
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

                cargo_roots.push(manifest_dir.clone());
                record_dirs.push(image_dir.join("target"));

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
                // phase-486 -- no `NROS_DECLARED_*` fact rides the settings file
                // any more: the node count and the service families are the
                // image's sizing descriptor's (`[image] model_nodes`,
                // `infra_families`), written below.
                let mut env = std::collections::BTreeMap::new();
                if let Some(r) = &resolved {
                    env.extend(derived_pool_env(&r.resolved));
                }
                // phase-454 W14 (RFC-0100 D4) — the SECOND descriptor producer.
                //
                // A workspace image has no leaf inventories: its entity facts
                // come from the resolved SystemModel, and since
                // phase-457-payload W2 its bound tables from the workspace's
                // own `generated/` trees below. Each field is composed by the
                // leaf road's code from whatever of those it got, and refuses
                // only on an input that is missing (issue 1393, closed). D6
                // is what makes that safe to publish: `Fact::stated()` is the
                // only accessor that yields a value, so no consumer can read a
                // refusal as a default.
                //
                // Written UNDER `image_dir`, so the `[env]` row below is
                // relative to the cargo config's GRANDPARENT (`build/<coord>/`)
                // and the image stays self-contained — the same rule and the
                // same base the leaf road proved by copy-out (W11/W12).
                //
                // NO CONTRACT, NO FILE. `resolve_image` hands back an inventory
                // only when the model described wiring, and an all-refused
                // descriptor would move the `[meta] basis` every consumer
                // guards on in order to say nothing.
                let mut path_env: std::collections::BTreeMap<String, PathBuf> =
                    std::collections::BTreeMap::new();
                // The board facts, the same layer the single-package road
                // applies (`leaf_settings::board_fact_layer`), resolved from
                // this image's bringup. Without it the board crates' build
                // scripts saw no `NROS_BOARD_TOML` and compiled the platform
                // default network into a QEMU image (issue 1811).
                crate::cmd::leaf_settings::board_fact_layer(
                    &bringup_dir,
                    nros_root,
                    &board,
                    &mut env,
                    &mut path_env,
                )?;
                // RFC-0103 D10f (phase-497 W2) — the image's resolved network,
                // as the one text its board and `nros::main!` read.
                if let Some(n) = &image_net {
                    crate::orchestration::image_net::write(&root.join("build"), &image_id, n)
                        .map_err(|e| eyre::eyre!("{e}"))?;
                    env.insert(nros_board_common::image_net::ENV.to_string(), n.text());
                }
                // RFC-0109 D8 (phase-514 W3) — the session properties lowered
                // from the bringup's RMW file, for `nros::main!` to bake.
                if let Some(text) =
                    crate::orchestration::rmw_config::session_text(&bringup_dir, &image_id)
                        .map_err(|e| eyre::eyre!("{e}"))?
                {
                    env.insert(
                        nros_orchestration_ir::session_properties::ENV.to_string(),
                        text,
                    );
                }
                let image_for = |wiring| crate::sizing_descriptor::ModelImage {
                    build_dir: &image_dir,
                    entry: &image_id,
                    wiring,
                    target_triple: descriptor.target.clone(),
                    // A board that pins no rustc triple builds for the
                    // host, and that is the ONE case where this
                    // process's own width is the target's.
                    host_build: descriptor.target.is_none(),
                    heap_budget_bytes: board_heap_budget(descriptor, &board, Some(nros_root)),
                    rmw: image.rmw.clone(),
                    bound_inventories: &[],
                    horizon: crate::sizing_descriptor::ModelHorizon::new("a workspace cargo image"),
                };
                if let Some((inv, model)) = resolved
                    .as_ref()
                    .and_then(|r| r.inventory.as_ref().zip(r.model.as_ref()))
                {
                    // issue 1594 -- the probe sidecars' per-subscription
                    // registration observation, joined by the contract join's
                    // own rule. Not fatal: an unreadable workspace leaves every
                    // row unobserved, which refuses `registration_path` (the
                    // over-size, never the under-size).
                    let observed = match crate::contract_join::observe_workspace_registrations(
                        inv, model, &root,
                    ) {
                        Ok((observed, _, notes)) => {
                            for n in notes {
                                eprintln!("nros build:   {n}");
                            }
                            observed
                        }
                        Err(e) => {
                            eprintln!(
                                "nros build: warning: no registration observation joined \
                                 ({e}); every subscription keeps its receive region"
                            );
                            inv.clone()
                        }
                    };
                    let inv = &observed;
                    // phase-457-payload W2 — the workspace's own `generated/`
                    // tables, found by the discovery a single-package leaf uses
                    // and read by the reader every road shares. This is the
                    // third model-road producer; the two cmake ones take the
                    // tables their configure registered instead.
                    let bound_tables = crate::leaf_payload_classes::generated_bound_tables(&root);
                    match crate::sizing_descriptor::write_for_model(
                        &crate::sizing_descriptor::ModelImage {
                            bound_inventories: &bound_tables,
                            ..image_for(crate::sizing_descriptor::Wiring::Composed(inv))
                        },
                    ) {
                        Ok(Some(written)) => {
                            eprintln!(
                                "nros build:   sizing descriptor → {}",
                                written.path.display()
                            );
                            // phase-454 W6.c — the four Cyclone facts a
                            // `cc::Build` cannot read from a file. All four are
                            // refused on this road today (they are `[types]`
                            // maxima and the board heap), so this adds no row;
                            // it is here so the two producers stay one shape
                            // rather than two that happen to agree.
                            for (k, v) in written.cyclonedds_env() {
                                env.insert(k, v);
                            }
                            path_env.insert(
                                nros_sizing_descriptor::DESCRIPTOR_ENV.to_string(),
                                written.path,
                            );
                        }
                        Ok(None) => {}
                        // NOT fatal, and the same argument stage 3.5 makes:
                        // making the descriptor a new way for a build to stop
                        // would be a regression paid by every image for the
                        // benefit of the few that derive.
                        Err(e) => eprintln!(
                            "nros build: warning: `{image_id}`'s sizing descriptor was not \
                             written ({e}); every consumer keeps its own defaults"
                        ),
                    }
                } else if let Some(infra) = resolved.as_ref().and_then(|r| r.store_infra) {
                    // Issue 1706 -- no wiring, so no full descriptor; but a
                    // bringup that implies a parameter store still says so,
                    // and the descriptor is this road's only carrier. A model
                    // with no wiring declares no `params:` either (issue 1436's
                    // predicate), so the declarations are `Absent`.
                    let params = crate::entity_inventory::ParamDeclarations::Absent;
                    match crate::sizing_descriptor::write_for_model(&image_for(
                        crate::sizing_descriptor::Wiring::Unwired {
                            params: &params,
                            infra,
                            model_nodes: Some(infra.model_nodes),
                            roots: &[root.as_path()],
                        },
                    )) {
                        Ok(Some(written)) => {
                            eprintln!(
                                "nros build:   sizing descriptor (no wiring: the model's \
                                 facts only, issue 1706, phase-486 W9) → {}",
                                written.path.display()
                            );
                            path_env.insert(
                                nros_sizing_descriptor::DESCRIPTOR_ENV.to_string(),
                                written.path,
                            );
                        }
                        Ok(None) => {}
                        Err(e) => eprintln!(
                            "nros build: warning: `{image_id}`'s parameter-store descriptor was \
                             not written ({e}); the store keeps the heap path"
                        ),
                    }
                }
                // phase-445 W6 — what the image's declared TRANSPORT implies,
                // then the APP rung. Same two layers, same order and the same
                // reasons as the single-package road
                // (`cmd::leaf_settings::{image_block_layers, write}`): a
                // schema key that is read on one of two roads is a key that
                // silently stops being true on the other, which is the defect
                // `leaf_system`'s own header records for the retired manifest
                // tables. `[image] env` is LAST, so it outranks the
                // implication and the derived pools and is outranked only by
                // the calling environment (nothing here writes `force`).
                env.extend(crate::cmd::leaf_settings::image_block_layers(
                    Some(&image_id),
                    &image,
                )?);
                // Issue 1869 (RFC-0100 D13) — a caller that names ONE target
                // dir for several images (the fixture lanes) gets a build dir
                // per CONFIGURATION inside it, and the descriptor as a class
                // copy, so equal configurations share every unit and different
                // ones never rebuild or reuse each other's. The user's road
                // names no target dir and is untouched.
                let shared_class = {
                    let getenv = |k: &str| std::env::var(k).ok();
                    match crate::builder::shared_target::caller_target_dir(
                        &args.native_args,
                        &getenv,
                        &root,
                    ) {
                        Some(tdir) => {
                            let extra = [
                                (
                                    "cargo_config",
                                    descriptor.cargo_config.as_deref().unwrap_or(""),
                                ),
                                ("target", descriptor.target.as_deref().unwrap_or("")),
                            ];
                            let class = crate::builder::shared_target::apply(
                                &tdir,
                                &env,
                                &mut path_env,
                                &extra,
                            )
                            .map_err(|e| eyre::eyre!("`{image_id}`'s shared build class: {e}"))?;
                            eprintln!(
                                "nros build:   shared target dir → build class {} ({})",
                                class.key,
                                class.dir.display()
                            );
                            (!crate::builder::shared_target::caller_names_build_dir(
                                &args.native_args,
                                &getenv,
                            ))
                            .then(|| class.build_dir())
                        }
                        None => None,
                    }
                };
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
                        path_env,
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
                // A GENERATED entry has a GENERATED lock beside it, and
                // `--frozen` above forbids creating or moving one:
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
                // A hand-written entry's EXISTING lock is the user's and is
                // left alone; one with NO lock gets one created, because each
                // entry is its own cargo root now (RFC-0098 D9) and the lock
                // that used to cover it was the deleted workspace root's —
                // `--frozen` cannot build a root that has none.
                //
                // RFC-0104 D3 (phase-485 W2): a lock created here starts from
                // the registry pin set, so `update --workspace` keeps every
                // pinned version the manifests accept instead of resolving the
                // registry fresh.
                let had_lock = manifest_dir.join("Cargo.lock").is_file();
                crate::orchestration::registry_pins::seed(&manifest_dir, Some(nros_root))?;
                if lock_is_ours || !had_lock {
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
                    cargo_prepare = Some(Handoff::new("cargo", u).in_dir(&root));
                }
                if let Some(build_dir) = &shared_class {
                    // `build.build-dir` (cargo 1.91+): intermediates per class,
                    // final artifacts still uplifted into the caller's
                    // `--target-dir`. A TOML string, so the path is quoted.
                    a.push("--config".to_string());
                    a.push(format!(
                        "build.build-dir={}",
                        toml_edit::Value::from(build_dir.display().to_string())
                    ));
                }
                a.extend(args.native_args.iter().cloned());
                output = Some(crate::cmd::build_next::BuildOutput::Cargo {
                    // A caller-named target dir wins over the settings file's,
                    // as it does for cargo itself (CLI/env above config).
                    target_dir: crate::builder::shared_target::caller_target_dir(
                        &args.native_args,
                        &|k| std::env::var(k).ok(),
                        &root,
                    )
                    .unwrap_or_else(|| image_dir.join("target")),
                    triple: descriptor.target.as_deref().map(|t| {
                        crate::orchestration::board_descriptor::triple_of_build_target(t)
                            .to_string()
                    }),
                    profile_dir: crate::cmd::build_next::cargo_profile_dir(
                        image.profile.as_deref(),
                        &args.native_args,
                    ),
                    bin: crate::cmd::build_next::cargo_bin_name(&manifest_dir),
                });
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
                let manifest_dir =
                    root.join("build")
                        .join(cmake_coordinate(&platform, &image, &bringup_dir));
                record_dirs.push(manifest_dir.clone());
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
                let coord = cmake_coordinate(&platform, &image, &bringup_dir);
                // A C++ source ANYWHERE in a package, not just at its top —
                // `builder::discover::holds_cpp_source`, the one spelling the
                // west application's `LANG` also reads (phase-470 W5.b3). The
                // SCOPE differs and stays here: this root serves the whole
                // workspace, a west application only the node packages one
                // image's launch file names.
                let has_cpp = found
                    .packages
                    .iter()
                    .any(|p| crate::builder::discover::holds_cpp_source(&p.dir));
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
                                    ) == Ok(Driver::CMake)
                                        && cmake_coordinate(d.platform.kebab(), img, bd) == coord
                                })
                                .unwrap_or(false)
                    })
                    .filter_map(|(_, bd, id, img)| {
                        let name = crate::builder::entry::package_name(&id);
                        if root
                            .join("src")
                            .join(&name)
                            .join("CMakeLists.txt")
                            .is_file()
                        {
                            return None;
                        }
                        if img.before_boot.is_some() {
                            return Some(Err(before_boot_not_on_c_road(&id)));
                        }
                        let b = img.board.clone().unwrap_or_default();
                        Some(Ok(crate::builder::cmake_root::CmakeEntry {
                            image: id.clone(),
                            launch: img.launch.clone().unwrap_or_else(|| "default".to_string()),
                            args: img
                                .args
                                .iter()
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect(),
                            // The workspace's own language: the generated TU has
                            // to compile against what it links.
                            lang: if has_cpp {
                                nros_lang::Language::Cpp
                            } else {
                                nros_lang::Language::C
                            },
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
                            // Issue 1946 — the image's domain, by the resolver
                            // the cargo and west roads read
                            // (`leaf_system`: `[image]`, its defaults, then
                            // `[system]`). Until this the CMake road carried
                            // none, and a stated domain was dropped silently.
                            domain_id: match nros_orchestration_ir::leaf_system::read_image(
                                &bd, &id,
                            ) {
                                Ok(sys) => sys.and_then(|s| s.network.domain_id),
                                Err(e) => return Some(Err(eyre::eyre!("{e}"))),
                            },
                            name,
                        }))
                    })
                    .collect::<Result<Vec<_>>>()?;

                // phase-481 W4 — this configure's images resolve ONE configuration
                // (the coordinate is keyed on it), so its rows are this image's.
                // Handed to every cargo command the configure spawns by the
                // issue-1712 carrier (`NanoRosImageEnv.cmake`); removed when the
                // image states none.
                let image_env = crate::cmd::leaf_system::write_image_env(
                    &manifest_dir.join(crate::builder::cmake_root::IMAGE_ENV_FILE),
                    &crate::cmd::leaf_settings::image_block_layers(Some(&image_id), &image)?,
                )?
                .is_some();
                let spec = crate::builder::cmake_root::CmakeRootSpec {
                    entries: cmake_entries,
                    image_env,
                    workspace: root.clone(),
                    system: bringup.clone(),
                    platform: platform.clone(),
                    // phase-484 W4d — the board's OWN cmake module, not the
                    // alias the image happened to author: `board = "freertos"`
                    // names the mps2-an385 FreeRTOS board, whose module is
                    // `mps2-an385-freertos`. A descriptor that states none keeps
                    // the authored name (and cmake refuses a missing module).
                    board: descriptor
                        .cmake_module
                        .clone()
                        .or_else(|| image.board.clone()),
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
                // Issue 1827 — the bringup this image is built from. The
                // configure's board-facts lookup asked the WORKSPACE, and a
                // workspace with two bringups (`realtime-c`) is an ambiguity
                // the verb refuses, so FreeRTOS images there got no
                // `FREERTOS_PORT` and `zpico-sys` panicked.
                a.push(format!("-DNROS_BRINGUP_DIR={}", bringup_dir.display()));
                // RFC-0103 D10f (phase-497 W2) — the image's resolved network.
                // `nros_board_net_c` renders the C initializer from it.
                if let Some(n) = &image_net {
                    crate::orchestration::image_net::write(&root.join("build"), &image_id, n)
                        .map_err(|e| eyre::eyre!("{e}"))?;
                    a.push(format!(
                        "-D{}={}",
                        nros_board_common::image_net::ENV,
                        n.text()
                    ));
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
                // Issue 1653 (RFC-0100 Amendment 1, "D4 revised") — the BOARD
                // heap, resolved here at stage 4 for every road, handed to the
                // configure the way the triple is. The configure does not read
                // `nros-board.toml` and must not learn to (a second reader of
                // the board's knobs, the shape RFC-0064 R5 D4 removed);
                // `nano_ros_entry()` passes it to the descriptor producer as
                // `--heap-budget-bytes`, so `[target] heap_budget_bytes` is
                // stated and Cyclone's D11 boot check is armed on this road.
                // Absent when the board states no `[knobs.memory]
                // heap_bytes`, which keeps the field REFUSED -- "nobody said".
                a.push(board_heap_configure_arg(
                    descriptor,
                    &board,
                    nano_ros_root.as_deref(),
                ));
                // phase-462 W6c -- the board's `[board.rx]`, on the same terms.
                a.extend(board_rx_configure_args(descriptor));
                // Issue 1304 — the ROOT this generated file's
                // `find_package(nano_ros)` resolves against. `nano_rosConfig.cmake`
                // sits at the SDK root and is located through `nano_ros_ROOT`,
                // which only `activate.sh` exports — so on a host with no
                // sourced shell the configure died with "Could not find a package
                // configuration file provided by nano_ros", after `nros build`
                // had already resolved that very root.
                //
                // PASSED, not baked into the generated file, for the reason the
                // preamble and `NROS_RESOLVED_DIR` above are: the file stays
                // workspace-agnostic and byte-identical across machines (W3.c),
                // which an absolute store path written into it would not be.
                //
                // Before `native_args` so a caller's own `-Dnano_ros_ROOT` still
                // wins: cmake takes the last definition on the line.
                if let Some(nr) = &nano_ros_root {
                    a.push(crate::builder::nano_ros_root_arg(nr));
                }
                a.extend(board_build_type_arg(&platform));
                a.extend(args.native_args.iter().cloned());
                cmake_configure = Some(Handoff::new("cmake", a).in_dir(&root));
                // A generated entry is a top-level target of the generated
                // root; a hand-written `src/<entry>` package is a subdirectory
                // (the fixture locator's two shapes).
                output = Some(crate::cmd::build_next::BuildOutput::CMake {
                    build_dir: manifest_dir.join("cmake"),
                    executable: Some(
                        if root
                            .join("src")
                            .join(&want_entry)
                            .join("CMakeLists.txt")
                            .is_file()
                        {
                            format!("src/{want_entry}/{want_entry}")
                        } else {
                            want_entry.clone()
                        },
                    ),
                });
                Some(
                    Handoff::new(
                        "cmake",
                        vec!["--build".to_string(), format!("{rel_src}/cmake")],
                    )
                    .in_dir(&root),
                )
            }
            Driver::West => {
                // RFC-0109 D8 (phase-514 W3a) — a C/C++ west application bakes
                // the bringup's lowered RMW file through `nano_ros_entry()`. A
                // Rust one gets it through the resolve directory (issue 2051):
                // the table it lowers to, which the module names to the Rust
                // lane's cargo build and `zephyr_component_main!` installs.
                // Without a resolve directory there is no carrier,
                // and baking nothing would boot the image on the board's
                // default router, so that one case still refuses.
                if !image_has_non_rust(&image, &bringup_dir) {
                    let text =
                        crate::orchestration::rmw_config::session_text(&bringup_dir, &image_id)
                            .map_err(|e| eyre::eyre!("{e}"))?;
                    match &resolved_dir {
                        Some(d) => crate::orchestration::rmw_config::write_west_session_file(
                            d,
                            text.as_deref(),
                        )
                        .map_err(|e| eyre::eyre!("{e}"))?,
                        None if text.is_some() => eyre::bail!(
                            "`{qual}`: this build ran no resolve phase, which is what carries \
                             the bringup's lowered RMW file to a Rust Zephyr application's \
                             cargo build (issue 2051). Build it with `nros build`, or state \
                             the endpoint as the image's `locator`"
                        ),
                        None => {}
                    }
                }
                // Issue 1419 — a RUST west application (`rust_cargo_application()`)
                // calls no `nano_ros_entry()`, so the configure-time census check
                // a C/C++ Zephyr entry runs never reaches it. Stage 4 is that
                // road's one point that holds the model and the image before any
                // compile -- the cargo road's reason, one driver over. A C/C++
                // west application keeps its configure check.
                if !args.dry_run
                    && stage4_checks_census(
                        Driver::West,
                        descriptor.platform,
                        image_has_non_rust(&image, &bringup_dir),
                    )
                {
                    crate::cmd::entity_census::check_cargo_image(
                        &root,
                        &bringup_dir,
                        &image_id,
                        &image,
                    )
                    .wrap_err_with(|| {
                        format!(
                            "`{qual}`: the census and the contract disagree, or the census no \
                             longer describes the code (issue 1419)"
                        )
                    })?;
                }
                // W5 — overlays reach Zephyr through EXTRA_CONF_FILE and
                // APPLICATION_CONFIG_DIR. Never CONF_FILE: that suppresses
                // Zephyr's own boards/ and socs/ discovery entirely.
                // The application is resolved FIRST: its directory is where a
                // Zephyr app keeps its own `prj-*.conf`, so the overlay search
                // needs it (issue 0892).
                //
                // phase-470 W5.a (issue 1288) — GENERATE it where we can, and
                // fall back to LOCATING a hand-written one where we cannot.
                // Which of the two is not a heuristic:
                //
                // * `[image.<id>] entry = "<pkg>"` names a package, and a name
                //   is the answer to the ambiguity `west_application_dir`
                //   exists to resolve. It must keep working until W5.b retires
                //   the last hand-written application, and it is checked FIRST
                //   because `entry` may name a package whose name is NOT
                //   `<id>_entry` — `[image.zephyr_robot1] entry =
                //   "zephyr_entry_robot1"` is exactly that, so generation would
                //   otherwise kick in beside a package it never looked at.
                // * otherwise `generate_entry` writes one, unless a
                //   hand-written `src/<id>_entry` suppresses it (D13's
                //   migration is a DELETION — that check lives there).
                //
                // The generated application is found BY CONSTRUCTION. It cannot
                // be found by the `DEPLOY`-token scan below: a Zephyr entry's
                // `DEPLOY` names the PLATFORM, so that scan answers a question
                // about the board it never reads (issue 1517).
                //
                // * a STANDALONE leaf whose own `CMakeLists.txt` is a Zephyr
                //   application (`examples/zephyr/<lang>/<example>/`) IS the
                //   application. Generating one beside it pointed west at a
                //   directory with no `prj.conf` (C/C++: "No prj.conf file(s)
                //   was found") or demanded a selection facade for an entry
                //   nobody needs (Rust).
                let leaf_app = (image.entry.is_none())
                    .then(|| zephyr_leaf_application(&root, &bringup_dir))
                    .flatten();
                let generated = if image.entry.is_some() || leaf_app.is_some() {
                    GeneratedEntry::default()
                } else {
                    generate_entry(
                        &root,
                        &bringup_dir,
                        &bringup,
                        &image_id,
                        &image,
                        descriptor,
                        &platform,
                        nano_ros_root.as_deref(),
                        image_has_non_rust(&image, &bringup_dir),
                    )?
                };
                let app = match (&generated.dir, leaf_app.as_ref()) {
                    (Some(d), _) | (None, Some(d)) => {
                        eprintln!("nros build:   west application → {}", d.display());
                        d.clone()
                    }
                    (None, None) => west_application_dir(
                        &image_id,
                        &image,
                        descriptor,
                        &found,
                        &catalog,
                        &bringup_dirs,
                    )?
                    .unwrap_or_else(|| bringup_dir.clone()),
                };
                // The board id WEST knows, not the one the image authored —
                // `<bringup>/boards/<board>/` is the directory Zephyr's own
                // `boards/<board>.conf` discovery runs under, and Zephyr knows
                // the west spelling. `[image.zephyr] board = "zephyr"` and
                // `board = "native_sim/native/64"` are the same board, so they
                // must reach the same directory.
                let west_board = descriptor.west_build_board(&board);
                let mut overlays = crate::builder::zephyr::resolve_in(
                    &bringup_dir,
                    Some(&app),
                    &west_board,
                    &image,
                )
                .map_err(|e| eyre::eyre!("{e}"))?;
                // Issue 1288 — the board package's own Zephyr files (base
                // `prj.conf`, `boards/<flat>.{conf,overlay}`) reach a
                // GENERATED application here, by the rule
                // `nano_ros_use_board()` applies to a hand-written one
                // (`board_projection::board_files`, one derivation for both).
                // Same position too: ahead of the image's own fragments, so a
                // `conf` fragment overrides the board; the bringup's
                // `prj.conf` is Zephyr's CONF_FILE and merges BEFORE every
                // extra, so a value that must beat the board is listed in
                // `conf`. A hand-written application owns its preamble and
                // gets nothing added — it would get the files twice.
                if generated.dir.is_some() {
                    if let Some(dir) = nano_ros_root.as_deref().and_then(|r| {
                        crate::orchestration::board_projection::descriptor_dir(descriptor, r)
                    }) {
                        let files =
                            crate::orchestration::board_projection::board_files(&west_board, &dir);
                        let mut confs = files.conf_files();
                        confs.append(&mut overlays.extra_conf_file);
                        overlays.extra_conf_file = confs;
                        if let Some(o) = files.board_overlay {
                            overlays.extra_dtc_overlay_file.insert(0, o);
                        }
                    }
                }
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

                // `west_board` is resolved above, beside the overlay directory
                // it also names — one rule, on the descriptor
                // (`BoardDescriptor::west_build_board`, issue 1517). It used to
                // be spelled here and read only the OUTER `west_board`, which
                // no in-tree descriptor declares, so `nros build fvp` emitted
                // `-b fvp-aemv8r-smp` for a descriptor that states
                // `fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`.
                let mut a = vec!["build".to_string(), "-b".to_string(), west_board];
                if overlays.sysbuild {
                    a.push("--sysbuild".to_string());
                }
                // Issue 1669 -- each image configures into its OWN build dir,
                // `build/<coordinate>/<image>_west`, unless the caller names
                // one (`-- -d <dir>`, which the fixture lanes do: theirs wins).
                // West's default is `build/` under the cwd, which is the
                // workspace root here -- nros's own `build/` -- so the first
                // Zephyr image's tree landed beside the coordinate directories
                // and a second image of the same workspace was refused ("is
                // for application …, but source directory … was specified").
                // In the FIRST zone and before the positional: `-d` takes one
                // value, so it cannot swallow the application path.
                output = Some(crate::cmd::build_next::BuildOutput::West {
                    build_dir: if names_west_build_dir(&west_extra) {
                        crate::cmd::build_next::west_dir_from_args(&west_extra, &root)
                    } else {
                        Some(west_build_dir(
                            &root, &platform, &image, &bringup, &image_id,
                        ))
                    },
                });
                if !names_west_build_dir(&west_extra) {
                    record_dirs.push(west_build_dir(
                        &root, &platform, &image, &bringup, &image_id,
                    ));
                    a.push("-d".to_string());
                    a.push(
                        west_build_dir(&root, &platform, &image, &bringup, &image_id)
                            .display()
                            .to_string(),
                    );
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
                let mut resolved_opt: Vec<String> = resolved_dir
                    .as_ref()
                    .map(|d| vec![format!("-DNROS_RESOLVED_DIR={}", d.display())])
                    .unwrap_or_default();
                // Issue 1653's west half -- the board heap, on the same
                // configure line the cmake arm hands it on. West emits no root
                // (RFC-0065 D3), but its second argument zone IS a cmake
                // command line, and the entry's descriptor producer reads the
                // cache variable the same way on both roads. A bare `west
                // build` passes nothing and the field stays refused, exactly as
                // a bare `cmake` does.
                resolved_opt.push(board_heap_configure_arg(
                    descriptor,
                    &board,
                    nano_ros_root.as_deref(),
                ));
                resolved_opt.extend(board_rx_configure_args(descriptor));
                // Issue 1867 -- the image's `nros` module is THIS checkout. The
                // store workspace's manifest project carries none (issue 1258),
                // so without this every image built against it aborted at
                // Kconfig on `undefined symbol NROS`.
                if let Some(nr) = nano_ros_root.as_deref() {
                    resolved_opt.push(crate::builder::zephyr::module_arg(nr));
                    // Issue 1998 -- and the root the generated app's
                    // `find_package(nano_ros)` imports. The cmake roads pass it
                    // (issue 1304); without it a west configure died with
                    // "Could not find a package configuration file provided by
                    // nano_ros". Before `cmake_extra`, so a caller's own
                    // `-Dnano_ros_ROOT` still wins.
                    resolved_opt.push(crate::builder::nano_ros_root_arg(nr));
                }
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
                         $NROS_ZEPHYR_WORKSPACE, \
                         $NROS_HOME/workspaces/zephyr/<line>, \
                         <workspace>/zephyr-workspace, \
                         ../nano-ros-workspace[-4.4]; <line> is \
                         $NROS_ZEPHYR_VERSION, default 3.7)"
                    );
                }
                // Run from the nros workspace with ZEPHYR_BASE set, which is
                // exactly what `west-fixtures.sh` does and what makes a
                // freestanding application work. No cwd gymnastics: the
                // application path is absolute.
                let h = Handoff::new("west", a).in_dir(&root);
                let h = match &zbase {
                    Some(z) => h.with_env("ZEPHYR_BASE", z.as_os_str()),
                    None => h,
                };
                // phase-439 W7 -- the configure may READ the resolve's entity
                // inventory only when THIS run of `nros build` wrote the record
                // it reads it by: the digest rides the environment, which ninja's
                // own re-runs of cmake inherit and a later reconfigure of the
                // same build dir does not (`NROS_RESOLVED_DIR` is cached, so it
                // cannot tell them apart). The record covers the registered
                // rows, the model and the workspace, not the probe sidecars that
                // floor the counts, and those can move between two builds.
                Some(
                    match resolved_dir
                        .as_ref()
                        .and_then(|d| crate::resolve::entity_record_digest(d))
                    {
                        Some(digest) => h.with_env(crate::resolve::ENTITY_RECORD_ENV, digest),
                        None => h,
                    },
                )
            }
        };

        out.push(ResolvedBuild {
            configure: cmake_configure.or(cargo_prepare),
            cmake_build_dir: cmake_build_dir_for(driver, &platform, &image, &bringup_dir),
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
            record: crate::orchestration::project_lock::BuildRecord {
                project: root.clone(),
                cargo_roots,
                build_dirs: record_dirs,
            },
            capabilities: owned_caps(&bringup_dir),
            output,
            descriptor: Some(descriptor.clone()),
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
fn plan_single_package(
    args: &Args,
    root: &std::path::Path,
    planning: Planning,
) -> Result<Option<Vec<ResolvedBuild>>> {
    // issue 1641 — the one ladder, not a hand-spelled one. This was
    // `explicit > env > walk-up`, the order 1510 fixed in `nano_ros_root` and
    // left standing here; RFC-0103 D6 (amended) has since removed the env rung.
    let Some(nros_root) =
        crate::orchestration::nano_ros_root::resolve(args.nano_ros_path.clone(), root)
    else {
        // The workspace road reports a missing checkout in its own words.
        return Ok(None);
    };
    // ---- stage 2 — the image: the one named, else the leaf's own rule -----
    //
    // Issue 1305 — a leaf may declare several images (one per RMW and ROS
    // edition). They share ONE generated selection package and ONE
    // `generated/` tree, so one image is built per invocation: two in one plan
    // would each rewrite what the other compiles against.
    let Some(pkg) = crate::cmd::leaf_settings::leaf_package(root) else {
        return Ok(None);
    };
    let named: Vec<&str> = args
        .images
        .iter()
        .map(|w| match w.split_once(':') {
            Some((p, id)) if p == pkg => id,
            _ => w.as_str(),
        })
        .collect();
    let image = match named.as_slice() {
        [] => None,
        [one] => Some(*one),
        many => eyre::bail!(
            "{} is a single-package leaf: build one image per `nros build` (asked for {}). \
             Its images share one generated selection package and one `generated/` tree \
             (issue 1305), so build them one after another.",
            root.display(),
            many.join(", ")
        ),
    };
    let Some(img) = crate::cmd::leaf_settings::resolve_image(root, &nros_root, image)? else {
        return Ok(None);
    };
    plan_leaf_image(args, root, &nros_root, planning, img)
}

/// Stages 3–4 for one resolved image of a single-package leaf.
fn plan_leaf_image(
    args: &Args,
    root: &std::path::Path,
    nros_root: &std::path::Path,
    planning: Planning,
    img: crate::cmd::leaf_settings::LeafImage,
) -> Result<Option<Vec<ResolvedBuild>>> {
    let nros_root = nros_root.to_path_buf();
    let qual = plan::qualified(&img.package, &img.image_id);
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
        eyre::bail!("{}", crate::cmd::board_facts::unresolved_board(&img.board));
    };
    // RFC-0103 D10 (phase-497 W2) — refused here, at preflight, with the
    // image named; `leaf_settings` resolves the same answer into the build.
    leaf_image_net(descriptor, &img.board, Some(&nros_root), &img.decl)
        .map_err(|e| eyre::eyre!("`{qual}` cannot be built: {e}"))?;
    crate::cmd::setup::expose_store_tools(descriptor, Some(&nros_root));
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
    // phase-460 W1 follow-up -- a model sync refused (or one whose inputs
    // moved since) is not "unsynced": it names its own remedy.
    if let Err(r) = crate::leaf_entity_env::leaf_model_path(root) {
        eyre::bail!("{}: {r}", root.display());
    }
    let unsynced: Vec<String> = unsynced_inputs(root, &img);
    if !unsynced.is_empty() {
        eyre::bail!(
            "{} has not been synced — missing {}.\n  Run `nros sync` in {} (RFC-0098 D2), then build again.",
            root.display(),
            unsynced.join(", "),
            root.display()
        );
    }

    // Issue 1305 — `generated/` holds ONE edition's type hashes, and the image
    // must be built against its own: a jazzy image over humble message crates
    // would advertise the RIHS01 keyexpr tail with a `TypeHashNotSupported`
    // hash, a mismatch no peer reports. Sync records the edition it generated
    // for; a tree from before that record is not second-guessed.
    let want = crate::cmd::leaf_settings::image_ros_edition(&img)?;
    if let Some(have) = crate::cmd::ws::generated_edition(&img.leaf.join("generated"))
        && have != want
    {
        eyre::bail!(
            "{}: image `{}` builds for ROS {}, and `generated/` was synced for {}.\n  \
             Run `nros sync --image {}` in {} first (issue 1305).",
            root.display(),
            img.image_id,
            want.as_str(),
            have.as_str(),
            img.image_id,
            root.display()
        );
    }

    // Issue 1716 — a query names the leaf's image and writes no settings file.
    if planning == Planning::Query {
        return Ok(Some(vec![ResolvedBuild {
            qualified: qual,
            board: img.board.clone(),
            platform: img.platform.clone(),
            driver: Driver::Cargo,
            handoff: None,
            rmw: img.decl.rmw.clone(),
            entry_package: Some(img.package.clone()),
            target: img.target.clone(),
            profile: None,
            cmake_build_dir: None,
            capabilities: owned_caps(root),
            configure: None,
            record: Default::default(),
            output: None,
            descriptor: None,
        }]));
    }

    // ---- stage 4 — the one settings file ------------------------------------
    let Some((img, shared_class)) = crate::cmd::leaf_settings::write_shared(
        root,
        &nros_root,
        "nros build",
        Some(img.image_id.as_str()),
        &args.native_args,
    )?
    else {
        eyre::bail!(
            "{}: resolved as a single-package leaf, then not",
            root.display()
        );
    };
    eprintln!("nros build:   settings → {}", img.config_path.display());

    let (cwd, mut a) = crate::cmd::leaf_settings::build_command(&img);
    // RFC-0104 D3 (phase-485 W2): a leaf with no lock starts from the registry
    // pin set, and is resolved ONCE before the build — the same prepare step a
    // generated entry takes above, because `--frozen` (and the PATH shim's
    // `--locked`) cannot prune a seed down to the leaf's graph.
    let had_lock = img.leaf.join("Cargo.lock").is_file();
    let seeded = matches!(
        crate::orchestration::registry_pins::seed(&img.leaf, Some(&nros_root))?,
        crate::orchestration::registry_pins::Seed::Seeded { .. }
    );
    let prepare = (!had_lock && seeded).then(|| {
        // `a` is `build --manifest-path <m> --config <c>`: the same root and
        // the same settings, resolved instead of built.
        let mut u = vec!["update".to_string(), "--workspace".to_string()];
        u.extend(a[1..].iter().cloned());
        if args.offline {
            u.push("--offline".to_string());
        }
        Handoff::new("cargo", u).in_dir(&cwd)
    });
    if args.offline {
        a.push("--frozen".to_string());
    }
    if let Some(class) = &shared_class {
        eprintln!(
            "nros build:   shared target dir → build class {} ({})",
            class.key,
            class.dir.display()
        );
        let getenv = |k: &str| std::env::var(k).ok();
        if !crate::builder::shared_target::caller_names_build_dir(&args.native_args, &getenv) {
            a.push("--config".to_string());
            a.push(format!(
                "build.build-dir={}",
                toml_edit::Value::from(class.build_dir().display().to_string())
            ));
        }
    }
    a.extend(args.native_args.iter().cloned());
    let output = crate::cmd::build_next::BuildOutput::Cargo {
        target_dir: crate::builder::shared_target::caller_target_dir(
            &args.native_args,
            &|k| std::env::var(k).ok(),
            &cwd,
        )
        .unwrap_or_else(|| crate::cmd::leaf_settings::target_dir(&img.leaf, &img.image_id)),
        triple: img
            .target
            .as_deref()
            .map(|t| crate::orchestration::board_descriptor::triple_of_build_target(t).to_string()),
        profile_dir: crate::cmd::build_next::cargo_profile_dir(None, &args.native_args),
        bin: crate::cmd::build_next::cargo_bin_name(&img.leaf),
    };
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
        cmake_build_dir: None,
        configure: prepare,
        record: crate::orchestration::project_lock::BuildRecord {
            project: root.to_path_buf(),
            cargo_roots: vec![img.leaf.clone()],
            build_dirs: img
                .config_path
                .parent()
                .map(|d| vec![d.to_path_buf()])
                .unwrap_or_default(),
        },
        capabilities: owned_caps(root),
        output: Some(output),
        descriptor: Some(descriptor.clone()),
    }]))
}

/// A single-package C/C++ example (issues 1296, 1308): a `CMakeLists.txt` and a
/// `system.toml` side by side, no `Cargo.toml`, on a board whose road is cmake.
///
/// The build is the leaf's OWN `CMakeLists.txt` — the `cmake -B … && cmake
/// --build …` pair the book documents and the lanes run — configured into
/// `build/<image>/cmake` with `-DNROS_IMAGE=<id>`, which
/// `nano_ros_read_leaf_system` honours. RFC-0098 leaves a C/C++ leaf's CMake
/// side as it is ("What is NOT decided here"): the leaf reads its own
/// `system.toml` through `nros ws leaf-system`, so this road adds nothing to it
/// but the inputs a bare configure cannot know — the board's toolchain file,
/// its heap, and the nano-ros root.
///
/// It used to fall through to the WORKSPACE road, which generated a root for a
/// workspace of one: a synthesised typed `<id>_entry` whose BRINGUP named
/// `src/<[system] name>` (a directory no leaf has) and a metadata step asking
/// `nros plan` for `<[system] name>/launch/system.launch.xml`, which nothing
/// writes. Both are workspace facts; the leaf already declares its own
/// executable. `None` for anything else, so a west or idf leaf keeps the road
/// that handles (or refuses) it.
fn plan_single_package_cmake(
    args: &Args,
    root: &std::path::Path,
) -> Result<Option<Vec<ResolvedBuild>>> {
    use nros_orchestration_ir::leaf_system;
    if !root.join("CMakeLists.txt").is_file()
        || root.join("Cargo.toml").is_file()
        || !root.join(leaf_system::SYSTEM_TOML).is_file()
    {
        return Ok(None);
    }
    let Some(nros_root) =
        crate::orchestration::nano_ros_root::resolve(args.nano_ros_path.clone(), root)
    else {
        return Ok(None);
    };
    let package = std::fs::read_to_string(root.join("package.xml"))
        .ok()
        .and_then(|x| crate::orchestration::prereq_resolve::package_name(&x))
        .or_else(|| root.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "leaf".to_string());
    // `--image <id>` or `<pkg>:<id>`; none ⇒ the leaf's own rule (one image,
    // or `[system] default_images`).
    let want = match args.images.as_slice() {
        [] => None,
        [one] => Some(
            one.strip_prefix(&format!("{package}:"))
                .unwrap_or(one)
                .to_string(),
        ),
        many => eyre::bail!(
            "{} is a single-package example: name one image, not {}",
            root.display(),
            many.len()
        ),
    };
    let decl = match &want {
        Some(id) => leaf_system::read_image(root, id),
        None => leaf_system::read(root),
    }
    .map_err(|e| eyre::eyre!(e))?;
    let Some(decl) = decl else {
        if let Some(id) = want {
            eyre::bail!("`{id}` is not an image of {} (RFC-0098 D3)", root.display());
        }
        return Ok(None);
    };
    let Some(board) = decl.board.clone() else {
        eyre::bail!(
            "{}: names no board (RFC-0098 D3: `[image.<id>] board = \"<board>\"`)",
            decl.origin_path().display()
        );
    };
    let catalog = crate::orchestration::board_descriptor::BoardCatalog::load(&nros_root)
        .map_err(|e| eyre::eyre!("board catalog under {}: {e}", nros_root.display()))?;
    let Some(descriptor) = crate::cmd::board_facts::resolve_board(&catalog, &board) else {
        eyre::bail!("{}", crate::cmd::board_facts::unresolved_board(&board));
    };
    leaf_image_net(descriptor, &board, Some(&nros_root), &decl)
        .map_err(|e| eyre::eyre!("`{board}` cannot be built: {e}"))?;
    let platform = descriptor.platform.kebab().to_string();
    if plan::driver_for_board(&platform, descriptor.entry_kind, true) != Ok(Driver::CMake) {
        return Ok(None);
    }
    for p in args.packages_select.iter().chain(&args.packages_up_to) {
        if p != &package {
            eyre::bail!(
                "`{p}` is not a package here: {} is a single-package example (`{package}`).",
                root.display()
            );
        }
    }
    let image_id = decl.image.clone().unwrap_or_else(|| board.clone());
    let qual = plan::qualified(&package, &image_id);

    // The toolchain half of preflight, as on the cargo leaf road; its sync
    // probe is a workspace heuristic, and this configure generates its own
    // message bindings.
    crate::cmd::setup::expose_store_tools(descriptor, Some(&nros_root));
    let mut missing = crate::builder::preflight::check(descriptor, root, Some(&nros_root));
    missing.retain(|m| m.remedy != "nros sync");
    if !missing.is_empty() {
        eyre::bail!("{}", crate::builder::preflight::report(&missing));
    }

    let build_dir = format!("build/{image_id}/cmake");
    let output_dir = root.join(&build_dir);
    let mut a = vec![
        "-S".to_string(),
        ".".to_string(),
        "-B".to_string(),
        build_dir.clone(),
        format!("-DNROS_IMAGE={image_id}"),
    ];
    if let Some(c) = &descriptor.cmake {
        a.push(format!(
            "-DCMAKE_TOOLCHAIN_FILE={}",
            nros_root.join(&c.toolchain_file).display()
        ));
    }
    a.push(board_heap_configure_arg(
        descriptor,
        &board,
        Some(&nros_root),
    ));
    a.extend(board_rx_configure_args(descriptor));
    a.push(crate::builder::nano_ros_root_arg(&nros_root));
    a.extend(board_build_type_arg(&platform));
    a.extend(args.native_args.iter().cloned());
    Ok(Some(vec![ResolvedBuild {
        qualified: qual,
        board,
        platform,
        driver: Driver::CMake,
        handoff: Some(
            Handoff::new("cmake", vec!["--build".to_string(), build_dir.clone()]).in_dir(root),
        ),
        rmw: decl.rmw.clone(),
        entry_package: Some(package),
        target: descriptor.target.clone(),
        profile: None,
        cmake_build_dir: Some(build_dir),
        capabilities: owned_caps(root),
        configure: Some(Handoff::new("cmake", a).in_dir(root)),
        record: crate::orchestration::project_lock::BuildRecord {
            project: root.to_path_buf(),
            cargo_roots: Vec::new(),
            build_dirs: vec![root.join("build").join(&image_id)],
        },
        output: Some(crate::cmd::build_next::BuildOutput::CMake {
            build_dir: output_dir,
            executable: crate::cmd::build_next::cmake_leaf_executable(root),
        }),
        descriptor: Some(descriptor.clone()),
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
    if !img.decl.components.is_empty() && crate::leaf_entity_env::leaf_model(root).is_none() {
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

    // RFC-0104 D5 / issue 1780 — the generated message crates are the LOCAL
    // half, a function of the host's ament install. Before planning, because
    // planning reads them (the single-package leaf's manifest path-depends on
    // `generated/<pkg>`, and every cargo image compiles them): a crate whose
    // recorded ament inputs moved is regenerated HERE, never compiled stale.
    // The cmake and west roads ask the same question of their own generated
    // trees at configure (`nros codegen --ament-witness`).
    {
        let root = match &args.workspace {
            Some(w) => w.clone(),
            None => std::env::current_dir().wrap_err("resolving cwd as the workspace root")?,
        };
        let root = std::fs::canonicalize(&root).unwrap_or(root);
        let regenerated = crate::cmd::ws::refresh_generated(&root, args.dry_run)?;
        if !args.dry_run {
            observe_system(&args, &regenerated);
        }
        // phase-517 W8 — the managed message rows, the same step sync runs:
        // a `<depend>` is one edit, whichever of the two verbs comes next.
        for m in crate::cmd::ws::refresh_managed_msg_deps(&root, args.dry_run)? {
            let verb = if args.dry_run { "would write" } else { "wrote" };
            eprintln!("nros build: {verb} message dependencies → {}", m.display());
        }
    }

    let mut plans = plan_builds(&args)?;
    if args.verbose {
        for p in &mut plans {
            for h in p.configure.iter_mut().chain(p.handoff.iter_mut()) {
                h.env.push(("NROS_VERBOSE".into(), "1".into()));
            }
        }
    }
    if let Some(tool) = args.check {
        retarget_to_check(&mut plans, tool)?;
    }
    pin_this_project(&args)?;
    if !args.dry_run {
        let drifts = drift_before_build(&plans, args.nano_ros_path.clone())?;
        if !drifts.is_empty() {
            eyre::bail!("{}", crate::orchestration::drift::report(&drifts));
        }
    }
    match drive(&plans, args.dry_run, &mut perform) {
        Ok(()) => {
            if !args.dry_run {
                let drifts = record_project_lock(&plans, args.nano_ros_path.clone())?;
                if !drifts.is_empty() {
                    eyre::bail!(
                        "the build finished, but it resolved off the pin set.\n{}",
                        crate::orchestration::drift::report(&drifts)
                    );
                }
                if args.check.is_none() {
                    print_next_steps(&args, &plans);
                }
            }
            Ok(())
        }
        // The native build already said why on its own stderr; exit with ITS
        // status, as `exec` did (RFC-0065 D1, amended by phase-485 W3).
        Err(e) => match e.downcast_ref::<ChildExit>() {
            Some(c) => {
                eprintln!("nros build: {e:#}");
                std::process::exit(c.code)
            }
            None => Err(e),
        },
    }
}

/// RFC-0104 D9 (phase-485 W4) — what can be checked BEFORE the build: each
/// cargo root that already has a lock (realigned onto the pins in a checkout,
/// refused where that cannot be done), and each tool the project lock records.
/// phase-517 W6 — after a successful build, what to type next for each image
/// ([`crate::cmd::build_next::next_steps`]). Runs nothing. A plan with no
/// board descriptor (package mode) has no platform follow-up and prints none.
fn print_next_steps(args: &Args, plans: &[ResolvedBuild]) {
    let root = args
        .workspace
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default();
    let nros_root = crate::orchestration::nano_ros_root::resolve(args.nano_ros_path.clone(), &root);
    let ctx = crate::orchestration::qemu_bin::Ctx::for_sdk(nros_root.as_deref());
    let qemu = |exe: &str| crate::orchestration::qemu_bin::locate(exe, &ctx);
    for p in plans {
        let Some(d) = &p.descriptor else {
            continue;
        };
        let steps = crate::cmd::build_next::next_steps(p, d, &qemu);
        print!("{}", crate::cmd::build_next::render(&p.qualified, &steps));
    }
}

fn drift_before_build(
    plans: &[ResolvedBuild],
    nano_ros_path: Option<std::path::PathBuf>,
) -> Result<Vec<crate::orchestration::drift::Drift>> {
    let mut out = Vec::new();
    for p in plans {
        if p.record.project.as_os_str().is_empty() {
            continue;
        }
        let Some(nr) =
            crate::orchestration::nano_ros_root::resolve(nano_ros_path.clone(), &p.record.project)
        else {
            continue;
        };
        out.extend(cargo_drift(&p.record, &nr)?);
        // Tools only OUTSIDE a checkout. Inside one, every tool path is
        // CONSTRUCTED from the index pin (`<store>/<tool>/<pin>`, issue 1546),
        // so the build cannot use another version, and the lock's `[tool]` rows
        // are a gitignored history of past `nros setup` runs — measured stale on
        // this host for three tools a native build never touches. Where the
        // lock is the project's own record (RFC-0104 D4), a row the index
        // no longer names IS drift.
        if crate::orchestration::nano_ros_root::is_checkout(&nr) {
            continue;
        }
        let index_path = nr.join("nros-sdk-index.toml");
        if let Ok(index) = crate::orchestration::sdk_index::SdkIndex::load(&index_path) {
            let lock = crate::orchestration::sdk_store::SdkLock::load(
                &crate::orchestration::store::lock_path_for(&p.record.project),
            )?;
            out.extend(crate::orchestration::drift::tool_drift(
                &lock,
                &index,
                &crate::orchestration::sdk_index::host_key(),
            ));
        }
    }
    out.dedup();
    Ok(out)
}

/// The cargo kind for every root of one plan.
fn cargo_drift(
    rec: &crate::orchestration::project_lock::BuildRecord,
    nr: &std::path::Path,
) -> Result<Vec<crate::orchestration::drift::Drift>> {
    let Some(pins) = crate::orchestration::registry_pins::pin_file(nr)? else {
        return Ok(Vec::new());
    };
    let in_checkout = crate::orchestration::nano_ros_root::is_checkout(nr);
    let mut out = Vec::new();
    for root in &rec.cargo_roots {
        let label = root
            .strip_prefix(&rec.project)
            .ok()
            .map(|r| r.display().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| root.display().to_string());
        out.extend(crate::orchestration::drift::enforce_cargo_root(
            root,
            &label,
            &pins,
            in_checkout,
        )?);
    }
    Ok(out)
}

/// RFC-0104 D4 (phase-485 W3) — fold every finished plan into its project's
/// `nros-sdk.lock`: the sources it read, the West modules it used and each
/// cargo root's registry set. Returns what the build resolved off the pin set
/// (RFC-0104 D9, phase-485 W4): a lock the build created or moved, and a source
/// read at another commit than its pin.
///
/// After the build, never before: what a build READ is only on disk once it has
/// run. A failure to WRITE the lock is a warning; drift is not.
fn record_project_lock(
    plans: &[ResolvedBuild],
    nano_ros_path: Option<std::path::PathBuf>,
) -> Result<Vec<crate::orchestration::drift::Drift>> {
    let mut drifts = Vec::new();
    for p in plans {
        if p.record.project.as_os_str().is_empty() {
            continue;
        }
        let nr =
            crate::orchestration::nano_ros_root::resolve(nano_ros_path.clone(), &p.record.project);
        if let Some(nr) = &nr {
            drifts.extend(cargo_drift(&p.record, nr)?);
        }
        match crate::orchestration::project_lock::record(&p.record, nr.as_deref()) {
            Ok(r) => {
                if let Some(path) = r.written {
                    eprintln!("nros build:   recorded → {}", path.display());
                }
                drifts.extend(r.drifts);
            }
            Err(e) => eprintln!("nros build: warning: project lock not recorded: {e:#}"),
        }
    }
    drifts.dedup();
    Ok(drifts)
}

/// RFC-0104 D6 (phase-485 W6) — record what the host's system packages are and
/// say, once, what moved; warn on every build while one is outside its declared
/// range. Never refuses: the host's ROS is the user's, not ours to pin.
fn observe_system(args: &Args, regenerated: &[String]) {
    let root = match &args.workspace {
        Some(w) => w.clone(),
        None => match std::env::current_dir() {
            Ok(d) => d,
            Err(_) => return,
        },
    };
    let prefixes = crate::orchestration::system_obs::ament_prefixes();
    if prefixes.is_empty() {
        return; // no ROS environment: nothing to observe, and nothing to say
    }
    let Some(nr) = crate::orchestration::nano_ros_root::resolve(args.nano_ros_path.clone(), &root)
    else {
        return;
    };
    let Ok(index) =
        crate::orchestration::sdk_index::SdkIndex::load(&nr.join("nros-sdk-index.toml"))
    else {
        return;
    };
    let lock_path = crate::orchestration::store::lock_path_for(&root);
    let Ok(mut lock) = crate::orchestration::sdk_store::SdkLock::load(&lock_path) else {
        return;
    };
    let generated = crate::orchestration::system_obs::generated_names(&root);
    let observed = crate::orchestration::system_obs::observe(&index.compat, &generated, &prefixes);
    for line in crate::orchestration::system_obs::report(&lock.system, &observed, regenerated) {
        eprintln!("{line}");
    }
    let before = lock.clone();
    for o in observed {
        lock.system.insert(
            o.name,
            crate::orchestration::system_obs::SystemRecord { version: o.version },
        );
    }
    if lock != before
        && let Err(e) = lock.save(&lock_path)
    {
        eprintln!("nros build: warning: system observations not recorded: {e:#}");
    }
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

/// The native build's own failing exit status, carried out of [`drive`] so
/// `nros build` exits with it.
///
/// RFC-0065 D1 ended the pipeline in `exec`, which gave the caller the build's
/// status for free. Since phase-485 W3 every plan is WAITED on, because the
/// project lock is recorded after the build (RFC-0104 D4), and a process that
/// has become the build can record nothing. So the status is carried
/// explicitly: a CI step that keyed on `cargo`'s or `cmake`'s exit code still
/// sees it.
#[derive(Debug)]
pub(crate) struct ChildExit {
    pub code: i32,
    pub shown: String,
}

impl std::fmt::Display for ChildExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "`{}` exited {}", self.shown, self.code)
    }
}

impl std::error::Error for ChildExit {}

/// Perform one handover for real: run the native command as a child, stdio
/// INHERITED (never piped, so its stderr reaches the user untouched, which is
/// what D1's `exec` was for), and wait.
///
/// The seam [`drive`] is tested through: a test substitutes a recorder here so
/// it can observe every plan's handover without a compiler.
fn perform(hand: &Handoff) -> Result<()> {
    let st = crate::builder::handoff::wait(hand).map_err(|e| eyre::eyre!("{e}"))?;
    if st.success() {
        return Ok(());
    }
    #[cfg(unix)]
    let signal = std::os::unix::process::ExitStatusExt::signal(&st);
    #[cfg(not(unix))]
    let signal: Option<i32> = None;
    let code = st.code().or(signal.map(|s| 128 + s)).unwrap_or(1);
    Err(ChildExit {
        code,
        shown: hand.display(),
    }
    .into())
}

/// Stage 5 for every plan: announce it, then hand it over.
///
/// Split out of [`run`] for one reason — the loop is the thing that was wrong
/// (issue 1206) and it was the one part of `nros build` no test could reach.
/// `plan_builds` runs no tool and is heavily tested; `run` needs a real workspace on
/// disk; this needs neither.
///
/// `handover` performs one plan's command. Production passes [`perform`].
fn drive(
    plans: &[ResolvedBuild],
    dry_run: bool,
    handover: &mut dyn FnMut(&Handoff) -> Result<()>,
) -> Result<()> {
    let total = plans.len();
    for (i, p) in plans.iter().enumerate() {
        // The counter is the ONLY progress a multi-image build reports while
        // it runs — a user who sees `[3/7]` as the last line knows where it
        // stopped, and before this there was nothing that distinguished
        // "built 7" from "built 1 and said nothing about the other 6".
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
                 Images on Zephyr boards work now - they need no generated \
                 root (RFC-0065 D3).",
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
        // than during planning so planning never spawns a native tool — the
        // property that makes `--dry-run` trivially correct instead of a second
        // code path. (Planning does WRITE generated files; issue 1716.)
        if let Some(cfg) = &p.configure {
            run_configure(cfg)?;
        }
        let outcome = handover(hand);
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
    // phase-489 W6 — a cmake configure that cannot change anything is skipped:
    // `cmake -S -B` re-runs the whole configure on every call (20 of 20 rows of
    // a no-change C workspace re-run did), and `cmake --build`, which follows,
    // re-runs it itself when a registered input moved. See `cmake_configure`.
    let stamp = crate::builder::cmake_configure::ConfigureStamp::of(cfg);
    if let Some(s) = &stamp {
        if s.is_current() {
            eprintln!(
                "nros build: cmake configure up to date ({}); `cmake --build` re-checks its inputs",
                s.build_dir().display()
            );
            return Ok(());
        }
        s.invalidate();
    }
    run_configure_step(cfg)?;
    if let Some(s) = &stamp {
        s.record();
    }
    Ok(())
}

fn run_configure_step(cfg: &Handoff) -> Result<()> {
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
/// [`declared_capabilities`], owned, for a plan to carry.
fn owned_caps(bringup_dir: &std::path::Path) -> Vec<String> {
    declared_capabilities(bringup_dir)
        .into_iter()
        .map(str::to_string)
        .collect()
}

pub(crate) fn declared_capabilities(bringup_dir: &std::path::Path) -> Vec<&'static str> {
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
                     Pass --nano-ros-path <checkout>.",
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
                image_env: false,
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
            // Issue 1304 — same seam as the workspace lane above; `nros_root` is
            // the root this arm already refuses to proceed without.
            a.push(crate::builder::nano_ros_root_arg(nros_root));
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
            // RFC-0104 D3 (phase-485 W2): seeded from the pin set first, so the
            // resolve below keeps every pinned version the manifests accept.
            let had_lock = pkg.dir.join("Cargo.lock").is_file();
            crate::orchestration::registry_pins::seed(&pkg.dir, nano_ros_root)?;
            let configure = (!had_lock).then(|| {
                let mut u = vec!["update".to_string(), "--workspace".to_string()];
                if args.offline {
                    u.push("--offline".to_string());
                }
                Handoff::new("cargo", u).in_dir(&pkg.dir)
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
            board: "linux".to_string(),
            platform: "posix".to_string(),
            driver,
            handoff: Some(handoff),
            rmw: None,
            entry_package: None,
            target: None,
            profile: None,
            cmake_build_dir: None,
            capabilities: Vec::new(),
            configure,
            record: crate::orchestration::project_lock::BuildRecord {
                project: root.to_path_buf(),
                cargo_roots: if driver == Driver::Cargo {
                    vec![pkg.dir.clone()]
                } else {
                    Vec::new()
                },
                build_dirs: vec![build_dir.clone(), pkg.dir.join("target")],
            },
            output: None,
            descriptor: None,
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
    // phase-470 W5.b3 — does THIS image's node graph cross out of Rust? The
    // caller's `image_has_non_rust`, passed rather than re-derived: it reads the
    // discovered package set, which this function does not hold, and a second
    // derivation of "which language is this image" is exactly the drift class
    // that made `has_cpp` answer for the workspace when the question was about
    // the image.
    has_non_rust: bool,
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
    // phase-460 W1 follow-up -- the gated door, as every other consumer of a
    // resolved model. A refused or stale model stops the build here; only an
    // UNRESOLVABLE one keeps the warning below.
    let model_path = match open_model(bringup_dir, &model_rel) {
        Ok(p) => p,
        Err(ModelDoor::Refused(r)) => eyre::bail!("`{image_id}`: {r}"),
        Err(ModelDoor::Unresolved(e)) => {
            eprintln!("nros build: warning: cannot resolve the model for `{image_id}`: {e}");
            return Ok(GeneratedEntry::default());
        }
    };

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
        return Ok(GeneratedEntry { dir: None });
    }

    let mut plan = match crate::codegen::entry::plan_from_model(&model_path, image.board.clone()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("nros build: warning: cannot plan `{image_id}`: {e}");
            return Ok(GeneratedEntry { dir: None });
        }
    };
    // phase-470 W5.b2 (issue 1288) — resolve the SCHEDULE, because the board
    // crate's feature set depends on it (`builder::entry::schedule_board_features`,
    // which owns the rationale). Same `resolve_tiers` over the same model the
    // `nros::main!` expansion runs, so the feature and the code it gates cannot
    // disagree.
    //
    // A refusal here is REPORTED and not fatal: the macro resolves the same
    // table and will refuse with the same reason, at the point where the user
    // can read it against their own tier table. Deriving no features from a
    // table that did not resolve is then a compile error naming the missing
    // `run_tiers`, which is the loud half of the asymmetry that function
    // describes.
    let plan_rtos = nros_entry_lower::tier_rtos_key_for(&plan.board);
    if let Err(e) = crate::codegen::entry::resolve_plan_sched(&mut plan, plan_rtos) {
        eprintln!("nros build: warning: cannot resolve tiers for `{image_id}`: {e}");
    }

    // Issue 1706 -- a launch `<param>` seeds the parameter store through the
    // same declare path a `param_services` image uses, so the image needs the
    // store's heap whether or not it declares the axis. The decision (an
    // implied STORE, never implied services): a node that declares parameters
    // locally is legitimate ROS without the six services, so refusing it or
    // switching services on would both be wrong; what it cannot do without is
    // the allocation. A store reached only from APP code is invisible here and
    // is caught at run time instead, where the declaration is refused with the
    // number (`report_parameter_store_refused`).
    let launch_seeds_params = plan.nodes.iter().any(|n| !n.params.is_empty());

    let nodes = launch_closure(root, &plan);

    // Where the entry will be written — needed to make its dependency paths
    // relative, and computed before the spec because the spec carries them.
    // Also before the C/C++ branch below, which writes into it and nothing
    // else.
    let entry_dir_for_deps = root
        .join("build")
        .join(coordinate(platform, image))
        .join(crate::builder::entry::package_name(image_id));

    // ---- phase-470 W5.b3 (issue 1288) — the C/C++ west application --------
    //
    // A Zephyr image whose node graph crosses out of Rust gets an application
    // and NO cargo entry: `nano_ros_add_executable(... BRINGUP ...)` generates
    // the TU that carries `main`, so there is no `Cargo.toml`, no `src/lib.rs`
    // and no `build.rs` to write.
    //
    // Branching HERE, before the facade block, is the point: the selection
    // facade is a Rust crate that a Rust entry depends on, and it is emitted
    // per ENTRY PACKAGE. A C/C++ image has no entry crate to carry it, so
    // reaching that block would refuse a build over a facade nothing could
    // use. (The eight hand-written C/C++ applications never met it either —
    // they returned at the `hand_written` check above.)
    //
    // `has_non_rust` is the caller's `image_has_non_rust`, the same predicate
    // that routes a NATIVE image to the cmake driver. One predicate, so the
    // language a workspace is built in cannot differ by platform.
    if has_non_rust {
        if image.before_boot.is_some() {
            return Err(before_boot_not_on_c_road(image_id));
        }
        let mut app = crate::builder::west_app::resolve_cmake(
            root,
            &entry_dir_for_deps,
            bringup_dir,
            image_id,
            image.launch.as_deref(),
            &image.args,
            image.panic.as_deref(),
            platform,
            &nodes,
        )
        .map_err(|e| eyre::eyre!("generating the west application for `{image_id}`: {e}"))?;
        if launch_seeds_params {
            app.capabilities
                .push(crate::builder::west_app::PARAM_STORE_AXIS.to_string());
        }
        // phase-481 W1 -- the image, for the module hook's Kconfig fragment.
        // `nano_ros_add_executable(... TYPED)` is a C++ entry.
        app.image = Some(
            crate::builder::west_app::ImageStatement::new(
                &entry_dir_for_deps,
                bringup_dir,
                image_id,
                "cpp",
            )
            .map_err(|e| eyre::eyre!("generating the west application for `{image_id}`: {e}"))?,
        );
        crate::builder::west_app::write(&app, &entry_dir_for_deps)
            .map_err(|e| eyre::eyre!("generating the west application for `{image_id}`: {e}"))?;
        // phase-439 W6 (issue 1252) -- the image's message-bound fragment,
        // composed BEFORE the configure from the same closure, tables and
        // entity inventory the configure's composer will read, beside stage
        // 3.5's answer. The configure copies it in ahead of its first reader
        // (`nros_resolved_seed_message_bounds`), so a configure that agrees
        // spends no pass deriving it. Never fatal: no seed is today's
        // behaviour, and the reason is said once.
        if let Some(cm) = app.cmake.as_ref() {
            let resolved_dir = root
                .join("build")
                .join(crate::resolve::dir_name_for(bringup, image_id));
            let probe_codegen = metadata_probe_build_dir(root)
                .join("nros-codegen")
                .join(format!("nano_ros_{}", cm.lang));
            let node_dirs: Vec<std::path::PathBuf> = nodes.iter().map(|(_, d)| d.clone()).collect();
            match crate::message_bound_knobs::write_west_seed(
                &resolved_dir,
                &probe_codegen,
                &node_dirs,
            ) {
                Ok(p) => eprintln!("nros build:   message bounds seeded → {}", p.display()),
                Err(why) => eprintln!("nros build:   message bounds not seeded: {why}"),
            }
        }
        return Ok(GeneratedEntry {
            dir: Some(entry_dir_for_deps),
        });
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
            //     --> build/posix-zenoh/linux_rust_qos_entry/src/main.rs:9:1
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
            // Measured on this tree, `[image.linux_cyclonedds] rmw =
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
                //   Error: nros build: `linux_rust_qos` needs a selection
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
    // named after a board, but `[image.linux_service_server]` is not, and
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
    let candidates = crate::builder::entry::deploy_candidates(&board_name, image_id, platform);

    // Where the entry will be written — needed to make its dependency paths
    // relative, and computed before the spec because the spec carries them.
    let entry_dir_for_deps = generated_entry_dir(root, platform, image, image_id);

    // Snapshotted before the spec takes `nodes`: the Zephyr arm below seeds its
    // `[patch]` walk from them (`registry_patches_from`).
    let spec_nodes_dirs: Vec<PathBuf> = nodes.iter().map(|(_, d)| d.clone()).collect();

    // Every Rust road reaches this spec, so the key is checked here once.
    crate::orchestration::image::validate_before_boot(image.before_boot.as_deref())
        .map_err(|e| eyre::eyre!("`[image.{image_id}]`: {e}"))?;
    let mut spec = EntrySpec {
        image_id: image_id.to_string(),
        launch,
        args: image.args.clone(),
        panic: image.panic.clone(),
        before_boot: image.before_boot.clone(),
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
        // phase-470 W5.a — the WEST APPLICATION half. A Zephyr staticlib entry
        // IS the west application's directory (`rust_cargo_application()` runs
        // cargo from `CMAKE_CURRENT_SOURCE_DIR` with no `--manifest-path`), so
        // its manifest has to carry what no settings file can reach it with.
        west: match descriptor.entry_kind {
            crate::orchestration::board_descriptor::EntryKind::ZephyrStaticlib => Some(
                crate::builder::west_app::resolve(
                    root,
                    &entry_dir_for_deps,
                    nros_root,
                    image_id,
                    image.rmw.as_deref(),
                    &descriptor.platform_feature,
                    // Seeded from the NODE packages, because the entry manifest
                    // these rows go into has not been written yet.
                    &registry_patches_from(root, nros_root, &spec_nodes_dirs),
                )
                .and_then(|mut app| {
                    // Issue 1702 -- the declared axes reach Kconfig from the
                    // generated CMakeLists (see `capability_kconfig_cmake`).
                    app.capabilities = declared_capabilities(bringup_dir)
                        .into_iter()
                        .map(str::to_string)
                        .collect();
                    if launch_seeds_params {
                        app.capabilities
                            .push(crate::builder::west_app::PARAM_STORE_AXIS.to_string());
                    }
                    // phase-481 W1 -- the image, for the module hook.
                    app.image = Some(crate::builder::west_app::ImageStatement::new(
                        &entry_dir_for_deps,
                        bringup_dir,
                        image_id,
                        "rust",
                    )?);
                    Ok(app)
                })
                .map_err(|e| eyre::eyre!("{e}"))?,
            ),
            _ => None,
        },
    };
    let mut facts = BoardFacts::from_descriptor_for(descriptor, &candidates);
    // phase-470 W5.b2 (issue 1288) — the board-crate features this IMAGE's
    // schedule implies, on top of whatever its descriptor declares. The
    // descriptor's `board_features` is a property of the BOARD and cannot say
    // this: `tiers` is true of an image whose bringup declares them and false of
    // its sibling on the same board. `builder::entry::schedule_board_features`
    // owns the derivation and the case against declaring it instead.
    if let Some(krate) = facts.board_crate.clone() {
        let crate_dir = nros_root.join(
            facts
                .crate_path
                .clone()
                .unwrap_or_else(|| format!("packages/boards/{krate}")),
        );
        facts
            .board_features
            .extend(crate::builder::entry::schedule_board_features(
                plan.resolved_tiers.as_ref(),
                |f| crate::orchestration::facade::crate_declares_feature(&crate_dir, f),
            ));
        // Issue 1305 — the entry surface this board key's framework drives
        // (`board-entry` / `rtic` on the mps2 bare-metal crate), by the same
        // derivation a single-package leaf's selection package uses.
        facts
            .board_features
            .extend(crate::builder::entry::entry_surface_features(
                &candidates,
                |f| crate::orchestration::facade::crate_declares_feature(&crate_dir, f),
            ));
        facts.board_features.sort();
        facts.board_features.dedup();
    }
    let parent = entry_dir_for_deps
        .parent()
        .expect("generated_entry_dir always has a coordinate parent")
        .to_path_buf();
    let dir = crate::builder::entry::write(&spec, &facts, &parent)
        .map_err(|e| eyre::eyre!("generating the entry for `{image_id}`: {e}"))?;
    // The shell around it. AFTER the entry, because `is_materialized` guards
    // both and a user who took ownership owns the `CMakeLists.txt` too.
    if let Some(west) = &spec.west {
        crate::builder::west_app::write(west, &dir)
            .map_err(|e| eyre::eyre!("generating the west application for `{image_id}`: {e}"))?;
    }

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
        let facade_root = facade_root(root);
        let entry_name = crate::builder::entry::package_name(image_id);
        let healed = write_entry_facade(&entry_name, &dir, bringup_dir, &facade_root);

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
    } else if spec.facade_dir.is_some() {
        // Issue 1962 — a facade that EXISTS is rewritten too, every build.
        //
        // The heal above used to be the only writer this function had, so a
        // facade was written once — the first build that found none — and
        // then reused by every build after it, whatever generator wrote it.
        // `nros sync` does not refresh it either: sync writes a facade per
        // Rust package it can SEE, and a generated entry under
        // `build/<coordinate>/` is not one of them. When the generator grew
        // the `__nros_register_backends!` / `__nros_register_type!` macros
        // (RFC-0071 D11) the bridge workspaces kept a 2026-10-15 facade
        // without them, and their entries stopped compiling:
        //
        //     error[E0433]: cannot find `__nros_register_backends` in
        //     `linux_entry_nros_selection`
        //
        // `write_facade` writes only what changed, so this costs nothing on an
        // up-to-date tree, and the entry already depends on this directory, so
        // it needs no second write.
        let entry_name = crate::builder::entry::package_name(image_id);
        write_entry_facade(&entry_name, &dir, bringup_dir, &facade_root(root))
            .map_err(|e| eyre::eyre!("refreshing the selection facade for `{image_id}`: {e}"))?;
    }

    Ok(GeneratedEntry { dir: Some(dir) })
}

/// Write the selection facade for the generated entry at `dir` from the
/// bringup's `system.toml` — the heal for a missing one, and the refresh for
/// one an older generator wrote (issue 1962).
fn write_entry_facade(
    entry_name: &str,
    dir: &Path,
    bringup_dir: &Path,
    facade_root: &Path,
) -> Result<Option<crate::orchestration::facade::Facade>> {
    let raw = std::fs::read_to_string(bringup_dir.join("system.toml"))
        .map_err(|e| eyre::eyre!("reading {}: {e}", bringup_dir.join("system.toml").display()))?;
    let sys = toml::from_str::<crate::orchestration::cargo_metadata_schema::SystemToml>(&raw)
        .map_err(|e| eyre::eyre!("parsing the bringup's system.toml: {e}"))?;
    crate::orchestration::facade::write_facade(
        entry_name,
        dir,
        &dir.join("Cargo.toml"),
        bringup_dir,
        &sys,
        facade_root,
    )
}

/// What [`generate_entry`] produced for one image.
#[derive(Debug, Default)]
struct GeneratedEntry {
    /// The generated entry package, or `None` when a hand-written one
    /// suppressed generation or the launch could not be resolved here.
    dir: Option<PathBuf>,
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
    registry_patches_from(ws_root, nano_ros_root, &[entry_dir.to_path_buf()])
}

/// [`registry_patches`], seeded from several packages instead of one entry.
///
/// The Zephyr road needs this: its rows go INTO the entry manifest
/// (`builder::west_app` — the west driver has no settings file to put them
/// in), so they have to be derived BEFORE that manifest exists. Seeding from
/// the node packages the launch file names reaches the same closure, because
/// everything the entry contributes beyond them is a path dep outside the
/// workspace, which this walk does not follow anyway.
pub(crate) fn registry_patches_from(
    ws_root: &std::path::Path,
    nano_ros_root: &std::path::Path,
    seeds: &[PathBuf],
) -> std::collections::BTreeMap<String, PathBuf> {
    // Issue 0849 — rows are spelled from this root; a symlinked spelling beside
    // canonical seeds is two copies of every core crate to cargo.
    let nano_ros_root = &nano_ros_root
        .canonicalize()
        .unwrap_or_else(|_| nano_ros_root.to_path_buf());
    let mut out = std::collections::BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut queue: Vec<PathBuf> = seeds.to_vec();
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
    let pkg = crate::cmd::ws::cargo_package_name_of(&dir.join("Cargo.toml"))?;
    // Issue 1289 — an entry is the package an image CLAIMS, never one a
    // manifest table marks. Conservative like before: an unreadable or
    // ambiguous claim answers `None` here (the build reports it elsewhere).
    nros_orchestration_ir::leaf_system::claiming_image(
        dir,
        &pkg,
        bringup_dirs.iter().map(PathBuf::as_path),
    )
    .ok()
    .flatten()
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

/// The workspace packages a launch plan names, once each, in plan order --
/// the closure a C/C++ west application `add_subdirectory()`s
/// (`builder::west_app::resolve_cmake`), and therefore the packages whose
/// components its configure registers. A launch file may name one package
/// several times; the build needs it once.
fn launch_closure(
    root: &std::path::Path,
    plan: &crate::codegen::entry::Plan,
) -> Vec<(String, std::path::PathBuf)> {
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
    nodes
}

/// The build dir of `nros sync`'s metadata probe for this workspace, which
/// holds every workspace component's `nros-metadata.json` row and the codegen
/// output (`nros-codegen/`) the message-bound seed reads.
fn metadata_probe_build_dir(root: &std::path::Path) -> std::path::PathBuf {
    crate::orchestration::metadata_probe_cmake::probe_dir_for_workspace(
        &root.join("build").join("nros-metadata"),
    )
    .join("build")
}

/// phase-439 W7 (issue 1228) -- the population a C/C++ west image's configure
/// will register, from the metadata probe's rows of the packages its launch
/// plan names. `Err` says why there is none, and the resolve then composes from
/// the model alone and records no inputs, so the configure recomposes.
fn registered_population(
    root: &std::path::Path,
    model_path: &std::path::Path,
    board: Option<String>,
) -> Result<crate::entity_inventory::EntityInventory, String> {
    let plan = crate::codegen::entry::plan_from_model(model_path, board)
        .map_err(|e| format!("cannot plan the launch: {e}"))?;
    let packages: std::collections::BTreeSet<String> = launch_closure(root, &plan)
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    if packages.is_empty() {
        return Err("the launch plan names no workspace package".into());
    }
    let probe = metadata_probe_build_dir(root).join("nros-metadata.json");
    if !probe.is_file() {
        return Err(format!(
            "no metadata probe at {} (run `nros sync`)",
            probe.display()
        ));
    }
    crate::cmd::entity_inventory::registered_from_probe(&probe, &packages)
        .map_err(|e| format!("{e:#}"))
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
/// `[knobs.memory] heap_bytes` for the board this image names.
///
/// The RFC-0049 board rung, read off the descriptor's own file. The leaf road
/// reaches the same number through `NROS_BOARD_TOML`, which `board_facts`
/// resolved for it; a workspace image has no such table, but it has the
/// descriptor, and the descriptor knows where it was parsed from.
///
/// `None` for an in-memory descriptor, a board that states no memory rung, or a
/// file that no longer parses — and `None` is an ANSWER here: `[target]` then
/// refuses `heap_budget_bytes`, which is Cyclone's "a board that states no heap
/// gets no judgement" (RFC-0100 D11).
///
/// An in-tree descriptor's `source` is RELATIVE TO THE NANO-ROS ROOT (it goes
/// into committed artifacts), so it is joined onto that root, the way
/// `board_facts` resolves `NROS_BOARD_TOML`. It used to be opened as given,
/// i.e. relative to the CWD: correct only when `nros build` ran from the
/// checkout root, and silently `None` -- the heap unstated, D11 inert -- from
/// the workspace directory a user actually builds in (found by issue 1653's
/// west measurement). An absolute `source` (an extra root or a workspace
/// board) is unaffected by the join.
fn board_heap_budget(
    descriptor: &crate::orchestration::board_descriptor::BoardDescriptor,
    board: &str,
    nano_ros_root: Option<&std::path::Path>,
) -> Option<usize> {
    let path = board_descriptor_file(descriptor, nano_ros_root)?;
    nros_board_common::platform_config::BoardKnobsFile::load_for_board(&path, Some(board))
        .ok()?
        .knobs
        .memory
        .heap_bytes
}

/// Phase 518 (issue 2072) -- the build type for a cmake configure of a BOARD
/// image, which `nros build` used to leave unset.
///
/// An empty `CMAKE_BUILD_TYPE` compiles C and C++ with no optimisation, and
/// the profile table maps it to cargo's `dev` profile for the Rust half. On a
/// host that is a debugging default; on an MCU it is a different program.
/// Measured on mps2-an385 FreeRTOS: a ported Autoware node's
/// `diagnostic_updater::Updater::publish_now` reserved 2.2 MB of stack for one
/// call. The fixture road always passed `Release`, so no test ran what
/// `nros build` produced.
///
/// `Release` for every platform but `posix`, whose images are host processes.
/// Pushed before the caller's `native_args`, so `-DCMAKE_BUILD_TYPE=Debug`
/// still wins: cmake takes the last definition on the line.
fn board_build_type_arg(platform: &str) -> Option<String> {
    (platform != "posix").then(|| "-DCMAKE_BUILD_TYPE=Release".to_string())
}

/// Issue 1653 — the BOARD heap as a configure argument, for the two drivers
/// whose configure line `nros build` writes (cmake, and west's cmake zone).
///
/// `-DNROS_BOARD_HEAP_BUDGET_BYTES=<n>` when the board states `[knobs.memory]
/// heap_bytes`; the configure never reads the board file itself (RFC-0064 R5 D4).
/// ONE spelling for both arms: the west arm had none, which is the half issue
/// 1653 left open.
///
/// **ALWAYS emitted, empty when the board states nothing.** A `-D` is a CACHE
/// entry, so omitting the argument once a board stops stating a heap leaves the
/// previous value in `CMakeCache.txt`, and the descriptor goes on stating a heap
/// nobody states any more -- measured on both roads (`freertos_posix` kept
/// 111000, `zephyr_cyclonedds` 2097152, each after the board line was deleted).
/// The empty value is the configure's own spelling of "nobody said"
/// (`_nros_sizing_heap_args` treats it as unset), so the field goes back to
/// refused on the next configure.
fn board_heap_configure_arg(
    descriptor: &crate::orchestration::board_descriptor::BoardDescriptor,
    board: &str,
    nano_ros_root: Option<&std::path::Path>,
) -> String {
    let value = board_heap_budget(descriptor, board, nano_ros_root)
        .map(|h| h.to_string())
        .unwrap_or_default();
    format!("-DNROS_BOARD_HEAP_BUDGET_BYTES={value}")
}

/// phase-462 W6c (RFC-0074) — the board's `[board.rx]` per-frame receive cost
/// as configure arguments, for the two drivers whose configure line `nros
/// build` writes. `cmake/NanoRosBoardRxCost.cmake` hands them to both CLI
/// producers that derive the zenoh read-task budget.
///
/// ALWAYS both, empty when the board states nothing, for
/// [`board_heap_configure_arg`]'s reason: a `-D` is a cache entry, and an
/// omitted one keeps the value a board no longer states.
fn board_rx_configure_args(
    descriptor: &crate::orchestration::board_descriptor::BoardDescriptor,
) -> [String; 2] {
    let rx = descriptor.rx;
    [
        format!(
            "-DNROS_BOARD_RX_FRAME_COST_US={}",
            rx.map(|r| r.frame_cost_us.to_string()).unwrap_or_default()
        ),
        format!(
            "-DNROS_BOARD_RX_FLOOR_US={}",
            rx.and_then(|r| r.floor_us)
                .map(|f| f.to_string())
                .unwrap_or_default()
        ),
    ]
}

/// RFC-0103 D10 (phase-497 W2) — a board's `[board.net]`, from the
/// descriptor the catalog recorded for it. A board with no descriptor file
/// has no interfaces, so an image naming one is refused, never dropped.
pub(crate) fn descriptor_board_net(
    descriptor: &crate::orchestration::board_descriptor::BoardDescriptor,
    board: &str,
    nano_ros_root: Option<&std::path::Path>,
) -> Result<nros_board_common::platform_config::BoardNet> {
    match board_descriptor_file(descriptor, nano_ros_root) {
        Some(path) => {
            crate::orchestration::image_net::board_net(&path, board).map_err(|e| eyre::eyre!("{e}"))
        }
        None => Ok(Default::default()),
    }
}

/// A single-package leaf's resolved network (`None`: it states none).
pub(crate) fn leaf_image_net(
    descriptor: &crate::orchestration::board_descriptor::BoardDescriptor,
    board: &str,
    nano_ros_root: Option<&std::path::Path>,
    decl: &nros_orchestration_ir::leaf_system::LeafSystem,
) -> Result<Option<crate::orchestration::image_net::ImageNet>> {
    if decl.os_config.is_empty() {
        return Ok(None);
    }
    let net = descriptor_board_net(descriptor, board, nano_ros_root)?;
    // RFC-0109 D9 check 6 — each layer-1 file goes to the network's owner.
    net.check_os_config(&decl.os_config)
        .map_err(|e| eyre::eyre!("on board `{board}`: {e}"))?;
    if decl.network.net.is_empty() {
        return Ok(None);
    }
    crate::orchestration::image_net::resolve_leaf(decl, &net)
        .map_err(|e| eyre::eyre!("on board `{board}`: {e}"))
}

/// A workspace image's resolved network, from its bringup's `system.toml`,
/// with every image of that bringup resolved alongside it so a collision on a
/// shared network is refused naming both.
pub(crate) fn workspace_image_net(
    catalog: &crate::orchestration::board_descriptor::BoardCatalog,
    bringup_dir: &std::path::Path,
    image_id: &str,
    nano_ros_root: Option<&std::path::Path>,
) -> Result<Option<crate::orchestration::image_net::ImageNet>> {
    let path = bringup_dir.join("system.toml");
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let mut sys = toml::from_str::<crate::orchestration::cargo_metadata_schema::SystemToml>(&raw)
        .map_err(|e| eyre::eyre!("{}: {e}", path.display()))?;
    // RFC-0109 D4 (phase-514 W2) — the board network files the images name.
    sys.fold_os_config(bringup_dir)
        .map_err(|e| eyre::eyre!("{}: {e}", path.display()))?;
    let mut all = crate::orchestration::image_net::resolve_system(&sys, |id| {
        let img = match &sys.image_defaults {
            Some(base) => sys.image[id].with_base(base),
            None => sys.image[id].clone(),
        };
        let board = img.board.clone().unwrap_or_default();
        let d = crate::orchestration::image::resolve_image_board(catalog, id, &img)?;
        descriptor_board_net(d, &board, nano_ros_root).map_err(|e| format!("{e}"))
    })
    .map_err(|e| eyre::eyre!("{}: {e}", path.display()))?;
    Ok(all.remove(image_id))
}

fn board_descriptor_file(
    descriptor: &crate::orchestration::board_descriptor::BoardDescriptor,
    nano_ros_root: Option<&std::path::Path>,
) -> Option<std::path::PathBuf> {
    let src = std::path::Path::new(descriptor.source.as_deref()?);
    if src.is_absolute() {
        return Some(src.to_path_buf());
    }
    Some(nano_ros_root?.join(src))
}

/// What stage 3.5 resolved for one image.
///
/// The INVENTORY rides beside [`crate::resolve::Resolved`] rather than inside
/// it, and that is not tidiness. `Resolved` is the `resolved.toml` artifact's
/// data model and holds only derived VALUES — *"`Refused` carries prose and NO
/// numbers"* — while phase-454 W14's second descriptor producer needs the ROWS:
/// every endpoint's kind, type, topic and four QoS policies. Folding those into
/// `Resolved` would put a second, wider schema behind a file that publishes a
/// narrower one.
struct ResolvedImage {
    /// `build/<bringup>__<entry>/`, where `resolved.toml` was written.
    dir: std::path::PathBuf,
    resolved: crate::resolve::Resolved,
    /// The composed inventory, when the model described wiring at all.
    ///
    /// `None` is the ordinary state: 109 of 114 resolvable models describe no
    /// wiring, and `EntityInventory::from_model` returns `None` for each. It is
    /// also the signal that NO descriptor is written for this image — "no
    /// contract, no change" (phase-454 W12's control, held on this road too).
    inventory: Option<crate::entity_inventory::EntityInventory>,
    /// The resolved model the inventory was composed from -- issue 1594's
    /// observation join needs its contract and its remaps. `Some` exactly when
    /// `inventory` is.
    model: Option<nros_orchestration_ir::overlay::LoadedModel>,
    /// Issue 1706 -- the bringup's parameter-store families, whenever the
    /// model RESOLVED, wiring or not: a model with no wiring writes no full
    /// descriptor, and still says whether the image builds a store.
    store_infra: Option<crate::entity_inventory::InfraServices>,
}

/// `resolved.toml`'s `[locations]` — phase-484 W6c (RFC-0103 D7).
///
/// Every `[source.*]` row of the nano-ros root's index that the ONE ladder
/// answers for, with this workspace as the project (its `system.toml`
/// `[sources]` is rung 3). A row the ladder cannot answer is left out rather
/// than recorded as a refusal: the index names every platform's trees, most of
/// which a given image never reads, and `nros locate --all --why` already
/// explains a miss. Empty when there is no nano-ros root or its index will not
/// load — the record is never a reason for a build to stop.
/// Why [`open_model`] gave no model.
enum ModelDoor {
    /// `nros sync` refused it, or its provenance no longer holds.
    Refused(crate::model_gate::Refusal),
    /// No model, and none could be resolved from the inputs.
    Unresolved(String),
}

/// phase-460 W1 follow-up -- the one door `nros build` opens a resolved model
/// through. A model `nros sync` produced is verified like at every other door
/// (`model_gate::open`: the refusal marker on every rung, the resolver pin,
/// the recorded and the launch-tree inputs). Only when no rung
/// holds a model does it resolve one from the inputs (`ensure_model`, which
/// itself refuses on a marker), so a model this build resolved a moment ago
/// is current by construction.
fn open_model(
    bringup_dir: &std::path::Path,
    model_rel: &str,
) -> Result<std::path::PathBuf, ModelDoor> {
    match crate::model_gate::open(bringup_dir, model_rel) {
        Err(r) => Err(ModelDoor::Refused(r)),
        Ok(Some(p)) => Ok(p),
        Ok(None) => nros_orchestration_ir::model_location::ensure_model(bringup_dir, model_rel)
            .map(|(p, _inputs)| p)
            .map_err(ModelDoor::Unresolved),
    }
}

fn image_locations(
    nano_ros_root: Option<&std::path::Path>,
    project: &std::path::Path,
) -> Vec<crate::resolve::Location> {
    let Some(nr) = nano_ros_root else {
        return Vec::new();
    };
    let Ok(index) =
        crate::orchestration::sdk_index::SdkIndex::load(&nr.join("nros-sdk-index.toml"))
    else {
        return Vec::new();
    };
    index
        .source
        .keys()
        .filter_map(|name| {
            crate::cmd::locate::locate_answer(&index, nr, name, Some(project))
                .ok()
                .map(|a| crate::resolve::Location {
                    name: name.clone(),
                    answer: Ok((nros_build_paths::canonical(&a.path), a.rung.label())),
                })
        })
        .collect()
}

// Eight declarations of one image, each read where it already lives; a struct
// built only to carry them across this one call would be a second home for them.
#[allow(clippy::too_many_arguments)]
fn resolve_image(
    root: &std::path::Path,
    bringup: &str,
    bringup_dir: &std::path::Path,
    image_id: &str,
    image: &crate::orchestration::image::ImageBlock,
    platform: &str,
    board: &str,
    nano_ros_root: Option<&std::path::Path>,
    rx_cost: Option<crate::entity_inventory::RxFrameCost>,
    compose_registered: bool,
) -> Option<ResolvedImage> {
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
    // The model rides beside the inventory for issue 1594's observation join.
    type Composed = (EntityInventory, nros_orchestration_ir::overlay::LoadedModel);
    let mut store_infra = None;
    // phase-439 W7 -- set exactly when the composition below ran over the
    // registered population, which is what lets the configure read it.
    let mut entity_inputs: Option<crate::resolve::EntityInputs> = None;
    let inventory: Result<Composed, String> = (|| {
        let model_rel =
            model_location::launch_to_model_rel(bringup_dir, image.launch.as_deref(), &args_vec)
                .map_err(|e| format!("cannot resolve the launch for this image: {e}"))?;
        // phase-460 W1 follow-up -- the gated door. This phase cannot fail a
        // build, so a refused or stale model is its refusal reason: no number
        // is read from it.
        let model_path = open_model(bringup_dir, &model_rel).map_err(|e| match e {
            ModelDoor::Refused(r) => r.to_string(),
            ModelDoor::Unresolved(e) => format!("cannot resolve the SystemModel: {e}"),
        })?;
        let raw = std::fs::read_to_string(&model_path)
            .map_err(|e| format!("reading {}: {e}", model_path.display()))?;
        let model: ros_launch_manifest_model::SystemModel = serde_yaml_ng::from_str(&raw)
            .map_err(|e| format!("parsing {}: {e}", model_path.display()))?;
        // phase-486 W3 -- the switches THIS image builds with: the overlay's
        // system list plus its own `[image.<id>] features` (W2).
        let model =
            nros_orchestration_ir::overlay::LoadedModel::with_overlay_beside(model, &model_path)?;
        store_infra = Some(crate::entity_inventory::InfraServices::from_model(
            &model,
            Some(image_id),
        ));
        // phase-454 W3 -- a QoS value this build cannot read refuses the SEED
        // too. `from_model` parses `reliability:` / `durability:` / `history:`
        // and an unreadable spelling parses to `None`, which is the spelling
        // for NOBODY SAID -- so without this the seed would compose an image
        // whose declaration had been silently dropped. The verb fatals on the
        // same model a moment later; here it is a refusal reason, because a
        // seed that cannot answer is a normal state and a seed that answers
        // WRONGLY is not.
        crate::cmd::entity_inventory::reject_unknown_qos_values(&model)
            .map_err(|e| format!("{e}"))?;
        // phase-454 W7 (RFC-0100 D8) -- and a `qos_overrides.*` parameter that
        // disagrees with the contract refuses the seed, for the same reason:
        // the seed would otherwise compose an image sized from the contract
        // while the bake ran the parameter's QoS. A refusal reason and not a
        // process failure, exactly as above -- `nros ws entity-inventory`
        // fatals on the same model at configure time.
        crate::cmd::entity_inventory::reject_qos_override_divergence(&model)
            .map_err(|e| format!("{e}"))?;
        // `None` is "no wiring described", which is a DECLARATION GAP and not
        // an error: 5 of the tree's 114 resolvable models describe wiring, and
        // they are exactly the 5 with a contract sidecar (issue 0973).
        //
        // phase-446 W4 -- the contract's `params:` ride along, exactly as the
        // configure-time producer attaches them, so the two fragments agree
        // byte for byte (issue 1228).
        //
        // Issue 1436 -- composed BEFORE the entity inventory, and independently
        // of it. The cmake road states the rule in its own words ("a model with
        // no topics can still declare parameters"); this road nested the
        // parameter half inside the entity half's `Some` arm, so a
        // parameter-only contract lost every parameter fact AND failed the
        // resolve with a message about wiring.
        //
        // Nesting the attach is safe now, and ONLY because `from_model`'s
        // predicate covers `node_params` too: `None` here implies the contract
        // declared no parameters either. The `debug_assert!` pins that
        // implication, so narrowing the predicate again fails loudly here
        // instead of silently dropping the facts a second time.
        let params = crate::entity_inventory::ParamDeclarations::from_model(&model);

        // phase-439 W7 (issue 1228) -- a C/C++ west image is composed the way
        // its configure composes it: the population its packages REGISTER,
        // with the contract folded in, by `nros ws entity-inventory`'s own
        // function. The registered rows are the refusal guard the model alone
        // lacks -- a registered component no contract describes is an absent
        // row, and the derivation refuses here exactly as it would there. The
        // verb's refusals become this phase's (it cannot fail a build), so a
        // model the configure would reject leaves no projection to read.
        if compose_registered && EntityInventory::from_model("", &model).is_some() {
            match registered_population(root, &model_path, image.board.clone()) {
                Ok(registered) => {
                    crate::cmd::entity_inventory::check_model(&model)
                        .map_err(|e| format!("{e:#}"))?;
                    let inputs =
                        crate::resolve::EntityInputs::new(&registered, &model_path, root, rx_cost);
                    let inv = crate::cmd::entity_inventory::compose_image(
                        registered,
                        &model_path.display().to_string(),
                        &model,
                        Some(root),
                        rx_cost,
                        "nros build:  ",
                    )
                    .map_err(|e| format!("{e:#}"))?;
                    entity_inputs = Some(inputs);
                    return Ok((inv, model.clone()));
                }
                Err(why) => eprintln!(
                    "nros build:   entity inventory composed from the model alone ({why}); \
                     the configure will recompose it"
                ),
            }
        }
        //
        // Issue 1694 -- a contract states what it names, and a count is never
        // smaller than what the launched nodes' code creates: floored by the
        // workspace's probe sidecars, the same composition the cmake road's
        // configure performs. A contract describing only the talker used to
        // size this image from the talker's publisher alone (`MAX_CBS = 0`,
        // `ExecutorFull` at boot).
        crate::orchestration::metadata_refresh::contract_inventory(
            model_path.display().to_string(),
            &model,
            Some(root),
            "nros build:  ",
        )
        .map(|mut inv| {
            inv.set_param_declarations(params.clone());
            // phase-462 W6c -- the board's per-frame receive cost, so the
            // read-task budget `resolved.toml` publishes and the descriptor the
            // cargo road writes from this composition derive with the same
            // `c`. Not CHECKED here (this phase cannot fail a build): the
            // descriptor producer checks, and refuses.
            inv.set_board_rx_cost(rx_cost);
            (inv, model.clone())
        })
        .ok_or_else(|| {
            debug_assert!(
                matches!(params, crate::entity_inventory::ParamDeclarations::Absent),
                "issue 1436: no entity inventory, but the contract declares parameters \
                     (`{}`) -- `EntityInventory::from_model` no longer covers `node_params` \
                     and this road is discarding them again",
                params.tag()
            );
            format!(
                "the launch tree resolved, and it declares neither wiring nor \
                 parameters. Nothing here can derive a count. State what each node \
                 creates -- or what parameters it declares -- in the contract sidecar \
                 beside the launch file ({}/launch/<stem>.contract.yaml); until then this \
                 image keeps its configured pool knobs.",
                bringup_dir.display()
            )
        })
    })();

    // Cloned BEFORE `write_resolved` consumes the reference, so the second
    // descriptor producer (phase-454 W14) reads the SAME composition this
    // resolve published rather than re-parsing the model into a second one --
    // which is the drift `nros_derive_entity_inventory_knobs`'s own header warns
    // about one artifact over ("a second count in cmake is how two green tools
    // come to disagree").
    let composed = inventory.as_ref().ok().cloned();
    let locations = image_locations(nano_ros_root, root);
    let written = match inventory {
        Ok((inv, _)) => write_resolved(&dir, ident, &inv, locations).and_then(|w| {
            crate::resolve::write_entity_inputs(&w, &dir, entity_inputs.as_ref(), &inv)?;
            Ok(w)
        }),
        Err(reason) => {
            let r = crate::resolve::Resolved::unresolvable(
                ident,
                bringup_dir.join("launch").display().to_string(),
                reason,
            )
            .with_locations(locations);
            // Same two writes as the derived path, minus the projection — kept
            // inline rather than routed through `write` because there is no
            // inventory to render one from.
            match std::fs::create_dir_all(&dir)
                .and_then(|()| {
                    // phase-439 W7 -- and the record a configure would
                    // read it by: no projection, nothing to take.
                    for name in [
                        crate::resolve::RESOLVED_CMAKE_NAME,
                        crate::resolve::RESOLVED_ENTITY_INPUTS_NAME,
                        crate::resolve::RESOLVED_ENTITY_JSON_NAME,
                    ] {
                        let stale = dir.join(name);
                        if stale.exists() {
                            std::fs::remove_file(&stale)?;
                        }
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
                    "nros build:   resolved → {} (entity counts not derived; configured defaults kept)",
                    w.toml_path.display()
                ),
            }
            Some(ResolvedImage {
                dir,
                resolved: w.resolved,
                model: composed.as_ref().map(|(_, m)| m.clone()),
                inventory: composed.map(|(i, _)| i),
                store_infra,
            })
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
///
/// phase-481 W4 (RFC-0098 D12) — and CONFIGURATIONS. The configure builds
/// `nros-c` / `nros-cpp` once for every image on it, so an image whose
/// `[image.<id>] env` differs cannot share it: its knobs are compiled into
/// those builds. An image that states configuration gets a `-cfg<hash>` suffix
/// over [`image_config_digest`]; images whose resolved configuration is
/// identical — including the ones that state none, which keep exactly the
/// path they had — still share one configure.
///
/// phase-441 W6 — the bringup's own RMW config (`rmw/<backend>.<ext>`,
/// phase-206 W2) is such a fact too: `nros_bake_rmw_user_config` writes it
/// ONCE per configure and it compiles into the shared `nros_rmw_<backend>`,
/// so two bringups with different configs on one configure each got whichever
/// was baked last. `bringup_dir` is the image's bringup.
fn cmake_coordinate(
    platform: &str,
    image: &crate::orchestration::image::ImageBlock,
    bringup_dir: &Path,
) -> String {
    let base = coordinate(platform, image);
    let base = match image.board.as_deref() {
        Some(b) if b != platform => format!("{base}-{}", b.replace(['/', '.'], "-")),
        _ => base,
    };
    match image_config_digest(image, bringup_dir) {
        Some(d) => format!("{base}-cfg{d}"),
        None => base,
    }
}

/// [`ResolvedBuild::cmake_build_dir`] for a workspace image: its cmake binary
/// dir when the driver is cmake, else `None`.
fn cmake_build_dir_for(
    driver: Driver,
    platform: &str,
    image: &crate::orchestration::image::ImageBlock,
    bringup_dir: &Path,
) -> Option<String> {
    matches!(driver, Driver::CMake).then(|| {
        format!(
            "build/{}/cmake",
            cmake_coordinate(platform, image, bringup_dir)
        )
    })
}

/// A short, stable digest of an image's resolved configuration
/// ([`crate::cmd::leaf_settings::image_block_layers`]), or `None` when it
/// states none. Keyed on the ROWS, never on their spelling in `system.toml`,
/// so reordering or re-quoting a table does not move the build directory.
/// Deploy keys (`ip`, `locator`, `domain_id`) are deliberately NOT in it: they
/// reach each image's own generated entry, never the shared runtime builds.
///
/// The bringup's RMW config file, when it ships one for the image's backend,
/// is one more row, keyed on its CONTENT: two bringups with the same bytes
/// still share a configure, and editing the file moves the directory the same
/// way editing a knob does.
fn image_config_digest(
    image: &crate::orchestration::image::ImageBlock,
    bringup_dir: &Path,
) -> Option<String> {
    use sha2::{Digest as _, Sha256};
    // A retired `[image] env` key is refused where the rows are WRITTEN (the
    // cmake road's image-env file); the coordinate only needs a stable name.
    let mut rows = crate::cmd::leaf_settings::image_block_layers(None, image).ok()?;
    if let Some(cfg) = rmw_user_config_file(bringup_dir, image.rmw.as_deref())
        && let Ok(body) = std::fs::read_to_string(&cfg)
    {
        rows.insert("rmw-user-config".to_string(), body);
    }
    if rows.is_empty() {
        return None;
    }
    let mut h = Sha256::new();
    for (k, v) in &rows {
        h.update(k.as_bytes());
        h.update([0u8]);
        h.update(v.as_bytes());
        h.update([0xffu8]);
    }
    Some(format!("{:x}", h.finalize())[..10].to_string())
}

/// The bringup's own RMW config for `rmw` — the file its backend's descriptor
/// names (`[rmw.config] file`, RFC-0109 D8) — when that file exists.
fn rmw_user_config_file(bringup_dir: &Path, rmw: Option<&str>) -> Option<PathBuf> {
    crate::orchestration::rmw_config::find(bringup_dir, rmw)
}

/// Where `nros build` GENERATES an image's entry package:
/// `build/<coordinate>/<image_id>_entry`.
///
/// ONE spelling (issue 1582). Three sites computed it inline — the cargo
/// root's manifest dir, the dependency-path base and the write — and none of
/// them carries the BRINGUP. That is deliberate and is not the defect: the
/// path is user-facing (`build/posix-zenoh/linux_entry/target/debug/…` is in
/// the book) and an image id is what a user names. The defect was that
/// nothing stopped two bringups from both generating `[image.zephyr]` into it:
/// see [`generated_output_collisions`].
fn generated_entry_dir(
    root: &Path,
    platform: &str,
    image: &crate::orchestration::image::ImageBlock,
    image_id: &str,
) -> PathBuf {
    root.join("build")
        .join(coordinate(platform, image))
        .join(crate::builder::entry::package_name(image_id))
}

/// Issue 1669 -- where `nros build` points `west build -d` for an image:
/// `build/<coordinate>/<bringup>__<image_id>_west`, beside the generated
/// application. Keyed on the BRINGUP as well as the id, because two bringups
/// may declare one image id over one hand-written application (`realtime-c`,
/// legal -- [`generated_outputs`] claims nothing for it): the application is
/// shared, the build tree must not be. The `<bringup>__<image>` spelling is
/// the one `build/<bringup>__<image>/resolved.toml` already uses. So no two
/// images share a Zephyr build tree and none lands in `build/` itself. ONE
/// spelling.
pub(crate) fn west_build_dir(
    root: &Path,
    platform: &str,
    image: &crate::orchestration::image::ImageBlock,
    bringup: &str,
    image_id: &str,
) -> PathBuf {
    root.join("build")
        .join(coordinate(platform, image))
        .join(format!("{bringup}__{image_id}_west"))
}

/// Does the passthrough already name west's build dir (`-d <dir>`,
/// `--build-dir <dir>`, `-d<dir>`, `--build-dir=<dir>`)? Then it wins.
fn names_west_build_dir(west_extra: &[String]) -> bool {
    west_extra.iter().any(|a| {
        a == "-d"
            || a == "--build-dir"
            || a.starts_with("--build-dir=")
            || (a.starts_with("-d") && a.len() > 2 && !a.starts_with("--"))
    })
}

/// The directory the build's missing-facade HEAL writes selection facades
/// under — `generated/nros-selection/`, keyed by the ENTRY name alone.
fn facade_root(root: &Path) -> PathBuf {
    root.join("generated/nros-selection")
}

/// Every path an image's build GENERATES that is keyed on its image id,
/// workspace-relative — or nothing, when a hand-written application suppresses
/// generation.
///
/// Issue 1582. Each of these is keyed on the image ID and not on the bringup,
/// so the id is a WORKSPACE-wide name for them even though RFC-0065 F7 lets
/// several bringups declare it (`plan::pick_one` rightly refuses to CHOOSE
/// between them). Two bringups generating the same one write one directory,
/// last writer wins: `realtime-rust`'s `demo_bringup` and `derived_bringup`
/// both declared `[image.zephyr]`, both west leaves configured
/// `APPLICATION_SOURCE_DIR = …/build/zephyr-zenoh/zephyr_entry`, and the
/// authored-tier image booted `derived-telem_node`.
///
/// The predicate is the build's own, per road:
///
/// * a hand-written `src/<id>_entry` (either build file) suppresses
///   generation on every road (`generate_entry`, and the cmake root's
///   "contributes nothing");
/// * on the west road an `[image.<id>] entry` names the application, and
///   `generate_entry` is never called;
/// * cargo and west write the entry package and (on a heal) its facade;
///   cmake emits a target of that name into the root at `build/<cmake-coord>/`.
fn generated_outputs(
    root: &Path,
    image_id: &str,
    image: &crate::orchestration::image::ImageBlock,
    platform: &str,
    driver: Driver,
    bringup_dir: &Path,
) -> Vec<PathBuf> {
    let pkg = crate::builder::entry::package_name(image_id);
    let hand_written = root.join("src").join(&pkg);
    if hand_written.join("Cargo.toml").is_file() || hand_written.join("CMakeLists.txt").is_file() {
        return Vec::new();
    }
    let rel = |p: PathBuf| p.strip_prefix(root).map(Path::to_path_buf).unwrap_or(p);
    match driver {
        Driver::West if image.entry.is_some() => Vec::new(),
        Driver::Cargo | Driver::West => vec![
            rel(generated_entry_dir(root, platform, image, image_id)),
            rel(facade_root(root).join(&pkg)),
        ],
        Driver::CMake => vec![
            PathBuf::from("build")
                .join(cmake_coordinate(platform, image, bringup_dir))
                .join(format!("<target {pkg}>")),
        ],
    }
}

/// Group `(qualified image, its generated outputs)` by output, keeping only
/// the outputs more than one image claims. Ordered, so the refusal is stable.
fn generated_output_collisions(claims: &[(String, Vec<PathBuf>)]) -> Vec<(PathBuf, Vec<String>)> {
    let mut by_path: std::collections::BTreeMap<PathBuf, Vec<String>> =
        std::collections::BTreeMap::new();
    for (qual, outs) in claims {
        for o in outs {
            let v = by_path.entry(o.clone()).or_default();
            if !v.contains(qual) {
                v.push(qual.clone());
            }
        }
    }
    by_path
        .into_iter()
        .filter(|(_, q)| q.len() > 1)
        .map(|(p, mut q)| {
            q.sort();
            (p, q)
        })
        .collect()
}

/// The refusal text for [`generated_output_collisions`], or `None`.
fn generated_output_collision_message(collisions: &[(PathBuf, Vec<String>)]) -> Option<String> {
    if collisions.is_empty() {
        return None;
    }
    let lines: Vec<String> = collisions
        .iter()
        .map(|(p, q)| format!("  {}  <-  {}", p.display(), q.join(", ")))
        .collect();
    Some(format!(
        "two images in this workspace would GENERATE the same output (issue 1582):\n\n{}\n\n  \
         A generated entry, its selection facade and its cmake target are keyed on the \
         image ID, not the bringup, so the second build overwrites the first and each \
         image then carries whichever was written last. Give one of them a distinct id \
         (`[image.<id>]` in its bringup's system.toml), or a hand-written application.",
        lines.join("\n")
    ))
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
/// Ambiguity is an ERROR, not a first match: whichever one a scan returns
/// first is right half the time and silently wrong the other half, so this
/// refuses until the image names its application (`entry = "..."`).
///
/// Two packages collide here whenever they declare the same `DEPLOY` token,
/// because a token resolves to a BOARD and that is all this compares. Note
/// what that means for a Zephyr entry, measured under issue 1517: `DEPLOY`
/// names the PLATFORM (`zephyr`) — `NanoRosEntry.cmake`'s link gate compares
/// it against `NROS_PLATFORM` — while the board the application really
/// targets is chosen by `nano_ros_use_board(<board>)` inside its own
/// CMakeLists, which this does not read. So for a Zephyr image on any board
/// but the `zephyr` descriptor's own, the derivation finds ZERO candidates
/// rather than too many, and the image must still name its entry.
/// `realtime-cpp`'s `[image.fvp]` was that case (and before that the
/// ambiguity case, its `board` misread as `native_sim/native/64` making
/// `fvp_entry` and `zephyr_entry` both match) until issue 1288 deleted its
/// hand-written application.
/// RFC-0065 D5 amendment — `before_boot` is a `nros::main!` argument, so it
/// has a home only in a Rust entry. A C/C++ generated entry is a
/// `nano_ros_entry(...)` TU with no such hook yet; refusing names the image
/// rather than dropping the step silently.
fn before_boot_not_on_c_road(image_id: &str) -> eyre::Report {
    eyre::eyre!(
        "`[image.{image_id}] before_boot` applies to a Rust entry, and this image's entry is \
         C/C++ (its package graph holds a C/C++ package). Run the step from a node's \
         `register`, or materialise the entry (`nros materialize {image_id}`)."
    )
}

/// The leaf directory, when it is a Zephyr application in its own right.
///
/// A standalone leaf keeps `system.toml` beside a `CMakeLists.txt` that calls
/// `find_package(Zephyr)` — every `examples/zephyr/<lang>/<example>/` does, with
/// its `prj.conf` and `boards/` next to it. That file is what makes a directory
/// a west application, so it is the predicate; a workspace bringup's
/// `CMakeLists.txt` (an ament package) never names Zephyr, and a workspace root
/// has no build file at all (RFC-0098 D9).
fn zephyr_leaf_application(
    root: &std::path::Path,
    bringup_dir: &std::path::Path,
) -> Option<PathBuf> {
    if root != bringup_dir {
        return None;
    }
    let cmake = std::fs::read_to_string(bringup_dir.join("CMakeLists.txt")).ok()?;
    cmake
        .lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .any(|l| l.contains("find_package(Zephyr"))
        .then(|| bringup_dir.to_path_buf())
}

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
/// Below the two rungs only this command has (`--zephyr-workspace`, an
/// exported `ZEPHYR_BASE`), the ladder is `scripts/lib/zephyr-workspace.sh`'s
/// (RFC-0095 D4) — the ONE workspace ladder `just` and the fixture builds use:
/// `$NROS_ZEPHYR_WORKSPACE` (wins outright), the store's
/// `$NROS_HOME/workspaces/zephyr/<line>`, then the checkout-relative trees of
/// the selected line (`$NROS_ZEPHYR_VERSION`, default `3.7`). This copy had no
/// store rung and searched both lines at once while claiming to be verbatim,
/// so a host provisioned the way `just zephyr setup` provisions today could
/// not `nros build` a west image (issue 1767 item 7). `NROS_ZEPHYR_WORKSPACE`
/// is the established spelling — an earlier version of this invented
/// `NROS_WEST_WORKSPACE`, which would have been one more name for one thing.
fn zephyr_base(root: &std::path::Path, flag: Option<&std::path::Path>) -> Option<PathBuf> {
    let line = zephyr_line(std::env::var("NROS_ZEPHYR_VERSION").ok());
    zephyr_base_with(
        flag,
        std::env::var_os("ZEPHYR_BASE"),
        std::env::var_os("NROS_ZEPHYR_WORKSPACE"),
        Some(zephyr_store_workspace(
            &crate::orchestration::store::root(),
            &line,
        )),
        &line,
        root,
    )
}

/// The selected Zephyr line — `nros_zephyr_ws_version` in
/// `scripts/lib/zephyr-workspace.sh`: `$NROS_ZEPHYR_VERSION`, else `3.7`.
fn zephyr_line(env: Option<String>) -> String {
    env.filter(|v| !v.is_empty())
        .unwrap_or_else(|| "3.7".to_string())
}

/// `<store>/workspaces/zephyr/<line>` — `nros_zephyr_ws_store_dir`'s answer
/// (RFC-0095 D2/D4: where `just zephyr setup` installs since phase-440 W4).
fn zephyr_store_workspace(store_root: &std::path::Path, line: &str) -> PathBuf {
    store_root.join("workspaces").join("zephyr").join(line)
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
    store_workspace: Option<PathBuf>,
    line: &str,
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
    // From here on, `nros_zephyr_ws_candidates` rung for rung.
    let mut candidates: Vec<PathBuf> = Vec::new();
    match workspace.filter(|w| !w.is_empty()) {
        // The override wins outright and ENDS the ladder — it is also the
        // install target, and a tree it names that is not there yet must not
        // be silently replaced by another one.
        Some(w) => candidates.push(PathBuf::from(w)),
        None => {
            candidates.extend(store_workspace);
            // The checkout-relative trees of THIS line only: a workspace of
            // the other line is a west-manifest mismatch, not a fallback.
            if line == "4.4" {
                if let Some(parent) = root.parent() {
                    candidates.push(parent.join("nano-ros-workspace-4.4"));
                }
            } else {
                candidates.push(root.join("zephyr-workspace"));
                if let Some(parent) = root.parent() {
                    candidates.push(parent.join("nano-ros-workspace"));
                }
            }
        }
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
        // A board with no road is not a framework entry — it is a board that
        // cannot be built at all, and `plan_builds` says so with the reason.
        // Silently excluding it here would strip that message one stage early.
        if let DeployResolution::Board(d) = catalog.resolve_deploy(&deploy)
            && plan::driver_for_board(d.platform.kebab(), d.entry_kind, false).is_ok_and(&want)
        {
            out.insert(pkg.dir.clone());
        }
    }
    out
}

/// `(bringup name, bringup dir, its images)` per bringup — the shape every
/// stage after DISCOVER passes around.
pub(crate) type Bringups = Vec<(String, PathBuf, plan::ImageSet)>;

/// Read every bringup's `[image.*]`.
///
/// Bringups are derived from the packages stage 1 ALREADY found — a bringup is
/// simply a package carrying a `system.toml`. Deliberately not
/// `cmd::bringup::discover_bringups`, which walks one level of the workspace
/// root and so cannot see the canonical `<root>/src/<name>_bringup/` layout;
/// and deliberately not a second walk of our own, which would be a third
/// opinion about what a package is (issue 0809's class).
pub(crate) fn collect_images(
    packages: &[cargo_nano_ros::provider_scan::WorkspacePackage],
) -> Result<Bringups> {
    let mut out = Vec::new();
    for pkg in packages {
        let system_toml = pkg.dir.join("system.toml");
        if !system_toml.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&system_toml)
            .wrap_err_with(|| format!("reading {}", system_toml.display()))?;
        // A retired table (`[deploy.*]`, `[param_services]`) is refused here
        // by the schema itself, naming its replacement (phase-486 W8). The
        // W1.f deprecation lint that used to read `[deploy.*]` build fields at
        // this point went with the table.
        let sys: crate::orchestration::cargo_metadata_schema::SystemToml =
            toml::from_str(&text).wrap_err_with(|| format!("parsing {}", system_toml.display()))?;
        out.push((
            pkg.name.clone(),
            pkg.dir.clone(),
            plan::ImageSet {
                images: effective_images(&sys),
                defaults: sys.image_defaults.clone(),
                default_images: sys.system.default_images.clone(),
            },
        ));
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
    let DependsReport {
        unresolved,
        wrong_role,
        from_snapshot,
        snapshot_ref,
        ..
    } = declared_depends_report(root, packages, nano_ros_root)?;
    report_declared_depends(unresolved, wrong_role, from_snapshot, snapshot_ref)
}

/// What [`declared_depends_report`] found — the ladder's verdict on every
/// `<depend>` a workspace declares, before anything decides what to do with it.
#[derive(Debug, Default)]
pub struct DependsReport {
    /// Names no rung resolved.
    pub unresolved: Vec<crate::orchestration::prereq_resolve::Unresolved>,
    /// Names that resolved to a `[prereq.*]` key whose role is not `package`.
    pub wrong_role: Vec<(
        crate::orchestration::prereq_resolve::Unresolved,
        &'static str,
    )>,
    /// Names only the pinned rosdep snapshot resolved.
    pub from_snapshot: Vec<String>,
    /// The snapshot's short ref, when it was consulted.
    pub snapshot_ref: Option<String>,
    /// Whether the `ros` rung had an ament index to read. Without one the
    /// ladder cannot tell a real ROS package from an unknown name, so an
    /// `unresolved` list from such a host is NOT VERIFIED rather than a verdict
    /// (issue 1509 rule 2, issue 1043's third outcome).
    pub ros_indexed: bool,
}

/// phase-398 W3's ladder over one workspace, as data — the half of
/// [`check_declared_depends`] that decides nothing. Issue 1509 rule 2 asks the
/// same question of every example workspace from the CLI's own tests, and a
/// second spelling of this ladder is what that issue rules out.
pub fn declared_depends_report(
    root: &std::path::Path,
    packages: &[cargo_nano_ros::provider_scan::WorkspacePackage],
    nano_ros_root: Option<&std::path::Path>,
) -> Result<DependsReport> {
    declared_depends_report_with_ament(
        root,
        packages,
        nano_ros_root,
        crate::orchestration::prereq_resolve::ros_packages(),
    )
}

/// [`declared_depends_report`] with the ament rung SUPPLIED rather than read
/// from `AMENT_PREFIX_PATH` — issue 1592.
///
/// An empty set is the ROS-less host, which is what the tier-2 runner is by
/// design. Asking that host's question from a host that HAS ROS is the only
/// way to catch an in-tree `<depend>` that resolves through ament alone: there
/// the ament rung answers it, and on a ROS-less host the report is NOT
/// VERIFIED, so neither reading ever failed on `custom_msgs`'
/// `rosidl_default_generators`. Same ladder, one input lifted to the boundary
/// (issue 1313's shape), never an environment write.
pub fn declared_depends_report_with_ament(
    root: &std::path::Path,
    packages: &[cargo_nano_ros::provider_scan::WorkspacePackage],
    nano_ros_root: Option<&std::path::Path>,
    ros: std::collections::BTreeSet<String>,
) -> Result<DependsReport> {
    use crate::orchestration::prereq_resolve as pr;

    let declared = pr::declared_depends(root);
    if declared.is_empty() {
        return Ok(DependsReport::default());
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
    if let Some(nr) = nano_ros_root {
        // The committed `nros-`prefixed msg crates are reached by their ROS
        // name in a `package.xml`, so undo BOTH halves of the rename: the
        // prefix, and the `-` a crate name uses where an ament name uses `_`.
        //
        // phase-465 W5 — this used to read `packages/interfaces` itself, whose
        // immediate children were the four hyphenated DRIVER packages
        // (`rcl-interfaces`, …) plus `README.md`, and it stripped a prefix none
        // of them carried. So it contributed `rcl-interfaces` where a
        // `<depend>` says `rcl_interfaces`, and this rung resolved nothing, for
        // as long as it existed. The crates live one level down and always did;
        // the collapse to one tree is what makes the path expressible at all.
        for edition in std::fs::read_dir(nr.join("packages/interfaces/generated"))
            .into_iter()
            .flatten()
            .flatten()
        {
            if let Ok(rd) = std::fs::read_dir(edition.path()) {
                generated.extend(rd.flatten().map(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    n.strip_prefix("nros-").unwrap_or(&n).replace('-', "_")
                }));
            }
        }
    }

    // Issue 1304 — the msg packages the toolchain BUNDLES (`std_msgs`,
    // `builtin_interfaces`, …), which codegen generates from the definitions at
    // `packages/cli/interfaces/` with no ROS install in the picture. This rung
    // was missing, and the C++ workspace `nros new` scaffolds declares
    // `<depend>std_msgs</depend>`: on a host WITH ROS the `ros` rung below
    // answered, so the preflight only ever refused where nobody looked — a
    // pristine installed host, which is `just probe installed`.
    generated.extend(cargo_nano_ros::bundled_interface_packages());

    let prereq_map: std::collections::BTreeMap<String, crate::orchestration::sdk_index::PrereqDep> =
        nano_ros_root
            .map(|nr| nr.join("nros-sdk-index.toml"))
            .filter(|p| p.is_file())
            .and_then(|p| crate::orchestration::sdk_index::SdkIndex::load(&p).ok())
            .map(|i| i.prereqs())
            .unwrap_or_default();
    let prereq_keys: std::collections::BTreeSet<String> = prereq_map.keys().cloned().collect();

    // Off-ROS safety: a package's own buildtool is satisfied by the builder that
    // is building it. Without this, adding the `<buildtool_depend>` that rosdep
    // expects would hard-fail every host with no `AMENT_PREFIX_PATH`.
    let mut self_buildtools = pr::self_satisfied_buildtools(root);
    // ...and so is a ROS package nano-ros's own compat layer supplies
    // (`rclcpp`, `rclcpp_components`, `ament_cmake_auto`, …). A workspace's
    // C++ includes `<rclcpp/rclcpp.hpp>` and gets nano-ros's (phase-417), so
    // its `<depend>rclcpp</depend>` is satisfied by this builder exactly as
    // `ament_cmake` is — and asking an ambient ROS refused it on the ROS-less
    // tier-2 runner (the `shadowing` compile-check fixture).
    self_buildtools.extend(pr::nano_ros_provided_packages(nano_ros_root));

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

    Ok(DependsReport {
        unresolved,
        wrong_role,
        from_snapshot,
        snapshot_ref: snapshot.as_ref().map(|s| s.short_ref()),
        ros_indexed: !ros.is_empty(),
    })
}

/// The deciding half of [`check_declared_depends`]: refuse, warn, or pass.
fn report_declared_depends(
    unresolved: Vec<crate::orchestration::prereq_resolve::Unresolved>,
    wrong_role: Vec<(
        crate::orchestration::prereq_resolve::Unresolved,
        &'static str,
    )>,
    from_snapshot: Vec<String>,
    snapshot_ref: Option<String>,
) -> Result<()> {
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
             \nand provision it with `nros setup` run in the workspace, which installs \
             every image's board (`nros setup --check` only reports).\n\
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
    if let (false, Some(snap_ref)) = (from_snapshot.is_empty(), snapshot_ref.as_ref()) {
        eprintln!(
            "nros build: {} <depend> name(s) resolved from the pinned rosdep \
             snapshot (rosdistro {}), {}:\n  {}\n  \
             Declare a `[prereq.*]` key with a `check` probe to make presence \
             diagnosable instead.",
            from_snapshot.len(),
            snap_ref,
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

/// Every `[image.*]` with its RMW RESOLVED, at the parse boundary.
///
/// Issue 1304 (and issue 0938, which believed this was already true). The
/// ladder is `SystemToml::resolved_rmw`: `[image.<id>].rmw` folded over
/// `[image_defaults]`, then `[system].rmw`, then `"zenoh"`. The builder read
/// only the first rung and then defaulted straight to zenoh, so the rest were
/// unreachable from
/// `nros build` — while 0938's own doc said "`nros build` has resolved an
/// image's RMW from `[image.*]` since RFC-0065" and removed the duality for
/// `nros plan` and `nros codegen-system` only.
///
/// What that cost: `nros new --workspace --rmw cyclonedds` writes the choice to
/// `[system] rmw` and NOWHERE else (its `[image.linux]` names a board), so the
/// scaffolded workspace the quick start builds selected the ZENOH backend —
/// `BACKEND zenoh` in the generated cmake root, `rmw = ""` in `resolved.toml`.
/// On a checkout that still compiles, because zenoh-pico is a submodule sitting
/// there; on an installed toolchain the build dies in `zpico-sys` with "zenoh-pico
/// source not provisioned", which is how `just probe installed` surfaced it.
///
/// Resolved HERE so there is one call and every reader downstream — the cmake
/// spec's `BACKEND`, `ResolvedBuild::rmw`, the `ImageIdent` that names the build
/// directory — sees the same answer.
fn effective_images(
    sys: &crate::orchestration::cargo_metadata_schema::SystemToml,
) -> std::collections::BTreeMap<String, crate::orchestration::image::ImageBlock> {
    sys.image
        .iter()
        .map(|(id, img)| {
            let mut img = img.clone();
            img.rmw = Some(sys.resolved_rmw(Some(id), None));
            (id.clone(), img)
        })
        .collect()
}

/// Issue 1419 -- does `nros build` stage 4 check this image's census?
///
/// Exactly the images no CONFIGURE checks: a CROSS image whose road calls no
/// `nano_ros_entry()`. That is every cargo-driver image (a cargo image never
/// configures) and a west image whose node graph is all Rust
/// (`rust_cargo_application()`). A C/C++ west or cmake image is checked by its
/// own configure (`cmake/NanoRosEntry.cmake`), so checking it here too would
/// be a second reader of one verdict; a HOST image is the census PRODUCER.
pub(crate) fn stage4_checks_census(
    driver: Driver,
    platform: crate::orchestration::board_descriptor::PlatformKind,
    has_non_rust: bool,
) -> bool {
    if platform == crate::orchestration::board_descriptor::PlatformKind::Posix {
        return false;
    }
    match driver {
        Driver::Cargo => true,
        Driver::West => !has_non_rust,
        Driver::CMake => false,
    }
}

#[cfg(test)]
mod effective_images_tests {
    use super::effective_images;

    /// `[system]` requires `name`, `rmw` and `domain_id`, so a fixture cannot
    /// omit `rmw` to mean "undeclared" — it carries the EMPTY string, which is
    /// what `resolved_rmw` reads as undeclared and what a workspace that never
    /// chose one actually holds.
    fn sys(
        header_rmw: &str,
        image_extra: &str,
    ) -> crate::orchestration::cargo_metadata_schema::SystemToml {
        let text = format!(
            "[system]\nname = \"demo\"\nrmw = \"{header_rmw}\"\ndomain_id = 0\n\n\
             [image.linux]\nboard = \"linux\"\n{image_extra}"
        );
        toml::from_str(&text).unwrap_or_else(|e| panic!("parse: {e}\n{text}"))
    }

    /// The shape `nros new --workspace --rmw cyclonedds` writes: the choice
    /// lives in `[system]`, the image names only a board. This is the case that
    /// built the wrong backend — `BACKEND zenoh` for a cyclonedds workspace.
    #[test]
    fn an_image_with_no_rmw_inherits_the_system_header() {
        let got = effective_images(&sys("cyclonedds", ""));
        assert_eq!(got["linux"].rmw.as_deref(), Some("cyclonedds"));
    }

    /// An image's own `rmw` still wins over the header.
    #[test]
    fn an_images_own_rmw_beats_the_header() {
        let got = effective_images(&sys("cyclonedds", "rmw = \"xrce\"\n"));
        assert_eq!(got["linux"].rmw.as_deref(), Some("xrce"));
    }

    /// Nothing declared anywhere resolves to the tree's default, not to an
    /// empty string — `resolved.toml` carried `rmw = ""` before this.
    #[test]
    fn nothing_declared_resolves_to_the_default() {
        let got = effective_images(&sys("", ""));
        assert_eq!(got["linux"].rmw.as_deref(), Some("zenoh"));
    }
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

    /// The deprecated typed `[safety]` block still counts — both spellings
    /// flip the axis. (`[param_services]` was the other typed block; it is
    /// refused since phase-486 W8, so it cannot reach this predicate.)
    #[test]
    fn typed_block_is_seen() {
        let d = bringup_with(&format!("{BASE}\n[safety]\nenabled = true\n"));
        assert_eq!(declared_capabilities(d.path()), vec!["safety"]);
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

    /// Issue 1962 — a facade an OLDER generator wrote is rewritten, not
    /// reused. The bridge workspaces kept a 2026-10-15 facade with no
    /// registration macros, and `nros::main!` could not find them.
    #[test]
    fn a_stale_facade_is_rewritten_by_the_current_generator() {
        let ws = tempfile::tempdir().unwrap();
        let root = ws.path();
        let nros = root.join("nros");
        std::fs::create_dir_all(&nros).unwrap();
        std::fs::write(
            nros.join("Cargo.toml"),
            "[package]\nname = \"nros\"\n\n[features]\nros-humble = []\n",
        )
        .unwrap();
        let bringup = root.join("src/demo_bringup");
        std::fs::create_dir_all(&bringup).unwrap();
        std::fs::write(
            bringup.join("system.toml"),
            "[system]\nname = \"demo\"\nrmw = \"zenoh\"\ndomain_id = 0\n\n\
             [image.linux]\nboard = \"linux\"\n",
        )
        .unwrap();
        let entry = root.join("build/posix-zenoh/linux_entry");
        std::fs::create_dir_all(&entry).unwrap();
        std::fs::write(
            entry.join("Cargo.toml"),
            "[package]\nname = \"linux_entry\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nnros = { path = \"../../../nros\" }\n",
        )
        .unwrap();
        let facades = facade_root(root);
        let stale = facades.join("linux_entry/src/lib.rs");
        std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
        std::fs::write(
            &stale,
            "//! This crate is intentionally empty.\n#![no_std]\n",
        )
        .unwrap();

        let f = write_entry_facade("linux_entry", &entry, &bringup, &facades)
            .unwrap()
            .expect("an image claims the entry: a facade");
        assert!(f.changed, "the stale facade must be rewritten");
        let lib = std::fs::read_to_string(&stale).unwrap();
        assert!(
            lib.contains("macro_rules! __nros_register_backends")
                && lib.contains("macro_rules! __nros_register_type"),
            "the rewritten facade carries the registration macros:\n{lib}"
        );
    }
}

#[cfg(test)]
mod retired_table_tests {
    use super::*;

    fn pkg(dir: &std::path::Path) -> Vec<cargo_nano_ros::provider_scan::WorkspacePackage> {
        vec![cargo_nano_ros::provider_scan::WorkspacePackage {
            name: "demo_bringup".to_string(),
            dir: dir.to_path_buf(),
            build_type: None,
            depends: Default::default(),
        }]
    }

    /// phase-486 W8 — the build path REFUSES a retired table, naming the file
    /// and the replacement. (It used to warn about `[deploy.*]` build fields
    /// here, W1.f's lint; a warning was right while the table still worked.)
    #[test]
    fn the_build_path_refuses_a_retired_table_naming_the_replacement() {
        for (body, names) in [
            (
                "[deploy.legacy]\nkind = \"embedded\"\nboard = \"mps2-an385-freertos\"\n",
                "[image.<id>]",
            ),
            ("[param_services]\n", "features = [\"param_services\"]"),
        ] {
            let d = tempfile::tempdir().expect("tempdir");
            std::fs::write(
                d.path().join("system.toml"),
                format!("[system]\nname = \"s\"\nrmw = \"zenoh\"\ndomain_id = 0\n\n{body}"),
            )
            .expect("write");
            let e = format!("{:#}", collect_images(&pkg(d.path())).expect_err("refused"));
            assert!(e.contains("is retired") && e.contains(names), "{e}");
            assert!(e.contains("system.toml"), "names the file: {e}");
        }
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
        assert_eq!(zephyr_base_with(None, None, None, None, "3.7", &root), None);

        // In-repo `zephyr-workspace/` — the common contributor layout.
        let inrepo = zdir(&root, "zephyr-workspace");
        assert_eq!(
            zephyr_base_with(None, None, None, None, "3.7", &root),
            Some(inrepo.join("zephyr"))
        );

        // `NROS_ZEPHYR_WORKSPACE` — the ESTABLISHED spelling (60 references in
        // the tree), not a new one — outranks the in-repo default.
        let explicit = zdir(&base, "elsewhere");
        assert_eq!(
            zephyr_base_with(
                None,
                None,
                Some(explicit.clone().into_os_string()),
                None,
                "3.7",
                &root
            ),
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
                None,
                "3.7",
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
            zephyr_base_with(
                None,
                None,
                Some(empty.clone().into_os_string()),
                None,
                "3.7",
                &root
            ),
            None,
            "a workspace with no zephyr/ is not a workspace",
        );
        assert_eq!(
            zephyr_base_with(
                None,
                Some(empty.join("nope").into_os_string()),
                None,
                None,
                "3.7",
                &root
            ),
            None,
            "a ZEPHYR_BASE that does not exist is not a Zephyr",
        );
        std::fs::remove_dir_all(&base).ok();
    }

    /// Issue 1767 item 7 — the rungs below the command's own are
    /// `scripts/lib/zephyr-workspace.sh`'s: the store outranks the checkout
    /// trees, only the selected line's checkout trees are candidates, and a
    /// set `NROS_ZEPHYR_WORKSPACE` ends the ladder.
    #[test]
    fn the_workspace_ladder_is_the_shell_ladder() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("nano-ros");
        std::fs::create_dir_all(&root).unwrap();
        let store = tmp.path().join("home");
        let store_37 = zephyr_store_workspace(&store, "3.7");
        assert_eq!(store_37, store.join("workspaces/zephyr/3.7"));

        // A sibling 4.4 tree is NOT a 3.7 answer (a west-manifest mismatch).
        let sib_44 = zdir(tmp.path(), "nano-ros-workspace-4.4");
        assert_eq!(
            zephyr_base_with(None, None, None, Some(store_37.clone()), "3.7", &root),
            None
        );
        assert_eq!(
            zephyr_base_with(None, None, None, None, "4.4", &root),
            Some(sib_44.join("zephyr"))
        );

        // The store outranks the in-repo tree.
        let inrepo = zdir(&root, "zephyr-workspace");
        assert_eq!(
            zephyr_base_with(None, None, None, Some(store_37.clone()), "3.7", &root),
            Some(inrepo.join("zephyr")),
            "an unpopulated store contributes nothing"
        );
        std::fs::create_dir_all(store_37.join("zephyr")).unwrap();
        assert_eq!(
            zephyr_base_with(None, None, None, Some(store_37.clone()), "3.7", &root),
            Some(store_37.join("zephyr"))
        );

        // A set override ends the ladder, even over a populated store.
        let missing = tmp.path().join("not-yet-installed");
        assert_eq!(
            zephyr_base_with(
                None,
                None,
                Some(missing.into_os_string()),
                Some(store_37),
                "3.7",
                &root
            ),
            None
        );

        assert_eq!(zephyr_line(None), "3.7");
        assert_eq!(zephyr_line(Some(String::new())), "3.7");
        assert_eq!(zephyr_line(Some("4.4".into())), "4.4");
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
    /// `examples/workspaces/cpp/src/zephyr_entry/CMakeLists.txt` carried the
    /// comment "nano_ros_node_register has no DEPLOY → component-only", one
    /// line above the real call. (phase-470 W5.b3 deleted that package — the
    /// application is generated now, and the generated file carries the same
    /// comment, so the hazard is unchanged and the case is kept.) A file-wide
    /// regex reads the comment as the answer;
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
                None,
                "3.7",
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
            zephyr_base_with(Some(&zephyr), None, None, None, "3.7", tmp.path()),
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
        assert_eq!(
            zephyr_base_with(Some(&empty), None, None, None, "3.7", tmp.path()),
            None
        );
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
            board: None,
            all: false,
            dry_run: true,
            verbose: false,
            offline: true,
            packages_select: select.iter().map(|s| (*s).to_string()).collect(),
            packages_up_to: up_to.iter().map(|s| (*s).to_string()).collect(),
            check: None,
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

        let b = Args::try_parse_from(["build", "linux", "--packages-select", "talker_pkg"])
            .expect("parses");
        assert_eq!(b.images, vec!["linux"]);
        assert_eq!(b.packages_select, vec!["talker_pkg"]);
    }

    /// `--check` takes its tool only with `=`, so a bare `--check` never
    /// swallows the image name after it.
    #[test]
    fn check_parses_bare_and_with_a_tool() {
        let a = Args::try_parse_from(["build", "--check", "linux"]).expect("parses");
        assert_eq!(a.check, Some(CheckTool::Check));
        assert_eq!(a.images, vec!["linux"]);

        let b = Args::try_parse_from([
            "build",
            "linux",
            "--check=clippy",
            "--",
            "--",
            "-D",
            "warnings",
        ])
        .expect("parses");
        assert_eq!(b.check, Some(CheckTool::Clippy));
        assert_eq!(b.native_args, vec!["--", "-D", "warnings"]);

        let c = Args::try_parse_from(["build", "linux"]).expect("parses");
        assert_eq!(c.check, None);
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
            board: "linux".to_string(),
            platform: "posix".to_string(),
            driver: Driver::Cargo,
            handoff: Some(hand),
            rmw: None,
            entry_package: None,
            target: None,
            profile: None,
            cmake_build_dir: None,
            capabilities: Vec::new(),
            configure: None,
            record: Default::default(),
            output: None,
            descriptor: None,
        }
    }

    fn nullary(program: &str) -> Handoff {
        Handoff::new(program, Vec::<String>::new())
    }

    /// The bug, stated as the invariant it violated: EVERY plan reaches a
    /// handover.
    ///
    /// Before issue 1206's fix this recorded ONE entry — the loop called
    /// `handoff::exec` on every iteration, and the first one never returned,
    /// so plans 2..N were unreachable by construction. Since phase-485 W3 no
    /// plan execs (the project lock is recorded after the build), so this
    /// holds for the last plan too.
    #[test]
    fn every_plan_reaches_a_handover() {
        let plans = vec![
            plan("one", nullary("true")),
            plan("two", nullary("true")),
            plan("three", nullary("true")),
        ];
        let mut seen: Vec<String> = Vec::new();
        drive(&plans, false, &mut |h| {
            seen.push(h.display());
            Ok(())
        })
        .expect("three plans, three handovers");
        assert_eq!(
            seen.len(),
            3,
            "one handover per plan, not one per invocation: {seen:?}"
        );
    }

    /// `--dry-run` prints and hands over to nothing — the property that keeps
    /// it a print of the real plan rather than a second code path.
    #[test]
    fn a_dry_run_performs_no_handover() {
        let plans = vec![plan("one", nullary("true")), plan("two", nullary("true"))];
        let mut count = 0usize;
        drive(&plans, true, &mut |_| {
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
        let e = drive(&plans, false, &mut |_| {
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
        perform(&Handoff::new("touch", [marker.clone().into_os_string()])).expect("touch runs");
        assert!(marker.is_file(), "the command ran and had its effect");
    }

    /// And a non-zero exit is a failure, not a silent success — and it carries
    /// the CHILD's status, which `nros build` exits with (what `exec` used to
    /// give for free).
    #[test]
    fn a_waited_handover_fails_with_the_childs_exit_code() {
        let e = perform(&Handoff::new("sh", ["-c", "exit 7"])).expect_err("exits 7");
        let c = e.downcast_ref::<ChildExit>().expect("carries a ChildExit");
        assert_eq!(c.code, 7, "{e:#}");
        assert!(format!("{e:#}").contains("exited 7"), "{e:#}");
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
    /// what resolves `board = "linux"`.
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
            "[package]\nname = \"demo_talker\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [dependencies]\nnros-board-linux = { version = \"*\" }\n\n[workspace]\n",
        )
        .unwrap();
        std::fs::write(leaf.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(
            leaf.join("system.toml"),
            "[system]\nname = \"demo\"\nrmw = \"zenoh\"\ndomain_id = 0\n\n\
             [image.linux]\nboard = \"linux\"\n",
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
            board: None,
            all: false,
            dry_run: true,
            verbose: false,
            offline: false,
            packages_select: Vec::new(),
            packages_up_to: Vec::new(),
            check: None,
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
        assert_eq!(p.qualified, "demo_talker:linux");
        let shown = p.handoff.as_ref().unwrap().display();
        assert!(
            shown.contains(
                "cargo build --manifest-path talker/Cargo.toml --config \
                 talker/build/linux/nros-cargo.toml"
            ),
            "{shown}"
        );
        // The leaf's `[workspace]` marker is its own root, not a root build
        // file to retire (RFC-0098 D9 is about workspaces).
        assert!(leaf.join("Cargo.toml").is_file());

        let settings = leaf.join("build/linux/nros-cargo.toml");
        let v: toml::Value = std::fs::read_to_string(&settings)
            .unwrap()
            .parse()
            .expect("the settings file is TOML");
        assert_eq!(v["build"]["target-dir"].as_str(), Some("linux/target"));
        assert_eq!(v["env"]["NROS_BOARD"].as_str(), Some("linux"));
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
        assert!(e.contains("`linux`"), "{e}");
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

    /// Issues 1296/1308 — the C/C++ shape of a single-package leaf, with the
    /// `[[component]]` row the converted examples carry. It used to take the
    /// workspace road, which generated a root for a workspace of one and asked
    /// `nros plan` for `<[system] name>/launch/system.launch.xml`, a path
    /// nothing writes ("no SystemModel for `native_c_talker/launch/…`").
    fn c_leaf(parent: &std::path::Path) -> PathBuf {
        let leaf = parent.join("c_talker");
        std::fs::create_dir_all(leaf.join("src")).unwrap();
        std::fs::write(
            leaf.join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.22)\nproject(native_c_talker C)\n",
        )
        .unwrap();
        std::fs::write(
            leaf.join("package.xml"),
            "<package format=\"3\"><name>native_c_talker</name></package>\n",
        )
        .unwrap();
        std::fs::write(
            leaf.join("system.toml"),
            "[system]\nname = \"native_c_talker\"\nrmw = \"zenoh\"\ndomain_id = 0\n\n\
             [[component]]\npkg = \"native_c_talker\"\nname = \"talker\"\n\n\
             [image.linux]\nboard = \"linux\"\n",
        )
        .unwrap();
        leaf
    }

    #[test]
    fn a_cmake_leaf_configures_its_own_cmakelists_for_its_image() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = c_leaf(tmp.path());
        let plans = plan_builds(&args(&leaf, &[])).expect("a single-package C leaf plans");
        assert_eq!(plans.len(), 1);
        let p = &plans[0];
        assert_eq!(p.driver, Driver::CMake);
        assert_eq!(p.qualified, "native_c_talker:linux");
        assert_eq!(p.cmake_build_dir.as_deref(), Some("build/linux/cmake"));
        let cfg = p.configure.as_ref().expect("a configure").display();
        assert!(
            cfg.contains("cmake -S . -B build/linux/cmake -DNROS_IMAGE=linux"),
            "the leaf's OWN CMakeLists, for the image it declares: {cfg}"
        );
        assert!(
            !cfg.contains("CMAKE_TOOLCHAIN_FILE"),
            "a host board needs none: {cfg}"
        );
        let build = p.handoff.as_ref().unwrap().display();
        assert!(build.contains("cmake --build build/linux/cmake"), "{build}");
        // Nothing generated a workspace root beside the leaf.
        assert!(!leaf.join("build/posix-zenoh-linux").exists());
    }

    #[test]
    fn a_cmake_leaf_refuses_an_image_it_does_not_declare() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = c_leaf(tmp.path());
        let e = plan_builds(&args(&leaf, &["esp32"]))
            .expect_err("wrong image")
            .to_string();
        assert!(e.contains("[image.esp32]") && e.contains("linux"), "{e}");
    }

    /// `--check` keeps every stage a build runs and changes only cargo's
    /// verb: the same manifest, the same generated settings, the same cwd.
    #[test]
    fn check_swaps_only_the_cargo_verb_of_a_leaf_build() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = leaf(tmp.path());
        let built = plan_builds(&args(&leaf, &[])).expect("plans");
        let built = built[0].handoff.as_ref().unwrap().clone();
        for (tool, verb) in [(CheckTool::Check, "check"), (CheckTool::Clippy, "clippy")] {
            let mut plans = plan_builds(&args(&leaf, &[])).expect("plans");
            retarget_to_check(&mut plans, tool).expect("a cargo image checks");
            let h = plans[0].handoff.as_ref().unwrap();
            assert_eq!(h.args[0], verb);
            assert_eq!(h.args[1..], built.args[1..], "only the verb moves");
            assert_eq!(h.cwd, built.cwd);
            assert!(
                h.display().contains(&format!(
                    "cargo {verb} --manifest-path talker/Cargo.toml --config \
                     talker/build/linux/nros-cargo.toml"
                )),
                "{}",
                h.display()
            );
        }
    }

    #[test]
    fn check_refuses_an_image_cargo_does_not_drive() {
        let tmp = tempfile::tempdir().unwrap();
        let leaf = c_leaf(tmp.path());
        let mut plans = plan_builds(&args(&leaf, &[])).expect("plans");
        let e = retarget_to_check(&mut plans, CheckTool::Check)
            .expect_err("cmake has no check")
            .to_string();
        assert!(
            e.contains("native_c_talker:linux") && e.contains("cmake"),
            "names the image and its driver: {e}"
        );
    }
}

#[cfg(test)]
mod generated_output_collision_tests {
    //! Issue 1582 — two images that GENERATE into one path.
    use super::*;
    use crate::orchestration::image::ImageBlock;

    fn img(board: &str) -> ImageBlock {
        ImageBlock {
            board: Some(board.to_string()),
            rmw: Some("zenoh".to_string()),
            ..Default::default()
        }
    }

    fn claim(
        root: &Path,
        qual: &str,
        image: &ImageBlock,
        platform: &str,
        driver: Driver,
    ) -> (String, Vec<PathBuf>) {
        let id = qual.split_once(':').expect("qualified").1;
        (
            qual.to_string(),
            generated_outputs(root, id, image, platform, driver, root),
        )
    }

    /// The reported shape: `realtime-rust`'s `demo_bringup` and
    /// `derived_bringup` both declared `[image.zephyr]` with no hand-written
    /// application, so both west leaves were configured on ONE generated
    /// directory, and the authored-tier image booted `derived-telem_node`.
    #[test]
    fn two_bringups_generating_one_id_collide_on_the_entry_and_the_facade() {
        let t = tempfile::tempdir().unwrap();
        let z = img("zephyr");
        let claims = vec![
            claim(t.path(), "demo_bringup:zephyr", &z, "zephyr", Driver::West),
            claim(
                t.path(),
                "derived_bringup:zephyr",
                &z,
                "zephyr",
                Driver::West,
            ),
        ];
        let got = generated_output_collisions(&claims);
        let paths: Vec<String> = got.iter().map(|(p, _)| p.display().to_string()).collect();
        assert_eq!(
            paths,
            vec![
                "build/zephyr-zenoh/zephyr_entry".to_string(),
                "generated/nros-selection/zephyr_entry".to_string(),
            ],
            "{got:?}"
        );
        for (_, q) in &got {
            assert_eq!(q, &["demo_bringup:zephyr", "derived_bringup:zephyr"]);
        }
        let msg = generated_output_collision_message(&got).expect("a refusal");
        assert!(msg.contains("issue 1582"), "{msg}");
        assert!(msg.contains("derived_bringup:zephyr"), "{msg}");
    }

    /// The fix: a distinct id is a distinct directory.
    #[test]
    fn distinct_ids_do_not_collide() {
        let t = tempfile::tempdir().unwrap();
        let z = img("zephyr");
        let claims = vec![
            claim(t.path(), "demo_bringup:zephyr", &z, "zephyr", Driver::West),
            claim(
                t.path(),
                "derived_bringup:zephyr_derived",
                &z,
                "zephyr",
                Driver::West,
            ),
        ];
        assert!(generated_output_collisions(&claims).is_empty());
    }

    /// The claimed entry dir is the one the build WRITES, not a parallel
    /// spelling of it: both come from `generated_entry_dir`.
    #[test]
    fn the_claimed_entry_dir_is_the_generated_one() {
        let t = tempfile::tempdir().unwrap();
        let z = img("zephyr");
        let outs = generated_outputs(t.path(), "zephyr", &z, "zephyr", Driver::West, t.path());
        assert_eq!(
            t.path().join(&outs[0]),
            generated_entry_dir(t.path(), "zephyr", &z, "zephyr")
        );
    }

    /// `realtime-c`'s shape, which is legal: ONE hand-written application
    /// serving two bringups. It generates nothing, so it claims nothing — each
    /// leaf has its own west build dir and the app picks the bringup itself.
    #[test]
    fn a_hand_written_application_claims_nothing() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("src/zephyr_entry")).unwrap();
        std::fs::write(t.path().join("src/zephyr_entry/CMakeLists.txt"), "").unwrap();
        let z = img("zephyr");
        for d in [Driver::West, Driver::Cargo, Driver::CMake] {
            assert!(
                generated_outputs(t.path(), "zephyr", &z, "zephyr", d, t.path()).is_empty(),
                "{d:?}"
            );
        }
    }

    /// Issue 1288 — the EXPLICIT-ENTRY rung outranks the `<id>_entry` rung.
    ///
    /// `[image.zephyr_robot1] entry = "zephyr_entry_robot1"` was the only
    /// evidence for this precedence, and it was a whole hand-written package.
    /// Here both candidates exist side by side — a package named exactly
    /// `<id>_entry` AND a differently named application the image names — and
    /// the named one must win on both halves of the rule: it is the west
    /// application, and generation (which would otherwise produce or find
    /// `<id>_entry`) does not run.
    #[test]
    fn an_explicit_entry_outranks_the_id_entry_package() {
        let t = tempfile::tempdir().unwrap();
        for name in ["zephyr_robot1_entry", "robot1_app"] {
            let dir = t.path().join("src").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("package.xml"),
                format!(
                    "<?xml version=\"1.0\"?>\n<package format=\"3\">\n  <name>{name}</name>\n  \
                     <version>0.0.0</version>\n  <description>t</description>\n  \
                     <maintainer email=\"a@b.c\">m</maintainer>\n  \
                     <license>Apache-2.0</license>\n</package>\n"
                ),
            )
            .unwrap();
            std::fs::write(dir.join("CMakeLists.txt"), "").unwrap();
        }
        let found = crate::builder::discover::discover(t.path(), &[]).expect("discover");
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap();
        let catalog =
            crate::orchestration::board_descriptor::BoardCatalog::load(&repo).expect("catalog");
        let crate::orchestration::board_descriptor::DeployResolution::Board(descriptor) =
            catalog.resolve_deploy("zephyr")
        else {
            panic!("the `zephyr` board resolves");
        };

        let mut image = img("zephyr");
        image.entry = Some("robot1_app".to_string());
        let app = west_application_dir("zephyr_robot1", &image, descriptor, &found, &catalog, &[])
            .expect("resolves");
        assert_eq!(
            app.as_deref(),
            Some(t.path().join("src/robot1_app").as_path()),
            "the named entry is the application, not `zephyr_robot1_entry`"
        );
        assert!(
            generated_outputs(
                t.path(),
                "zephyr",
                &image,
                "zephyr_robot1",
                Driver::West,
                t.path()
            )
            .is_empty(),
            "a named entry suppresses generation, so `<id>_entry` cannot win that way either"
        );

        // And the rung is not vacuous: without `entry`, the image is a
        // generation candidate again.
        image.entry = None;
        assert!(
            !generated_outputs(
                t.path(),
                "zephyr",
                &image,
                "zephyr_robot1",
                Driver::Cargo,
                t.path()
            )
            .is_empty()
        );
    }

    /// On the west road `[image.<id>] entry` names the application and
    /// generation never runs; on the cargo road it is not consulted, so the
    /// entry is still generated and still claimed.
    #[test]
    fn a_named_entry_suppresses_generation_on_the_west_road_only() {
        let t = tempfile::tempdir().unwrap();
        let mut z = img("zephyr");
        z.entry = Some("app".to_string());
        assert!(
            generated_outputs(t.path(), "zephyr", &z, "zephyr", Driver::West, t.path()).is_empty()
        );
        assert!(
            !generated_outputs(t.path(), "zephyr", &z, "zephyr", Driver::Cargo, t.path())
                .is_empty()
        );
    }

    /// cmake: one id on one coordinate is one target in one root — the pair
    /// `realtime-c`'s `smp_bringup` used to repeat — while the same id on two
    /// DIFFERENT boards lands in two roots and is fine.
    #[test]
    fn a_cmake_id_collides_only_on_one_coordinate() {
        let t = tempfile::tempdir().unwrap();
        let native = img("native");
        let same = vec![
            claim(
                t.path(),
                "demo_bringup:linux",
                &native,
                "posix",
                Driver::CMake,
            ),
            claim(
                t.path(),
                "smp_bringup:linux",
                &native,
                "posix",
                Driver::CMake,
            ),
        ];
        assert_eq!(generated_output_collisions(&same).len(), 1);

        let other = img("mps2-an385-freertos");
        let apart = vec![
            claim(t.path(), "a:x", &native, "posix", Driver::CMake),
            claim(t.path(), "b:x", &other, "freertos", Driver::CMake),
        ];
        assert!(generated_output_collisions(&apart).is_empty());
    }
}

#[cfg(test)]
mod board_heap_tests {
    use super::*;

    /// A nano-ros root holding one board that states `heap_bytes`, loaded the
    /// way `plan_builds` loads it -- so `source` is ROOT-relative.
    fn root_with_board(heap: Option<usize>) -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("packages/boards/nros-board-heapy");
        std::fs::create_dir_all(&dir).unwrap();
        let mut text = String::from(
            "[[board]]\nnames = [\"heapy\"]\nplatform = \"freertos\"\ntoolchain = \"stable\"\n\
             entry_kind = \"hosted-main\"\nsupported_netstacks = []\n",
        );
        if let Some(h) = heap {
            text.push_str(&format!("[knobs.memory]\nheap_bytes = {h}\n"));
        }
        std::fs::write(dir.join("nros-board.toml"), text).unwrap();
        std::fs::write(
            dir.join("package.xml"),
            "<package format=\"3\"><name>nros_board_heapy</name><export>\
             <nano_ros_provides kind=\"board\" name=\"heapy\"/></export></package>\n",
        )
        .unwrap();
        t
    }

    fn descriptor(
        root: &std::path::Path,
    ) -> crate::orchestration::board_descriptor::BoardDescriptor {
        let cat = crate::orchestration::board_descriptor::BoardCatalog::load_with_extra(root, &[])
            .unwrap();
        let d = cat
            .descriptors()
            .iter()
            .find(|d| d.names.iter().any(|n| n == "heapy"))
            .expect("the test board")
            .clone();
        assert!(
            !std::path::Path::new(d.source.as_deref().unwrap()).is_absolute(),
            "the premise: an in-tree source is root-relative"
        );
        d
    }

    /// Issue 1653 -- the heap reaches the configure line from ANY cwd. The
    /// relative `source` used to be opened against the cwd, so from anywhere
    /// but the checkout root the board's heap read as unstated. The test's cwd
    /// is the crate dir, never the temp root, so the old reader fails here.
    #[test]
    fn the_board_heap_resolves_against_the_nano_ros_root_not_the_cwd() {
        let t = root_with_board(Some(65536));
        let d = descriptor(t.path());
        assert_eq!(
            board_heap_configure_arg(&d, "heapy", Some(t.path())),
            "-DNROS_BOARD_HEAP_BUDGET_BYTES=65536"
        );
        assert_eq!(board_heap_budget(&d, "heapy", Some(t.path())), Some(65536));
    }

    /// Negative control: a board that states no heap hands the configure an
    /// EMPTY value -- "nobody said" stays refused (RFC-0100 D6), never a
    /// default -- and an empty value rather than no argument, because a `-D`
    /// is a cache entry and its absence would keep the last build's heap. With
    /// no root to resolve a relative source against, empty too: never a read
    /// relative to whatever the cwd happens to be.
    #[test]
    fn a_board_with_no_heap_clears_the_configure_value() {
        let t = root_with_board(None);
        let d = descriptor(t.path());
        assert_eq!(
            board_heap_configure_arg(&d, "heapy", Some(t.path())),
            "-DNROS_BOARD_HEAP_BUDGET_BYTES="
        );
        let t2 = root_with_board(Some(4096));
        let d2 = descriptor(t2.path());
        assert_eq!(
            board_heap_configure_arg(&d2, "heapy", None),
            "-DNROS_BOARD_HEAP_BUDGET_BYTES="
        );
    }

    /// phase-462 W6c -- the board's `[board.rx]` reaches the configure line as
    /// two `-D`s; a board that states none clears BOTH (a `-D` is a cache
    /// entry, so an omitted one keeps the last build's cost).
    #[test]
    fn the_board_rx_cost_reaches_the_configure_line_and_clears_when_unstated() {
        let t = root_with_board(None);
        let mut d = descriptor(t.path());
        assert_eq!(
            board_rx_configure_args(&d),
            [
                "-DNROS_BOARD_RX_FRAME_COST_US=".to_string(),
                "-DNROS_BOARD_RX_FLOOR_US=".to_string(),
            ]
        );
        d.rx = Some(crate::entity_inventory::RxFrameCost {
            frame_cost_us: 940,
            floor_us: None,
        });
        assert_eq!(
            board_rx_configure_args(&d),
            [
                "-DNROS_BOARD_RX_FRAME_COST_US=940".to_string(),
                "-DNROS_BOARD_RX_FLOOR_US=".to_string(),
            ]
        );
        d.rx = Some(crate::entity_inventory::RxFrameCost {
            frame_cost_us: 940,
            floor_us: Some(11_000),
        });
        assert_eq!(
            board_rx_configure_args(&d)[1],
            "-DNROS_BOARD_RX_FLOOR_US=11000"
        );
    }

    /// Issue 2072 -- a board image is configured `Release`, as every fixture
    /// is; a posix image is a host process and keeps cmake's default.
    #[test]
    fn a_board_image_configures_release_and_a_host_one_does_not() {
        for platform in ["freertos", "zephyr", "threadx", "nuttx"] {
            assert_eq!(
                board_build_type_arg(platform).as_deref(),
                Some("-DCMAKE_BUILD_TYPE=Release"),
                "{platform}"
            );
        }
        assert_eq!(board_build_type_arg("posix"), None);
    }
}

#[cfg(test)]
mod stage4_census_tests {
    use super::{Driver, stage4_checks_census};
    use crate::orchestration::board_descriptor::PlatformKind;

    /// Issue 1419 -- stage 4 checks the census of exactly the cross images no
    /// configure checks: every cargo image, and a west image whose graph is all
    /// Rust (`rust_cargo_application()` calls no `nano_ros_entry()`). Before,
    /// the west road checked nothing, so `realtime-rust`'s `zephyr_derived` --
    /// the one contracted Rust Zephyr image -- was never compared with its code.
    #[test]
    fn stage4_checks_the_census_no_configure_checks() {
        // The Rust Zephyr image: checked here (was: never).
        assert!(stage4_checks_census(
            Driver::West,
            PlatformKind::Zephyr,
            false
        ));
        // A C/C++ Zephyr image: its configure (`nano_ros_entry`) checks it.
        assert!(!stage4_checks_census(
            Driver::West,
            PlatformKind::Zephyr,
            true
        ));
        // Every cross cargo image, as before.
        assert!(stage4_checks_census(
            Driver::Cargo,
            PlatformKind::ThreadxLinux,
            false
        ));
        // cmake images are their configure's; host images are the producer.
        assert!(!stage4_checks_census(
            Driver::CMake,
            PlatformKind::Freertos,
            true
        ));
        assert!(!stage4_checks_census(
            Driver::Cargo,
            PlatformKind::Posix,
            false
        ));
    }
}

/// phase-481 W4 (RFC-0098 D12) — per-image cmake configures.
#[cfg(test)]
mod per_image_configure_tests {
    use super::*;
    use crate::orchestration::image::ImageBlock;

    fn image(env: &[(&str, &str)], locator: Option<&str>) -> ImageBlock {
        ImageBlock {
            board: Some("native".to_string()),
            rmw: Some("zenoh".to_string()),
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            locator: locator.map(str::to_string),
            ..Default::default()
        }
    }

    /// An image that states nothing keeps EXACTLY the path it had, which is
    /// what keeps the book's paths and every existing fixture row valid.
    #[test]
    fn an_image_stating_nothing_keeps_its_coordinate() {
        assert_eq!(
            cmake_coordinate("native", &image(&[], None), Path::new("/nonexistent")),
            "native-zenoh"
        );
    }

    /// Two images whose resolved configuration differs cannot share a
    /// configure; two whose configuration is identical do — however the rows
    /// were spelled in `system.toml` (a `BTreeMap` has no insertion order).
    #[test]
    fn configuration_decides_which_configure_an_image_shares() {
        let warn = cmake_coordinate(
            "native",
            &image(&[("APP_LOG_LEVEL", "warn")], None),
            Path::new("/nonexistent"),
        );
        let debug = cmake_coordinate(
            "native",
            &image(&[("APP_LOG_LEVEL", "debug")], None),
            Path::new("/nonexistent"),
        );
        assert!(warn.starts_with("native-zenoh-cfg"), "{warn}");
        assert_ne!(warn, debug);
        let ab = cmake_coordinate(
            "native",
            &image(&[("A", "1"), ("B", "2")], None),
            Path::new("/nonexistent"),
        );
        let ba = cmake_coordinate(
            "native",
            &image(&[("B", "2"), ("A", "1")], None),
            Path::new("/nonexistent"),
        );
        assert_eq!(ab, ba);
    }

    /// phase-441 W6 — a bringup's own RMW config is baked once per configure
    /// into the shared backend build, so two bringups whose files differ must
    /// not share one; two with the same bytes still do, and a bringup with no
    /// file (or a file for a backend this image does not use) keeps its path.
    #[test]
    fn a_bringups_rmw_config_is_configuration() {
        let t = tempfile::tempdir().expect("tempdir");
        let bringup = |name: &str, body: Option<&str>| {
            let d = t.path().join(name);
            std::fs::create_dir_all(d.join("rmw")).unwrap();
            if let Some(b) = body {
                std::fs::write(d.join("rmw").join("cyclonedds.xml"), b).unwrap();
            }
            d
        };
        let cyclone = |extra: &[(&str, &str)]| {
            let mut i = image(extra, None);
            i.rmw = Some("cyclonedds".to_string());
            i
        };
        let plain = bringup("plain", None);
        let a = bringup("a", Some("<CycloneDDS>a</CycloneDDS>"));
        let a2 = bringup("a2", Some("<CycloneDDS>a</CycloneDDS>"));
        let b = bringup("b", Some("<CycloneDDS>b</CycloneDDS>"));
        let img = cyclone(&[]);
        let base = cmake_coordinate("freertos", &img, Path::new("/nonexistent"));
        assert!(!base.contains("-cfg"), "{base}");
        assert_eq!(cmake_coordinate("freertos", &img, &plain), base);
        let ca = cmake_coordinate("freertos", &img, &a);
        assert!(ca.starts_with(&format!("{base}-cfg")), "{ca}");
        assert_eq!(ca, cmake_coordinate("freertos", &img, &a2));
        assert_ne!(ca, cmake_coordinate("freertos", &img, &b));
        // A zenoh image ignores a bringup's Cyclone file.
        assert_eq!(
            cmake_coordinate("freertos", &image(&[], None), &a),
            cmake_coordinate("freertos", &image(&[], None), Path::new("/nonexistent"))
        );
    }

    /// A declared transport's IMPLICATION is configuration too: a serial image
    /// turns smoltcp off in the runtime builds, so it cannot share them with an
    /// IP image — and it reads the same rule the cargo road does.
    #[test]
    fn a_transport_implication_is_configuration() {
        let serial = cmake_coordinate(
            "native",
            &image(&[], Some("serial/UART_0")),
            Path::new("/nonexistent"),
        );
        assert_ne!(
            serial,
            cmake_coordinate("native", &image(&[], None), Path::new("/nonexistent"))
        );
        assert_eq!(
            crate::cmd::leaf_settings::image_block_layers(None, &image(&[], Some("serial/UART_0")))
                .unwrap()
                .get("ZPICO_NO_SMOLTCP")
                .map(String::as_str),
            Some("1")
        );
    }

    /// Every GENERATED workspace fixture row (`image = …`) states its cmake
    /// build dir as a literal `build_subdir`, and the fixture build and the
    /// test locators both read that literal. Before W4 the coordinate could not
    /// move under it; now an image's configuration moves it (`-cfg<hash>`), so
    /// the literal is checked against the one derivation instead of trusted.
    #[test]
    fn every_generated_workspace_fixture_row_names_its_cmake_build_dir() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap();
        let manifest: toml::Value =
            toml::from_str(&std::fs::read_to_string(repo.join("examples/fixtures.toml")).unwrap())
                .unwrap();
        let catalog =
            crate::orchestration::board_descriptor::BoardCatalog::load(&repo).expect("catalog");
        let mut checked = 0usize;
        let mut wrong = Vec::new();
        for row in manifest
            .get("workspace_fixture")
            .and_then(|r| r.as_array())
            .into_iter()
            .flatten()
        {
            let (Some(dir), Some(bringup), Some(image_id), Some(subdir)) = (
                row.get("dir").and_then(|v| v.as_str()),
                row.get("bringup").and_then(|v| v.as_str()),
                row.get("image").and_then(|v| v.as_str()),
                row.get("build_subdir").and_then(|v| v.as_str()),
            ) else {
                continue;
            };
            let ws = repo.join(dir);
            let found = discover::discover(&ws, &[]).expect("discover");
            let bringups = collect_images(&found.packages).expect("images");
            let bringup_name = std::path::Path::new(bringup)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap();
            let Some((_, bd, _, img)) = plan::all_images(&bringups)
                .into_iter()
                .find(|(b, _, id, _)| b == bringup_name && id == image_id)
            else {
                wrong.push(format!("{dir} {bringup_name}:{image_id}: no such image"));
                continue;
            };
            let Ok(d) = crate::orchestration::image::resolve_image_board(&catalog, image_id, &img)
            else {
                continue;
            };
            let platform = d.platform.kebab();
            // The row's own language answers "does the image link non-Rust
            // code" — the planner's `image_has_non_rust` closure is local to it.
            // A row's `lang` is also `mixed`, which is no single `Language` and
            // does link non-Rust code — so "non-Rust unless it parses as Rust".
            let non_rust = !matches!(
                row.get("lang")
                    .and_then(|v| v.as_str())
                    .map(nros_lang::Language::parse),
                Some(Ok(nros_lang::Language::Rust))
            );
            let driver = plan::driver_for_board(platform, d.entry_kind, non_rust);
            let Some(want) = driver
                .ok()
                .and_then(|dr| cmake_build_dir_for(dr, platform, &img, &bd))
            else {
                continue;
            };
            checked += 1;
            if want != subdir {
                wrong.push(format!(
                    "{dir} {bringup_name}:{image_id}: build_subdir = {subdir:?}, nros build uses {want:?}"
                ));
            }
        }
        assert!(
            checked > 0,
            "no generated cmake workspace row was checked — the walk is vacuous"
        );
        assert!(
            wrong.is_empty(),
            "examples/fixtures.toml `build_subdir` disagrees with `cmake_coordinate` \
             (phase-481 W4 — write what nros build uses):\n  {}",
            wrong.join("\n  ")
        );
    }
}

#[cfg(all(test, unix))]
mod registry_patch_spelling_tests {
    use super::*;

    /// Issue 0849 — a nano-ros root reached through a symlink still spells
    /// every patch row by the checkout's realpath, the spelling the central
    /// `nros-patch.toml` uses. Two spellings in one resolution are two copies
    /// of every core crate, which cargo refuses on `links`.
    #[test]
    fn a_symlinked_nano_ros_root_patches_by_the_real_checkout() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real");
        let board = real.join("nr/packages/boards/nros-board-x");
        std::fs::create_dir_all(&board).unwrap();
        std::fs::write(
            board.join("Cargo.toml"),
            "[package]\nname = \"nros-board-x\"\n",
        )
        .unwrap();
        let link = tmp.path().join("via-link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let leaf = tmp.path().join("leaf");
        std::fs::create_dir_all(&leaf).unwrap();
        std::fs::write(
            leaf.join("Cargo.toml"),
            "[package]\nname = \"leaf\"\n\n[dependencies]\nnros-board-x = \"*\"\n",
        )
        .unwrap();
        let rows = registry_patches(&leaf, &link.join("nr"), &leaf);
        assert_eq!(
            rows.get("nros-board-x"),
            Some(&board.canonicalize().unwrap()),
            "{rows:?}"
        );
    }
}

#[cfg(test)]
mod zephyr_leaf_application_tests {
    use super::zephyr_leaf_application;

    /// A standalone Zephyr leaf is its own west application; a workspace
    /// bringup with an ament `CMakeLists.txt`, or one that only mentions
    /// Zephyr in a comment, is not.
    #[test]
    fn only_a_leaf_whose_cmakelists_finds_zephyr_is_its_own_application() {
        let td = tempfile::tempdir().unwrap();
        let leaf = td.path();
        std::fs::write(
            leaf.join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.20)\nfind_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})\n",
        )
        .unwrap();
        assert_eq!(zephyr_leaf_application(leaf, leaf).as_deref(), Some(leaf));
        // A bringup INSIDE a workspace never answers for the workspace.
        assert_eq!(
            zephyr_leaf_application(leaf, &leaf.join("src/bringup")),
            None
        );

        std::fs::write(
            leaf.join("CMakeLists.txt"),
            "# not find_package(Zephyr) here\nfind_package(ament_cmake REQUIRED)\n",
        )
        .unwrap();
        assert_eq!(zephyr_leaf_application(leaf, leaf), None);

        std::fs::remove_file(leaf.join("CMakeLists.txt")).unwrap();
        assert_eq!(zephyr_leaf_application(leaf, leaf), None);
    }
}

#[cfg(test)]
mod board_net_file_tests {
    /// RFC-0109 D4 (phase-514 W2) — the workspace road reads an image's
    /// interfaces from the board network file its `os_config` names. The Rust
    /// workspace's multihost pair sits on the `br` network those files define,
    /// each at its own address.
    #[test]
    fn the_workspace_road_resolves_interfaces_from_board_net_files() {
        use nros_board_common::image_net::Address;
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap();
        let catalog =
            crate::orchestration::board_descriptor::BoardCatalog::load(&repo).expect("catalog");
        let bringup = repo.join("examples/workspaces/rust/src/demo_bringup");
        for (image, host) in [("freertos_br_robot1", 10), ("freertos_br_robot2", 11)] {
            let net = super::workspace_image_net(&catalog, &bringup, image, Some(&repo))
                .expect("resolves")
                .unwrap_or_else(|| panic!("{image} has a network"));
            let eth0 = &net.interfaces[0];
            assert_eq!(eth0.name, "eth0");
            assert_eq!(eth0.network, "br");
            assert_eq!(eth0.address, Address::Static([192, 0, 3, host]), "{image}");
            // RFC-0109 D6 (phase-514 W6) — how the launcher attaches it is the
            // image's `[image.<id>.run] nic`, and a bridge is shared.
            assert_eq!(
                eth0.attach,
                crate::orchestration::image_net::NetAttach::Bridge("qemu-br".into()),
                "{image}"
            );
            assert!(!eth0.per_image, "{image}");
        }
    }

    /// RFC-0109 D9 check 6 (phase-514 W4) — every tracked image names only
    /// layer-1 files its board's network owner reads: Zephyr fragments on a
    /// Zephyr board, board network files on a reference board. The sweep is
    /// the tree's own `system.toml`s, so a migration that put a file on the
    /// wrong board fails here rather than at that image's build.
    #[test]
    fn every_tracked_image_names_layer_1_files_its_board_reads() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap();
        let catalog =
            crate::orchestration::board_descriptor::BoardCatalog::load(&repo).expect("catalog");
        let out = crate::source_stamp::nros_git_command("git")
            .args(["ls-files", "*system.toml"])
            .current_dir(&repo)
            .output()
            .expect("git ls-files");
        let (mut checked, mut problems) = (0, Vec::new());
        for rel in String::from_utf8_lossy(&out.stdout).lines() {
            let path = repo.join(rel);
            let Ok(sys) = toml::from_str::<crate::orchestration::cargo_metadata_schema::SystemToml>(
                &std::fs::read_to_string(&path).unwrap(),
            ) else {
                continue;
            };
            for (id, img) in sys.resolved_images() {
                let files = img.os_config_files();
                if files.is_empty() {
                    continue;
                }
                let Ok(d) = crate::orchestration::image::resolve_image_board(&catalog, &id, &img)
                else {
                    continue;
                };
                let board = img.board.clone().unwrap_or_default();
                let net = super::descriptor_board_net(d, &board, Some(&repo)).unwrap();
                checked += 1;
                if let Err(e) = net.check_os_config(&files) {
                    problems.push(format!("{rel} [image.{id}]: {e}"));
                }
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
        assert!(checked > 50, "the sweep reached only {checked} images");
    }
}

/// phase-460 W1 follow-up -- `nros build` opens a resolved model through the
/// gated door. A refused `nros sync` leaves a marker and no model; the
/// stage-4 entry generator and the single-package road both read that as a
/// refusal (the build stops), never as "nothing to read" or as a cue to
/// resolve one locally.
#[cfg(test)]
mod model_door_tests {
    use super::*;

    fn bringup_with_marker() -> (tempfile::TempDir, std::path::PathBuf, String) {
        let ws = tempfile::tempdir().unwrap();
        let bringup = ws.path().join("src").join("demo_bringup");
        std::fs::create_dir_all(bringup.join("launch")).unwrap();
        std::fs::write(bringup.join("system.toml"), "").unwrap();
        let rel = nros_orchestration_ir::model_location::launch_to_model_rel(&bringup, None, &[])
            .unwrap();
        // The sync rung, holding the marker and no model: what a refused
        // resolve leaves behind (`model_gate::quarantine`).
        let rung = ws
            .path()
            .join("build/nros/models/demo_bringup")
            .join("system_model.yaml");
        std::fs::create_dir_all(rung.parent().unwrap()).unwrap();
        std::fs::write(
            crate::model_gate::marker_path(&rung),
            "check: contract-check\ninput: system.toml\n",
        )
        .unwrap();
        (ws, bringup, rel)
    }

    #[test]
    fn a_refused_model_stops_the_entry_generator() {
        let (_ws, bringup, rel) = bringup_with_marker();
        match open_model(&bringup, &rel) {
            Err(ModelDoor::Refused(r)) => {
                assert!(r.to_string().contains("contract-check"), "{r}")
            }
            Err(ModelDoor::Unresolved(e)) => panic!("read as unresolvable: {e}"),
            Ok(p) => panic!("opened {}", p.display()),
        }
    }

    #[test]
    fn a_refused_leaf_model_is_a_refusal_not_an_absence() {
        let (_ws, bringup, _rel) = bringup_with_marker();
        assert!(crate::leaf_entity_env::leaf_model_path(&bringup).is_err());
        assert!(crate::leaf_entity_env::leaf_model(&bringup).is_none());
    }
}
