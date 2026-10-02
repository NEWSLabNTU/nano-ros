//! phase-463 W2 -- `nros ws entity-census`, the verb that runs an entry's own
//! native binary as the census producer.
//!
//! A CENSUS is what the code CREATES, observed: every publisher, subscription,
//! service, timer, guard condition and parameter a run of the entry declared,
//! keyed by node. It is the second of the three documents phase-463's check
//! joins; the other two are the contract as authored and
//! `build/nros/entity_inventory.json` as derived. W3 adds `check`, which is
//! the join; this wave is the producer.
//!
//! # Why the ENTRY and not a per-component probe
//!
//! The generated native entry already does, in the order boot does it, every
//! step a census depends on: it registers the linked backend, opens the
//! executor, seeds every LAUNCHED parameter, placement-news every component in
//! launch order, and registers the parameter services. A per-component probe
//! (phase-313) sees one component with no launch parameters, which is how a
//! timer whose period comes from `rate` gets recorded at the wrong period; and
//! on the safety island the batch probe does not build at all. The entry has
//! the whole image and needs no second build: `NROS_CENSUS_OUT` makes the
//! hosted boot funnel write what the recorder saw and exit where it would
//! otherwise spin, so the census binary IS the boot binary.
//!
//! The probe stays for leaf packages with no entry and no bringup. For an
//! entry with a bringup, this supersedes it.
//!
//! # What this verb adds to what the binary writes
//!
//! The binary writes the RECORDER's document -- schema v2, the one emitter
//! phase-308 left, the same one a probe sidecar carries. This verb wraps it
//! with provenance in the RFC-0063 shape (a derived artifact carries its
//! inputs' digests) and writes the result atomically to
//! `build/nros/census/<entry>.json`, so a later check can tell a census of
//! today's sources from a census of last week's.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use clap::{Args as ClapArgs, Subcommand};
use eyre::{Result, WrapErr, bail};
use sha2::{Digest, Sha256};

/// Where a census taken against NO model lands, under the build directory.
///
/// A census taken against a model goes beside the model instead -- see
/// [`census_path_for_model`] -- and that is the only place a cross configure
/// looks, so a census here is for a person reading it, not for a gate.
pub(crate) const CENSUS_DIR: &str = "nros/census";

/// Issue 1419 -- THE place a census of a model lives:
/// `<model-dir>/<model-stem>.census.json`, beside the model it was taken
/// against.
///
/// Keyed by the MODEL, never by the entry or the build directory, because the
/// two documents that have to meet here are written by two different builds:
/// the census by the NATIVE entry (`native_entry`, in the native image's build
/// directory), and the check by a CROSS configure (`threadx_entry`, in a
/// different build directory). phase-463 W4 keyed both sides on
/// `<their own build dir>/nros/census/<their own entry>.json`, so on the real
/// `nros build` layout they never named the same file: measured on
/// `examples/workspaces/cpp`, the threadx configure looked for
/// `build/threadx-linux-zenoh/cmake/nros/census/threadx_entry.json` and its
/// remedy said `nros ws entity-census run --entry threadx_entry` -- an entry
/// with no native binary. The one thing both builds DO share is the resolved
/// model (`build/nros/models/<bringup>/<stem>.yaml`, measured identical for
/// both), and a census is a statement about what that model's launch creates.
///
/// One spelling: `run` writes here, `check` reads here, and cmake asks this
/// function through `nros ws entity-census path` rather than re-deriving it.
pub(crate) fn census_path_for_model(model: &Path) -> PathBuf {
    let stem = model
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "model".to_string());
    model.with_file_name(format!("{stem}.census.json"))
}

/// This wrapper's own schema. The recorder's version travels beside it under
/// `census.version`, unchanged; they move independently and a reader that
/// confuses them would read the wrong field.
const CENSUS_SCHEMA: &str = "nros.entity_census/1";

/// The two variables a census run decides for the child, named rather than
/// spelled at the call site.
///
/// `check-config-knob-census` reads this crate's sources as BUILD-TIME ones
/// (its manifest names `nros-build-helpers`, which is what puts it in that
/// scan) and classifies `Command::env` as a READ of the environment, which it
/// is not -- it sets a CHILD's, the thing `with_env` is in that check's
/// not-a-read list for. Neither name is a build knob: one is this verb's own
/// switch and the other is a runtime spin budget. Naming them here keeps the
/// ladder census describing build knobs; when phase-461 W1 releases
/// `scripts/check/config-knob-census.py`, `env`/`env_remove` belong in its
/// NON_READ list and these can go back to being literals.
const CENSUS_OUT_ENV: &str = "NROS_CENSUS_OUT";
const ENTRY_SPIN_ENV: &str = "NROS_ENTRY_SPIN_MS";

#[derive(Debug, ClapArgs)]
pub struct EntityCensusArgs {
    #[command(subcommand)]
    pub command: Sub,
}

#[derive(Debug, Subcommand)]
pub enum Sub {
    /// Run the entry's native binary in census mode and write the census.
    Run(RunArgs),
    /// Compare a census with the contract that declared it (phase-463 W3).
    Check(CheckArgs),
    /// Print where the census of a model lives (issue 1419). The ONE spelling
    /// of that path, for a caller -- a cmake configure -- that has to register
    /// the file before the check reads it.
    Path(PathArgs),
    /// Make sure the census a CROSS image's configure will check exists and
    /// is fresh: build the native image generated from the same launch file,
    /// to its fixed point, and run it in census mode (issue 1419).
    ///
    /// The one command a cross build runs before its configure. It does
    /// nothing when there is nothing to do -- a host image, a model with no
    /// contract, a census that is already fresh -- so a build script can call
    /// it before every cross image.
    Take(TakeArgs),
}

#[derive(Debug, ClapArgs)]
pub struct TakeArgs {
    /// The image whose configure will CHECK the census: `<bringup>:<image>`,
    /// or a bare id when one bringup declares it. Usually a cross image
    /// (`demo_bringup:threadx`); the census itself is taken by the native
    /// image generated from the same launch file.
    #[arg(long, value_name = "IMAGE")]
    pub image: String,

    /// Workspace root. Defaults to the current directory.
    #[arg(long, value_name = "DIR")]
    pub workspace: Option<PathBuf>,

    /// Build the native image with `nros build --offline`.
    #[arg(long)]
    pub offline: bool,

    /// Give up on a census run that has not exited in this many seconds.
    #[arg(long, default_value = "60", value_name = "SECS")]
    pub timeout_secs: u64,
}

#[derive(Debug, ClapArgs)]
pub struct PathArgs {
    /// The resolved SystemModel the census is of.
    #[arg(long, value_name = "PATH")]
    pub model: PathBuf,
}

#[derive(Debug, ClapArgs)]
pub struct RunArgs {
    /// The entry to census. Names the executable the workspace built and the
    /// census file this writes (`<build-dir>/nros/census/<entry>.json`).
    #[arg(long, value_name = "NAME")]
    pub entry: String,

    /// Workspace root. Defaults to the current directory.
    #[arg(long, value_name = "DIR")]
    pub workspace: Option<PathBuf>,

    /// The build directory the workspace configured into.
    #[arg(long, default_value = "build", value_name = "DIR")]
    pub build_dir: PathBuf,

    /// The native entry binary, when it is not where the layouts below put it.
    #[arg(long, value_name = "PATH")]
    pub binary: Option<PathBuf>,

    /// The resolved SystemModel this entry was generated from. Its
    /// `meta.inputs` digests and its node packages become part of the
    /// census's provenance. Verified through the one model gate before it is
    /// read (phase-460 W1).
    #[arg(long, value_name = "PATH")]
    pub model: Option<PathBuf>,

    /// Write the census here. Defaults to beside `--model`
    /// (`<model-dir>/<stem>.census.json`), which is where a cross configure of
    /// any entry generated from the same model looks (issue 1419); with no
    /// `--model`, to `<build-dir>/nros/census/<entry>.json`, which nothing
    /// reads but a person.
    #[arg(long, value_name = "PATH")]
    pub out: Option<PathBuf>,

    /// Give up on a census run that has not exited in this many seconds.
    ///
    /// A census run constructs components and exits; it never spins. A run
    /// that does not finish is a node whose constructor blocks (issue 0286's
    /// shape), and a bounded wait reports that instead of hanging a build.
    #[arg(long, default_value = "60", value_name = "SECS")]
    pub timeout_secs: u64,
}

/// phase-463 W3 -- `nros ws entity-census check`.
///
/// The three documents are named rather than discovered, for the same reason
/// `run` takes `--binary`: this verb is what a recipe and a person both call,
/// and a check whose inputs are inferred cannot be reproduced by hand from the
/// line a build log printed. phase-463 W4 is the wave that wires it into
/// `nros sync` with the freshness rules.
#[derive(Debug, ClapArgs)]
pub struct CheckArgs {
    /// The census a `run` wrote. Both the wrapped form and the recorder's own
    /// document are accepted. Defaults to the census of `--model`
    /// (`<model-dir>/<stem>.census.json`, issue 1419), which is where `run
    /// --model` writes it -- naming it here is for a census kept elsewhere.
    #[arg(long, value_name = "PATH")]
    pub census: Option<PathBuf>,

    /// The authored contract, `<bringup>/launch/<stem>.contract.yaml`.
    ///
    /// phase-463 W4 made it OPTIONAL for one caller: a cross configure knows
    /// the model and not the launch stem, and re-deriving the sidecar's name
    /// in cmake would be a second spelling of a rule the model already
    /// records. When it is omitted, the contract is the `*.contract.yaml` the
    /// model's own `meta.inputs` names.
    #[arg(long, value_name = "PATH")]
    pub contract: Option<PathBuf>,

    /// `nros_entity_inventory.json`, the DERIVED third view. Optional: its
    /// absence costs the note that says what the pools were sized from, not
    /// the comparison.
    #[arg(long, value_name = "PATH")]
    pub inventory: Option<PathBuf>,

    /// The bringup's `system.toml`, which is where `[census.waive]` lives.
    #[arg(long, value_name = "PATH")]
    pub system_toml: Option<PathBuf>,

    /// The resolved SystemModel. Read only through the one model gate
    /// (phase-460 W1), so a refused or stale model refuses the check rather
    /// than quietly making it pass.
    #[arg(long, value_name = "PATH")]
    pub model: Option<PathBuf>,

    /// Turn warnings into errors. The merge queue's setting: an `unobserved`
    /// row is honest in a workspace a person is editing and is a hole in a
    /// gate.
    #[arg(long)]
    pub strict: bool,

    /// phase-463 W4 -- refuse to compare against a census that is absent, or
    /// that no longer describes the code.
    ///
    /// This is what an RTOS image's configure passes. Without it a check is
    /// still honest about the two documents it was handed; with it, the check
    /// first asks whether the census is still a statement about THIS source,
    /// and `[census] on_missing` / `on_stale` in `system.toml` decide whether
    /// the answer stops the build.
    ///
    /// Freshness is content-addressed, never mtime-based (the rule
    /// `metadata_refresh` states and the reason it states it): touching a
    /// source file leaves a census fresh, and adding an entity does not.
    #[arg(long)]
    pub require_fresh: bool,

    /// The workspace root the census's recorded input paths resolve against.
    /// Defaults to the current directory, which is what a person typing this
    /// verb in their workspace means.
    #[arg(long, value_name = "DIR")]
    pub workspace: Option<PathBuf>,

    /// The entry this census is of. Used in the remedy line of a freshness
    /// refusal, so the message names the command that fixes it rather than
    /// the shape of the command.
    #[arg(long, value_name = "NAME")]
    pub entry: Option<String>,
}

pub fn run(args: EntityCensusArgs) -> Result<()> {
    match args.command {
        Sub::Run(a) => run_census(a),
        Sub::Check(a) => check_census(a),
        Sub::Path(a) => {
            println!("{}", census_path_for_model(&a.model).display());
            Ok(())
        }
        Sub::Take(a) => take_census(a),
    }
}

// ---------------------------------------------------------------------------
// issue 1419 -- `take`: the census a cross configure will ask for, produced
// before it asks
// ---------------------------------------------------------------------------

/// How many builds `take` allows the native image to reach its fixed point in.
///
/// Measured on `examples/workspaces/cpp` (2026-10-03): from a CLEAN build
/// directory the first `nros build demo_bringup:native` links a `native_entry`
/// whose bytes the second build changes -- the message-bound fragments are
/// written by codegen DURING the first build, so its knobs are the
/// placeholders and only the next configure reads the real ones (issue 1252,
/// the one re-configure that survives). A census of the first binary is stale
/// the moment anything rebuilds the image. The second and every later build
/// are byte-identical, so two builds is the fixed point and three is the
/// ceiling before this calls the image non-convergent.
const TAKE_MAX_BUILDS: usize = 3;

/// The one sibling that takes the census for `model_rel`: a HOST image of
/// the same bringup resolved from the same launch and arguments.
///
/// "Host" is read off the board descriptor's platform, never off the image
/// id or the board name -- an image is conventionally called `native`, and a
/// convention is what issue 1397 found wrong when it was relied on. Several
/// host images can share one model (`native`, `native_cyclonedds`,
/// `native_xrce` all resolve the default launch); their census is the same
/// statement about the same components, so the FIRST by id is taken and the
/// choice is stable -- a second producer for one census file would make the
/// file's recorded binary depend on which ran last.
fn census_producer_for<'a>(
    images: &'a [(
        String,
        PathBuf,
        String,
        crate::orchestration::image::ImageBlock,
    )],
    bringup: &str,
    model_rel: &str,
    is_host: &dyn Fn(&crate::orchestration::image::ImageBlock) -> bool,
) -> Option<&'a (
    String,
    PathBuf,
    String,
    crate::orchestration::image::ImageBlock,
)> {
    images.iter().find(|(b, dir, _, img)| {
        b == bringup && is_host(img) && image_model_rel(dir, img).is_ok_and(|rel| rel == model_rel)
    })
}

/// The model an image is resolved into, relative to its bringup -- the same
/// call `nros build` makes for the same image.
fn image_model_rel(
    bringup_dir: &Path,
    image: &crate::orchestration::image::ImageBlock,
) -> std::result::Result<String, String> {
    let args: Vec<(String, String)> = image
        .args
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    nros_orchestration_ir::model_location::launch_to_model_rel(
        bringup_dir,
        image.launch.as_deref(),
        &args,
    )
}

fn take_census(args: TakeArgs) -> Result<()> {
    use crate::{
        builder::{discover, plan},
        orchestration::board_descriptor::{BoardCatalog, PlatformKind},
    };

    let root = match &args.workspace {
        Some(w) => w.clone(),
        None => std::env::current_dir().wrap_err("no current directory")?,
    };
    let root = std::fs::canonicalize(&root)
        .wrap_err_with(|| format!("resolving workspace root {}", root.display()))?;

    let members = discover::cargo_members_or_walk(&root);
    let found = discover::discover(&root, &members).map_err(|e| eyre::eyre!("{e}"))?;
    let bringups = crate::cmd::build::collect_images(&found.packages)?;
    let asked = plan::resolve(&bringups, std::slice::from_ref(&args.image))
        .map_err(|e| eyre::eyre!("{e}"))?;
    let (bringup, bringup_dir, image_id, image) = asked
        .into_iter()
        .next()
        .ok_or_else(|| eyre::eyre!("`{}` resolved to no image", args.image))?;
    let asked_q = plan::qualified(&bringup, &image_id);

    let nano_ros_root = crate::orchestration::nano_ros_root::resolve(None, &root)
        .ok_or_else(|| eyre::eyre!("{}", crate::orchestration::nano_ros_root::not_found_help()))?;
    let pkg_dirs: Vec<PathBuf> = found.packages.iter().map(|p| p.dir.clone()).collect();
    let catalog = BoardCatalog::load_with_packages(&nano_ros_root, &pkg_dirs)
        .map_err(|e| eyre::eyre!("loading board descriptors: {e}"))?;
    let is_host = |img: &crate::orchestration::image::ImageBlock| -> bool {
        img.board
            .as_deref()
            .and_then(|b| catalog.resolve(b, ""))
            .is_some_and(|d| d.platform == PlatformKind::Posix)
    };

    if is_host(&image) {
        println!(
            "entity-census take {asked_q}: a host image is the census PRODUCER, and its own \
             configure checks nothing -- nothing to take"
        );
        return Ok(());
    }

    let model_rel = image_model_rel(&bringup_dir, &image)
        .map_err(|e| eyre::eyre!("cannot resolve the launch of `{asked_q}`: {e}"))?;
    let (model_path, _) =
        nros_orchestration_ir::model_location::ensure_model(&bringup_dir, &model_rel)
            .map_err(|e| eyre::eyre!("cannot resolve the SystemModel of `{asked_q}`: {e}"))?;

    // Same question the configure asks first, answered the same way.
    if discover_contract(Some(&model_path)).is_none() {
        println!(
            "entity-census take {asked_q}: no contract -- `{}` folds in no `*.contract.yaml`, so \
             its configure has nothing for a census to reconcile",
            model_path.display()
        );
        return Ok(());
    }

    let census_path = census_path_for_model(&model_path);
    if let Freshness::Fresh = census_freshness(&census_path, &root) {
        let complete = std::fs::read_to_string(&census_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .is_some_and(|c| incomplete_reason(&c).is_none());
        if complete {
            println!(
                "entity-census take {asked_q}: fresh -> {}",
                census_path.display()
            );
            return Ok(());
        }
    }

    let all = plan::all_images(&bringups);
    let Some((_, _, native_id, _)) = census_producer_for(&all, &bringup, &model_rel, &is_host)
    else {
        bail!(
            "`{asked_q}` is resolved from `{model_rel}`, whose contract its configure checks \
             against a census -- and bringup `{bringup}` declares no HOST image resolved from the \
             same launch and arguments to take one. Declare one beside it (`[image.native] board \
             = \"native\"`, with the same `launch`/`args`); the census binary IS a host build of \
             the same components (phase-463 W2)."
        );
    };
    let native_q = plan::qualified(&bringup, native_id);
    let entry = crate::builder::entry::package_name(native_id);
    println!("entity-census take {asked_q}: taking the census with `{native_q}` ({entry})");

    let nros = std::env::current_exe().wrap_err("locating this nros binary")?;
    let build_dir = root.join("build");
    let mut previous: Option<String> = None;
    let mut binary = None;
    for attempt in 1..=TAKE_MAX_BUILDS {
        let mut cmd = Command::new(&nros);
        cmd.arg("build")
            .arg(&native_q)
            .arg("--workspace")
            .arg(&root);
        if args.offline {
            cmd.arg("--offline");
        }
        let status = cmd
            .status()
            .wrap_err_with(|| format!("running `nros build {native_q}`"))?;
        if !status.success() {
            bail!("`nros build {native_q}` exited {status}; no census can be taken without it");
        }
        let bin = locate_entry_binary(&build_dir, &entry).wrap_err_with(|| {
            format!(
                "`nros build {native_q}` succeeded but left no `{entry}` binary. A Rust entry has \
                 no census producer yet (issue 1419): its hooks are on the C++ ABI"
            )
        })?;
        let digest = file_digest(&bin)?;
        if previous.as_deref() == Some(digest.as_str()) {
            binary = Some(bin);
            break;
        }
        if attempt == TAKE_MAX_BUILDS {
            bail!(
                "`{native_q}` did not reach a fixed point in {TAKE_MAX_BUILDS} builds -- `{entry}` \
                 changed on every one, so any census of it is stale the moment it is written. A \
                 build that does not converge is the defect; see issue 1252"
            );
        }
        previous = Some(digest);
    }
    let binary = binary.expect("the loop breaks with a binary or bails");

    // Asked AGAIN, after the build. Before it, an unsynced workspace has no
    // `build/nros/models/` and `ensure_model` answers from its cache; the
    // build resolves the model into the workspace, which is the path the
    // cross configure reads -- so a census keyed by the first answer would
    // sit beside a model no configure opens.
    let (model_path, _) =
        nros_orchestration_ir::model_location::ensure_model(&bringup_dir, &model_rel)
            .map_err(|e| eyre::eyre!("cannot resolve the SystemModel of `{asked_q}`: {e}"))?;

    run_census(RunArgs {
        entry,
        workspace: Some(root.clone()),
        build_dir,
        binary: Some(binary),
        model: Some(model_path),
        out: None,
        timeout_secs: args.timeout_secs,
    })
}

fn check_census(args: CheckArgs) -> Result<()> {
    // The model gate FIRST, before anything is read: a check that opens a
    // refused model and then passes has laundered the refusal (phase-460 W1).
    if let Some(model) = args.model.as_deref() {
        crate::model_gate::verify(model, None)
            .map_err(|r| eyre::eyre!("{r}"))
            .wrap_err("the model this census was taken against")?;
    }

    // `system.toml` is read ONCE, here: it carries both halves of how this
    // system answers the census -- W3's per-row waivers and W4's two policy
    // keys -- and reading it twice is how the two halves would come to
    // disagree about which file they read.
    let system = read_system_toml(args.system_toml.as_deref())?;
    let policy = system
        .as_ref()
        .and_then(|s| s.census.clone())
        .unwrap_or_default();

    // Issue 1419 -- the census of THIS model, unless one is named.
    let census_path = match (&args.census, &args.model) {
        (Some(path), _) => path.clone(),
        (None, Some(model)) => census_path_for_model(model),
        (None, None) => bail!(
            "no --census and no --model. A census is of a model: pass `--model` and the census \
             `nros ws entity-census run --model` wrote beside it is read, or name one with \
             `--census`."
        ),
    };

    // Issue 1419 -- WHICH contract, before anything else is asked. A model
    // that folds in no `*.contract.yaml` has no contract-derived pool, so a
    // census of it has nothing to reconcile -- and asking for one would make
    // the configure demand (or warn about) an artifact that could never be
    // compared. Measured on `examples/workspaces/c`, which has no contract:
    // its threadx configure warned "census missing" with no census, and
    // FAILED the moment a census was taken ("no --contract, and the model
    // names no `*.contract.yaml`"), i.e. producing the evidence broke the
    // build. A missing contract the model DOES name never gets here: the
    // phase-460 W1 gate above re-hashes every input the model records.
    let contract_path = match args.contract.clone() {
        Some(p) => p,
        None => match discover_contract(args.model.as_deref()) {
            Some(p) => p,
            None if args.model.is_some() => {
                println!(
                    "census check: no contract -- the model folds in no `*.contract.yaml`, so no \
                     pool of this image is derived from one and there is nothing for a census \
                     to reconcile (issue 1419)"
                );
                return Ok(());
            }
            None => bail!(
                "no --contract, and no --model to find one in. The contract is \
                 `<bringup>/launch/<stem>.contract.yaml`, beside the launch file this entry was \
                 resolved from."
            ),
        },
    };

    // phase-463 W4 -- the freshness question, BEFORE the comparison. A check
    // that compares against a museum census and then passes has said
    // something true about two documents and nothing at all about the code.
    if args.require_fresh {
        let ws = match args.workspace.clone() {
            Some(w) => w,
            None => std::env::current_dir().wrap_err("no current directory")?,
        };
        match census_freshness(&census_path, &ws) {
            Freshness::Fresh => {}
            Freshness::Missing(why) => {
                return freshness_verdict(
                    policy.on_missing,
                    "missing",
                    &why,
                    args.entry.as_deref(),
                    args.model.as_deref(),
                );
            }
            Freshness::Stale(why) => {
                return freshness_verdict(
                    policy.on_stale,
                    "stale",
                    &why,
                    args.entry.as_deref(),
                    args.model.as_deref(),
                );
            }
        }
    }

    let census_raw = std::fs::read_to_string(&census_path)
        .wrap_err_with(|| format!("cannot read census `{}`", census_path.display()))?;
    let census: serde_json::Value = serde_json::from_str(&census_raw)
        .wrap_err_with(|| format!("`{}` is not JSON", census_path.display()))?;

    let contract = ros_launch_manifest_types::parse_manifest(&contract_path)
        .map_err(|e| eyre::eyre!("{e}"))
        .wrap_err_with(|| format!("cannot read contract `{}`", contract_path.display()))?;

    let inventory = match args.inventory.as_deref() {
        Some(path) => {
            let raw = std::fs::read_to_string(path)
                .wrap_err_with(|| format!("cannot read inventory `{}`", path.display()))?;
            Some(
                serde_json::from_str::<serde_json::Value>(&raw)
                    .wrap_err_with(|| format!("`{}` is not JSON", path.display()))?,
            )
        }
        None => None,
    };

    let waivers = policy.waivers();

    let report = crate::entity_census::check(crate::entity_census::Inputs {
        census: &census,
        contract: &contract,
        contract_path: contract_path.display().to_string(),
        inventory: inventory.as_ref(),
        waivers: &waivers,
        strict: args.strict,
    });
    print!("{}", report.render());

    // Issue 1419 -- a census whose run stopped early refuses WHATEVER its rows
    // say: the rows above are what was compared, and nothing after the failure
    // was observed at all. Printed after the rows, so a `missing-in-contract`
    // the partial run did record is read first.
    if let Some(why) = incomplete_reason(&census) {
        bail!(
            "census INCOMPLETE: {why}.\nThe {} row(s) above are everything the run created \
             before it stopped, and nothing after the failure was observed. Declare what the \
             code creates (or fix the image's sizing, if the contract is complete), rebuild \
             the native image, and take the census again.",
            report.rows.len()
        );
    }

    if report.refuses() {
        bail!(
            "the contract and the code disagree in {} place(s). Every row above names the node, \
             the entity, the file that should change and the line to add or remove; the \
             contract is a statement about the code, so one of the two is wrong.",
            report.errors()
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// phase-463 W4 -- freshness, and what a configure does about it
// ---------------------------------------------------------------------------

/// What `--require-fresh` found. Three answers, because two of them have
/// separate policies: a workspace that has never run the producer is not the
/// same statement as a census that would be BELIEVED and should not be.
pub(crate) enum Freshness {
    Fresh,
    Missing(String),
    Stale(String),
}

/// `system.toml`, read once, for both the waivers and the policy keys.
fn read_system_toml(
    path: Option<&Path>,
) -> Result<Option<crate::orchestration::cargo_metadata_schema::SystemToml>> {
    let Some(path) = path else {
        return Ok(None);
    };
    if !path.is_file() {
        // A bringup with no `system.toml` waives nothing and sets no policy,
        // which is the landing default. Absence is not an error here; a
        // MALFORMED file below still is.
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)
        .wrap_err_with(|| format!("cannot read `{}`", path.display()))?;
    let system =
        toml::from_str(&raw).wrap_err_with(|| format!("cannot parse `{}`", path.display()))?;
    Ok(Some(system))
}

/// Issue 1419 -- the reason a census is incomplete, when it is. Only the
/// WRAPPED form can say so; a bare recorder document carries no run status.
pub(crate) fn incomplete_reason(census: &serde_json::Value) -> Option<String> {
    census
        .get(INCOMPLETE_KEY)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// Is the census at `path` still a statement about the code in `ws`?
///
/// Content-addressed throughout: every recorded input is re-hashed by the
/// algorithm its own digest names (`metadata_refresh::recompute_digest`), so
/// `touch` on a source file changes nothing and ADDING an entity changes the
/// tree digest. That asymmetry is the whole property this wave is for, and it
/// is the reason mtimes are not consulted anywhere on this path.
///
/// A census this tree cannot READ is stale rather than an error: the question
/// asked is "may I believe this document", and "I cannot parse it" is a no.
pub(crate) fn census_freshness(path: &Path, ws: &Path) -> Freshness {
    if !path.is_file() {
        return Freshness::Missing(format!("no census at `{}`", path.display()));
    }
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Freshness::Stale(format!("`{}` cannot be read", path.display()));
    };
    let Ok(census) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Freshness::Stale(format!("`{}` is not JSON", path.display()));
    };

    // The RECORDER's schema, not this wrapper's. A recorder that moved makes
    // every census written under the old one stale by definition -- issue
    // 0427's rule for the resolver pin, applied to the producer.
    let recorded_schema = census
        .get("census")
        .unwrap_or(&census)
        .get("version")
        .and_then(serde_json::Value::as_u64);
    let expected = u64::from(crate::orchestration::metadata_refresh::RECORDER_SCHEMA_VERSION);
    match recorded_schema {
        Some(v) if v == expected => {}
        Some(v) => {
            return Freshness::Stale(format!("recorder schema {v}; this tree reads {expected}"));
        }
        None => {
            return Freshness::Stale(format!(
                "`{}` records no recorder schema version",
                path.display()
            ));
        }
    }

    let inputs: Vec<crate::orchestration::metadata_refresh::RecordedInput> = census
        .get("provenance")
        .and_then(|p| p.get("inputs"))
        .and_then(serde_json::Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let role = row.get("role")?.as_str()?;
                    if !is_freshness_input(role) {
                        return None;
                    }
                    Some(crate::orchestration::metadata_refresh::RecordedInput {
                        role: role.to_string(),
                        path: row.get("path")?.as_str()?.to_string(),
                        digest: row.get("digest")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if inputs.is_empty() {
        return Freshness::Stale(format!(
            "`{}` records no input describing the code (no binary, no source tree), so \
             nothing about it can be verified",
            path.display()
        ));
    }

    let stale = crate::orchestration::metadata_refresh::stale_recorded_inputs(ws, &inputs);
    if stale.is_empty() {
        return Freshness::Fresh;
    }
    Freshness::Stale(
        stale
            .iter()
            .map(|s| s.line())
            .collect::<Vec<_>>()
            .join("\n  "),
    )
}

/// Which recorded inputs decide FRESHNESS, as opposed to being provenance.
///
/// A census is a statement about THE CODE, so the inputs that can falsify it
/// are the ones that determine what the code creates: the binary that was run
/// and the component source trees it was built from. `nros ws entity-census
/// run` records more than that -- the model and, through it, the model's own
/// `meta.inputs`, which include the contract sidecar -- and recording them is
/// right: RFC-0063 says a derived artifact carries its inputs' digests, and a
/// reader wants to know which model this census was taken against.
///
/// They are NOT freshness inputs. DO NOT "simplify" this to "every recorded
/// digest": that reading is the obvious one, the phase doc's prose invites it,
/// and it destroys the signal this whole wave exists to produce.
///
/// The contract reaches this provenance through the model. So if every
/// recorded digest were a freshness input, the contract would invalidate the
/// very evidence it is being compared against, and FIXING a
/// `missing-in-contract` verdict -- adding the row the code already creates --
/// would demand a fresh census of code nobody touched. That trains people to
/// regenerate the census reflexively, as a step you perform to make the build
/// go, and a census taken that way stops being an observation of the code and
/// becomes a rubber stamp. A stale-census refusal then means nothing, because
/// it means nothing more than "run the command again".
///
/// The phase doc's own acceptance says the same thing in one line: edit a
/// component source and the census goes stale, then "add the contract row and
/// it configures" -- with no second census run.
///
/// The model is not going unchecked. It is verified on its own terms at every
/// door by the phase-460 W1 gate, which `check` calls FIRST, before anything
/// here is read. That is the right place for it, and it is not this one.
fn is_freshness_input(role: &str) -> bool {
    matches!(role, "binary" | "source_tree" | "entry_tu")
}

/// The command that makes a missing or stale census fresh, as one line a
/// person can paste.
///
/// Issue 1419 -- it names `take` for the IMAGE asking, when the asking entry
/// and the model say which image that is: a generated entry is
/// `<image>_entry` (`leaf_system::entry_package_name`) and the model lives
/// under its bringup. `take` then finds the native sibling, builds it to its
/// fixed point and runs it. Spelling `run --entry <native entry>` here instead
/// left the reader to work out which native image, to build it first, and --
/// measured -- to build it twice, since a census of a clean first build is
/// stale after the next one (issue 1252).
fn census_remedy(entry: Option<&str>, model: Option<&Path>) -> String {
    let image = entry.and_then(|e| e.strip_suffix("_entry"));
    let bringup = model
        .and_then(crate::model_gate::infer_bringup_dir)
        .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()));
    let take = match (bringup, image) {
        (Some(b), Some(i)) => format!("nros ws entity-census take --image {b}:{i}"),
        _ => "nros ws entity-census take --image <bringup>:<this image>".to_string(),
    };
    format!(
        "Take a census of the code as it is now, from the workspace root:\n    {take}\nIt builds \
         the NATIVE image generated from the same launch file and runs it in census mode. This \
         configure does not run it for you: building the native image from inside a cross \
         configure is the cross-cutting compile issue 0641 refuses."
    )
}

/// Apply `[census] on_missing` / `on_stale` to what freshness found.
///
/// Both arms say the SAME thing about the census and differ only in whether
/// the build continues, because a policy that changed the diagnosis would make
/// `warn` a different check rather than a softer one.
fn freshness_verdict(
    policy: crate::orchestration::cargo_metadata_schema::CensusPolicy,
    what: &str,
    why: &str,
    entry: Option<&str>,
    model: Option<&Path>,
) -> Result<()> {
    // Issue 1419 -- the remedy names the IMAGE asking and `take`, which finds
    // the NATIVE sibling generated from the same launch file: the entry asking
    // is usually a CROSS image (`threadx_entry`), which has no host binary.
    let remedy = census_remedy(entry, model);
    if policy.refuses() {
        bail!("census {what}: {why}\n{remedy}");
    }
    // `warn` is an opt-OUT now (issue 1419, 2026-10-03): `refuse` became the
    // default once every unattended build that configures a contracted cross
    // image takes the census first (`scripts/build/census-prepass.sh`). A
    // system that writes `warn` still hears it loudly, because this image's
    // pools are derived from a contract nothing compared with the code -- the
    // state that shipped `ExecutorFull` to a board with no console. The
    // `(WARNING` marker is what the configure keys a CMake WARNING on.
    println!("census {what} (WARNING, [census] on_{what} = \"warn\"): {why}");
    for line in remedy.lines() {
        println!("  {line}");
    }
    println!(
        "  This image's pools are derived from a contract that NOTHING has compared with the \
         code (issue 1419). The bringup's `system.toml` sets `[census] on_{what} = \"warn\"`; \
         remove it (the default is `refuse`) to make this a refusal."
    );
    Ok(())
}

/// The contract a model was resolved from, for a caller that knows the model
/// and not the launch stem.
///
/// Read out of the model's own `meta.inputs` rather than re-derived from a
/// stem: the resolver recorded every input it folded in, the sidecar is one of
/// them, and a second spelling of `<stem>.contract.yaml` in cmake would be one
/// more place for the rule to drift.
fn discover_contract(model: Option<&Path>) -> Option<PathBuf> {
    let model = model?;
    let raw = std::fs::read_to_string(model).ok()?;
    let system = ros_launch_manifest_model::SystemModel::from_yaml_str(&raw).ok()?;
    let bringup = crate::model_gate::infer_bringup_dir(model)?;
    system
        .meta
        .inputs
        .iter()
        .map(|i| bringup.join(&i.path))
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".contract.yaml"))
                && p.is_file()
        })
}

/// One recorded input, with the digest that makes the census reproducible.
///
/// The algorithm is part of the value (`sha256:` / `fnv1a64:`) because the two
/// kinds of input are hashed by different means for different reasons: a FILE
/// is hashed by content with the hash the model's own `meta.inputs` uses, and
/// a SOURCE TREE takes `metadata_refresh::source_digest`, the walk that
/// already decides when a sidecar is stale. A second spelling of either would
/// be a second answer to "did this input change".
#[derive(serde::Serialize)]
struct Input {
    role: &'static str,
    path: String,
    digest: String,
}

#[derive(serde::Serialize)]
struct Provenance {
    tool: String,
    inputs: Vec<Input>,
}

#[derive(serde::Serialize)]
struct Census {
    schema: &'static str,
    entry: String,
    provenance: Provenance,
    /// Issue 1419 -- present when the census run EXITED NON-ZERO after the
    /// recorder wrote what it had: the entities below are what was created
    /// before setup stopped, not the whole entry. Every check refuses such a
    /// census (see [`INCOMPLETE_KEY`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    incomplete: Option<String>,
    census: serde_json::Value,
}

/// Issue 1419 -- the key that marks a census whose run stopped early.
///
/// Why a census like that is WRITTEN rather than discarded: discarding the
/// document left nothing for the cross configure to compare -- it read
/// "census missing", which `[census] on_missing` lands as a warning. Kept, it
/// is a refusal on the host.
///
/// It used to be how the island's E3a ended: the native image is sized from
/// the contract this census checks, so a contract one entity short stopped
/// the census run at `ExecutorFull`, and the omitted entity was the one row
/// the recorder never saw. Since issue 1419's census sizing the run opens its
/// executor at the executor's ceilings instead (`CENSUS_SIZING` in
/// `nros-cpp`), so E3a is a named `missing-in-contract` row (measured on
/// `examples/workspaces/cpp` built with `NROS_EXECUTOR_MAX_CBS=1`). What is
/// left to land here is code that cannot boot at any sizing.
///
/// Why every check REFUSES it, whatever its rows say: a partial observation
/// cannot confirm that the contract is complete, and the failure that cut it
/// short is the UNDER direction this check exists to refuse.
pub(crate) const INCOMPLETE_KEY: &str = "incomplete";

fn run_census(args: RunArgs) -> Result<()> {
    let ws = match args.workspace {
        Some(w) => w,
        None => std::env::current_dir().wrap_err("no current directory")?,
    };
    let build = if args.build_dir.is_absolute() {
        args.build_dir.clone()
    } else {
        ws.join(&args.build_dir)
    };

    let binary = match args.binary {
        Some(b) => {
            if !b.is_file() {
                bail!("--binary `{}` is not a file", b.display());
            }
            b
        }
        None => locate_entry_binary(&build, &args.entry)?,
    };

    // Issue 1419 -- beside the MODEL when there is one, which is where every
    // configure generated from that model looks.
    let out = match (args.out.clone(), args.model.as_deref()) {
        (Some(out), _) => out,
        (None, Some(model)) => census_path_for_model(model),
        (None, None) => build.join(CENSUS_DIR).join(format!("{}.json", args.entry)),
    };
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)
            .wrap_err_with(|| format!("cannot create `{}`", dir.display()))?;
    }

    // The RAW recorder document, beside the census it becomes. Under the build
    // directory and not `/tmp`: it is build output, it is what a failed run
    // leaves for a human to read, and a temp file would be neither.
    let raw = out.with_extension("recorded.json");
    let _ = std::fs::remove_file(&raw);

    let started = Instant::now();
    let status = run_in_census_mode(&binary, &raw, Duration::from_secs(args.timeout_secs))?;
    let elapsed = started.elapsed();

    if !raw.is_file() {
        if !status.success() {
            bail!(
                "census run of `{}` exited {} and wrote nothing -- the funnel prints the reason",
                binary.display(),
                status
            );
        }
        bail!(
            "`{}` exited 0 in census mode but wrote no census at `{}` -- the image is built \
             without `metadata-mode`, or it is not a C++ entry (a Rust entry's funnel says so \
             and exits non-zero)",
            binary.display(),
            raw.display()
        );
    }
    let recorded = std::fs::read_to_string(&raw)
        .wrap_err_with(|| format!("cannot read `{}`", raw.display()))?;
    // Issue 1419 -- a run that wrote its census and THEN failed is recorded as
    // incomplete rather than thrown away. See `INCOMPLETE_KEY`.
    let incomplete = (!status.success()).then(|| {
        format!(
            "the census run of `{}` exited {status}: setup stopped before every component \
             was constructed (the funnel printed the reason). A census opens its executor at \
             the executor's own ceilings (64 callbacks, 64 nodes), not at the contract's, so \
             a contract one entity short no longer stops it -- that is a named \
             `missing-in-contract` row. What stops a census run is code that could not boot \
             on ANY sizing: more than 64 callbacks in one executor, a constructor that fails, \
             or a pool the census does not resize (the parameter store). The rows recorded \
             are what was created before the failure; an image built before issue 1419's \
             census sizing still stops at its contract's `ExecutorFull`, and rebuilding it is \
             the remedy",
            binary.display()
        )
    });
    let recorded: serde_json::Value = serde_json::from_str(&recorded)
        .wrap_err_with(|| format!("`{}` is not JSON", raw.display()))?;

    let census = Census {
        schema: CENSUS_SCHEMA,
        entry: args.entry.clone(),
        provenance: Provenance {
            tool: format!("nros-cli {}", env!("CARGO_PKG_VERSION")),
            inputs: collect_inputs(&ws, &binary, args.model.as_deref())?,
        },
        incomplete,
        census: recorded,
    };
    let json = serde_json::to_string_pretty(&census).wrap_err("serialize census")?;
    crate::atomic_file::atomic_write(&out, &json)
        .wrap_err_with(|| format!("cannot write `{}`", out.display()))?;

    let counts = summarise(&census.census);
    if let Some(why) = &census.incomplete {
        bail!(
            "entity-census {}: INCOMPLETE ({counts}) -> {}\n{why}.\nThe census is written and \
             marked incomplete; `nros ws entity-census check` against it refuses and names the \
             rows it could compare.",
            args.entry,
            out.display()
        );
    }
    println!(
        "entity-census {}: {} in {} ms -> {}",
        args.entry,
        counts,
        elapsed.as_millis(),
        out.display()
    );
    Ok(())
}

/// Run the binary with the census switch set, and nothing else changed.
///
/// `NROS_CENSUS_OUT` is the whole input. `NROS_RMW` is NOT set here on
/// purpose: the funnel selects the recording backend itself when the switch is
/// on (`census_select_backend` in `nros-cpp`), so a census run and a boot
/// differ by one variable rather than by two that have to agree.
/// `NROS_ENTRY_SPIN_MS` is cleared because an inherited one would be read by
/// the spin this mode replaces.
fn run_in_census_mode(
    binary: &Path,
    out: &Path,
    timeout: Duration,
) -> Result<std::process::ExitStatus> {
    let mut child = Command::new(binary)
        .env(CENSUS_OUT_ENV, out)
        .env_remove(ENTRY_SPIN_ENV)
        .spawn()
        .wrap_err_with(|| format!("cannot run `{}`", binary.display()))?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            // The STATUS goes back to the caller, which decides: a non-zero
            // exit that still wrote a census is an incomplete census (issue
            // 1419), and one that wrote nothing is a failed run.
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    bail!(
                        "census run of `{}` did not finish within {} s. A census constructs \
                         components and exits; a run that does not is a constructor that \
                         blocks, and the census of that node is `unobserved`.",
                        binary.display(),
                        timeout.as_secs()
                    );
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => bail!("waiting for `{}`: {e}", binary.display()),
        }
    }
}

/// The two layouts a configured workspace puts a native entry in, plus the
/// entry's own directory name.
///
/// A SEARCH and not a guess: the refusal below names every path tried, because
/// "no such binary" with no list is the diagnostic that sends a person reading
/// cmake.
fn locate_entry_binary(build: &Path, entry: &str) -> Result<PathBuf> {
    let mut candidates = vec![
        build.join("src").join(entry).join(entry),
        build.join(entry).join(entry),
        build.join(entry),
    ];
    for c in &candidates {
        if c.is_file() {
            return Ok(c.clone());
        }
    }
    // Issue 1419 -- the layout `nros build` writes: one generated root per
    // image, `<build>/<root>/cmake/<entry>` (`build/posix-zenoh-native/cmake/
    // native_entry`). The three spellings above are a hand-configured
    // workspace's; with only those, `run --entry native_entry` refused on every
    // workspace `nros build` had built. A SEARCH over the roots that exist, and
    // an ambiguity is a refusal naming each match rather than a pick.
    let mut roots: Vec<PathBuf> = std::fs::read_dir(build)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    roots.sort();
    let found: Vec<PathBuf> = roots
        .iter()
        .map(|r| r.join("cmake").join(entry))
        .filter(|c| c.is_file())
        .collect();
    match found.as_slice() {
        [one] => return Ok(one.clone()),
        [] => candidates.push(build.join("<image-root>").join("cmake").join(entry)),
        many => bail!(
            "entry `{entry}` names a binary in {} image roots, so which one to census cannot \
             be decided here:\n{}\nName it with `--binary` or `--build-dir`.",
            many.len(),
            many.iter()
                .map(|c| format!("  {}", c.display()))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
    bail!(
        "no native binary for entry `{entry}`. Tried:\n{}\n\
         Build the workspace for `DEPLOY native` first (the census binary IS the boot \
         binary -- there is no separate census build), or name it with `--binary`.",
        candidates
            .iter()
            .map(|c| format!("  {}", c.display()))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn file_digest(path: &Path) -> Result<String> {
    let bytes =
        std::fs::read(path).wrap_err_with(|| format!("cannot read `{}`", path.display()))?;
    Ok(format!("sha256:{:x}", Sha256::digest(&bytes)))
}

/// Everything the census was derived FROM, each with its digest.
///
/// The binary is first because it is the one input that is sufficient on its
/// own: two runs of the same binary produce the same census. The rest are what
/// a reader needs to decide whether that binary is still the right one to have
/// run -- the sources it was built from, and the model the entry was generated
/// from with the model's own recorded inputs carried through.
fn collect_inputs(ws: &Path, binary: &Path, model: Option<&Path>) -> Result<Vec<Input>> {
    let rel = |p: &Path| -> String {
        p.strip_prefix(ws)
            .unwrap_or(p)
            .to_string_lossy()
            .into_owned()
    };
    let mut inputs = vec![Input {
        role: "binary",
        path: rel(binary),
        digest: file_digest(binary)?,
    }];

    let Some(model_path) = model else {
        return Ok(inputs);
    };

    // phase-460 W1 -- the ONE gate, before the read. A refused or stale model
    // must not become provenance: a census stamped with it would claim an
    // ancestry the artifact does not have.
    crate::model_gate::verify(model_path, None)
        .map_err(|r| eyre::eyre!("{r}"))
        .wrap_err("the model this census would be stamped with")?;
    if !model_path.is_file() {
        bail!("--model `{}` does not exist", model_path.display());
    }
    inputs.push(Input {
        role: "model",
        path: rel(model_path),
        digest: file_digest(model_path)?,
    });

    let raw = std::fs::read_to_string(model_path)
        .wrap_err_with(|| format!("cannot read `{}`", model_path.display()))?;
    let Ok(system) = ros_launch_manifest_model::SystemModel::from_yaml_str(&raw) else {
        // Not fatal: the census is the binary's, and a model this reader
        // cannot parse costs provenance, not correctness.
        return Ok(inputs);
    };

    // The model's own recorded inputs, carried through rather than re-derived
    // (RFC-0063: a derived artifact carries its inputs' digests, and the model
    // is itself derived).
    let bringup = crate::model_gate::infer_bringup_dir(model_path);
    for input in &system.meta.inputs {
        let path = match &bringup {
            Some(dir) => rel(&dir.join(&input.path)),
            None => input.path.clone(),
        };
        inputs.push(Input {
            role: "model_input",
            path,
            digest: format!("sha256:{}", input.sha256),
        });
    }

    // One digest per COMPONENT SOURCE TREE: the packages this model's nodes
    // come from, and no others. A workspace-wide digest would make every
    // census stale on an unrelated package's edit.
    let packages: BTreeSet<&str> = system
        .structure
        .nodes
        .values()
        .filter_map(|n| n.pkg.as_deref())
        .collect();
    if !packages.is_empty()
        && let Ok(discovered) = crate::orchestration::workspace::Workspace::discover(ws)
    {
        for pkg in packages {
            let Some(found) = discovered.packages.iter().find(|p| p.name == pkg) else {
                continue;
            };
            let Ok(digest) = crate::orchestration::metadata_refresh::source_digest(&found.root)
            else {
                continue;
            };
            inputs.push(Input {
                role: "source_tree",
                path: rel(&found.root),
                digest,
            });
        }
    }

    Ok(inputs)
}

/// The one-line count a run prints.
///
/// The acceptance of this wave is a number per kind, so the verb says the
/// numbers rather than "wrote a file". The field names are the recorder's own
/// (`services` for the server half, `timers` carrying guard conditions under
/// their own kind); parameters are a TOP-LEVEL array keyed by node, not a
/// per-node one, which is where a reader that assumed symmetry would go wrong.
fn summarise(recorded: &serde_json::Value) -> String {
    let nodes = recorded.get("nodes").and_then(|n| n.as_array());
    let per_node = |key: &str| -> usize {
        nodes
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|n| n.get(key).and_then(|v| v.as_array()))
                    .map(|a| a.len())
                    .sum()
            })
            .unwrap_or(0)
    };
    let parameters = recorded
        .get("parameters")
        .and_then(|p| p.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    format!(
        "{} node(s), {} sub / {} pub / {} service server / {} service client / {} timer \
         slot(s) / {} parameter(s)",
        nodes.map(|a| a.len()).unwrap_or(0),
        per_node("subscribers"),
        per_node("publishers"),
        per_node("services"),
        per_node("service_clients"),
        per_node("timers"),
        parameters,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_binary_names_every_path_it_tried() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = locate_entry_binary(dir.path(), "island_entry")
            .expect_err("no binary was built")
            .to_string();
        assert!(err.contains("src/island_entry/island_entry"), "{err}");
        assert!(err.contains("--binary"), "{err}");
    }

    /// Issue 1419 -- the census of a model is keyed by the MODEL, so the
    /// native entry that writes it and the cross configure that reads it name
    /// one file although they are different entries in different build dirs.
    #[test]
    fn the_census_of_a_model_lives_beside_the_model_whoever_asks() {
        let model = Path::new("/ws/build/nros/models/demo_bringup/system_model.yaml");
        assert_eq!(
            census_path_for_model(model),
            Path::new("/ws/build/nros/models/demo_bringup/system_model.census.json")
        );
        // Two launch files of one bringup are two models and two censuses.
        let other = Path::new("/ws/build/nros/models/demo_bringup/service_server_model.yaml");
        assert_ne!(census_path_for_model(model), census_path_for_model(other));
    }

    /// Issue 1419 -- `nros build`'s layout, `<build>/<root>/cmake/<entry>`,
    /// is found; two roots holding one entry name is refused, not picked.
    #[test]
    fn an_nros_build_image_root_is_searched_and_an_ambiguity_refuses() {
        let dir = tempfile::tempdir().expect("tempdir");
        let native = dir.path().join("posix-zenoh-native").join("cmake");
        std::fs::create_dir_all(&native).unwrap();
        std::fs::write(native.join("native_entry"), b"").unwrap();
        std::fs::create_dir_all(dir.path().join("threadx-linux-zenoh").join("cmake")).unwrap();
        assert_eq!(
            locate_entry_binary(dir.path(), "native_entry").expect("found"),
            native.join("native_entry")
        );

        let second = dir.path().join("posix-cyclonedds-native").join("cmake");
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(second.join("native_entry"), b"").unwrap();
        let err = locate_entry_binary(dir.path(), "native_entry")
            .expect_err("two roots, one name")
            .to_string();
        assert!(err.contains("2 image roots"), "{err}");
        assert!(err.contains("posix-zenoh-native"), "{err}");
        assert!(err.contains("posix-cyclonedds-native"), "{err}");
    }

    /// A contract and a census that agree: one talker, one publisher on
    /// `/chatter`, no timer. Written into `dir`; returns (contract, census).
    fn agreeing_pair(dir: &Path, incomplete: Option<&str>) -> (PathBuf, PathBuf) {
        let contract = dir.join("system.contract.yaml");
        std::fs::write(
            &contract,
            "version: 1\nnodes:\n  talker:\n    pub:\n      chatter: {}\ntopics:\n  /chatter:\n    \
             type: std_msgs/msg/Int32\n    pub: [talker/chatter]\n",
        )
        .unwrap();
        let mut census = serde_json::json!({
            "schema": CENSUS_SCHEMA,
            "entry": "native_entry",
            "provenance": { "tool": "test", "inputs": [] },
            "census": {
                "version": 3,
                "nodes": [{
                    "id": "talker",
                    "unresolved_name": { "value": "talker", "kind": "relative" },
                    "namespace": null,
                    "publishers": [{
                        "id": "/chatter",
                        "unresolved_topic": { "value": "/chatter", "kind": "absolute" },
                        "interface": { "package": "std_msgs", "name": "msg/Int32", "kind": "message" },
                    }],
                    "subscribers": [], "services": [], "service_clients": [],
                    "actions": [], "action_clients": [], "timers": [],
                }],
                "parameters": [],
            },
        });
        if let Some(why) = incomplete {
            census[INCOMPLETE_KEY] = serde_json::Value::String(why.to_string());
        }
        let path = dir.join("system_model.census.json");
        std::fs::write(&path, census.to_string()).unwrap();
        (contract, path)
    }

    fn check_args(contract: &Path, census: &Path) -> CheckArgs {
        CheckArgs {
            census: Some(census.to_path_buf()),
            contract: Some(contract.to_path_buf()),
            inventory: None,
            system_toml: None,
            model: None,
            strict: true,
            require_fresh: false,
            workspace: None,
            entry: None,
        }
    }

    /// Issue 1419 -- the negative control for the test below: the same pair,
    /// complete, passes.
    #[test]
    fn a_complete_census_that_agrees_with_its_contract_passes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (contract, census) = agreeing_pair(dir.path(), None);
        check_census(check_args(&contract, &census)).expect("contract states the code");
    }

    /// Issue 1419 -- a census whose run stopped early refuses even when every
    /// row it holds is confirmed: nothing after the failure was observed, so a
    /// partial observation cannot confirm that the contract is complete.
    #[test]
    fn an_incomplete_census_refuses_whatever_its_rows_say() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (contract, census) =
            agreeing_pair(dir.path(), Some("the census run exited exit status: 250"));
        let err = check_census(check_args(&contract, &census))
            .expect_err("an incomplete census confirms nothing")
            .to_string();
        assert!(err.contains("census INCOMPLETE"), "{err}");
        assert!(err.contains("exit status: 250"), "{err}");
    }

    /// Issue 1419 -- with no `--census`, the check reads the census of
    /// `--model`, which is where `run --model` writes it; with neither, it
    /// refuses rather than guess a path.
    #[test]
    fn with_no_census_named_the_check_needs_a_model_to_find_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (contract, _) = agreeing_pair(dir.path(), None);
        let mut args = check_args(&contract, Path::new("unused"));
        args.census = None;
        let err = check_census(args)
            .expect_err("nothing names a census")
            .to_string();
        assert!(err.contains("no --census and no --model"), "{err}");
    }

    /// Issue 1419 -- the remedy is ONE pasteable command for the image asking.
    /// A generated entry is `<image>_entry` and the model sits under its
    /// bringup, so both halves of `<bringup>:<image>` are known here.
    #[test]
    fn the_remedy_names_take_for_the_image_asking() {
        let dir = tempfile::tempdir().unwrap();
        let bringup = dir.path().join("src/demo_bringup");
        std::fs::create_dir_all(bringup.join("config")).unwrap();
        std::fs::write(bringup.join("system.toml"), "").unwrap();
        let model = bringup.join("config/system_model.yaml");
        let remedy = super::census_remedy(Some("threadx_entry"), Some(&model));
        assert!(
            remedy.contains("nros ws entity-census take --image demo_bringup:threadx"),
            "{remedy}"
        );
        // Neither half known: still the verb, with the shape spelled out.
        let bare = super::census_remedy(None, None);
        assert!(
            bare.contains("entity-census take --image <bringup>:"),
            "{bare}"
        );
    }

    /// Issue 1419 -- `take` picks the census producer by MODEL and by board
    /// PLATFORM: a host image of the same bringup resolved from the same
    /// launch and arguments, the first by id when several share it, and never
    /// a host image of another launch.
    #[test]
    fn take_picks_the_first_host_sibling_of_the_same_model() {
        use crate::orchestration::image::ImageBlock;

        let dir = tempfile::tempdir().unwrap();
        let bringup = dir.path().join("demo_bringup");
        std::fs::create_dir_all(&bringup).unwrap();
        std::fs::write(
            bringup.join("system.toml"),
            "[[model]]\nlaunch = \"multihost.launch.xml\"\nout = \"robot1_model.yaml\"\nargs = { host = \"robot1\" }\n",
        )
        .unwrap();
        let img = |board: &str, launch: Option<&str>, args: &[(&str, &str)]| ImageBlock {
            board: Some(board.to_string()),
            launch: launch.map(str::to_string),
            args: args
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..ImageBlock::default()
        };
        let row = |id: &str, block: ImageBlock| {
            (
                "demo_bringup".to_string(),
                bringup.clone(),
                id.to_string(),
                block,
            )
        };
        // Sorted by id, as `plan::all_images` returns them.
        let images = vec![
            row("native", img("native", None, &[])),
            row("native_cyclonedds", img("native", None, &[])),
            row(
                "native_robot1",
                img(
                    "native",
                    Some("multihost.launch.xml"),
                    &[("host", "robot1")],
                ),
            ),
            row("zephyr", img("native_sim/native/64", None, &[])),
        ];
        let is_host = |b: &ImageBlock| b.board.as_deref() == Some("native");

        let pick = |model_rel: &str| {
            super::census_producer_for(&images, "demo_bringup", model_rel, &is_host)
                .map(|(_, _, id, _)| id.as_str())
        };
        assert_eq!(pick("config/system_model.yaml"), Some("native"));
        assert_eq!(pick("config/robot1_model.yaml"), Some("native_robot1"));
        assert_eq!(pick("config/nobody_model.yaml"), None);
        assert_eq!(
            super::census_producer_for(
                &images,
                "other_bringup",
                "config/system_model.yaml",
                &is_host
            )
            .map(|(_, _, id, _)| id.as_str()),
            None,
            "a sibling is in the SAME bringup"
        );
    }

    #[test]
    fn the_summary_counts_across_nodes_and_kinds() {
        let recorded = serde_json::json!({
            "version": 2,
            "nodes": [
                {
                    "id": "one",
                    "subscribers": [{"id": "a"}, {"id": "b"}],
                    "publishers": [{"id": "c"}],
                    "timers": [{"kind": "wall"}, {"kind": "guard_condition"}],
                    "services": [],
                },
                {
                    "id": "two",
                    "subscribers": [{"id": "d"}],
                    "publishers": [],
                    "services": [{"id": "e"}],
                },
            ],
            // Top level, keyed by node -- the recorder's shape, not a
            // per-node array.
            "parameters": [
                {"node": "one", "name": "rate"},
                {"node": "two", "name": "x"},
                {"node": "two", "name": "y"},
            ],
        });
        let line = summarise(&recorded);
        assert!(line.contains("2 node(s)"), "{line}");
        assert!(line.contains("3 sub"), "{line}");
        assert!(line.contains("1 pub"), "{line}");
        assert!(line.contains("1 service server"), "{line}");
        assert!(line.contains("2 timer slot(s)"), "{line}");
        assert!(line.contains("3 parameter(s)"), "{line}");
    }
}
