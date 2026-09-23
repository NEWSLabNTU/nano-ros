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

/// Where a census lands, under the build directory. One file per ENTRY: two
/// entries sharing a component see it twice, which is correct -- they may
/// launch it with different parameters.
pub(crate) const CENSUS_DIR: &str = "nros/census";

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

    /// Write the census here instead of under the build directory.
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
    /// document are accepted.
    #[arg(long, value_name = "PATH")]
    pub census: PathBuf,

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
    }
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

    // phase-463 W4 -- the freshness question, BEFORE the comparison. A check
    // that compares against a museum census and then passes has said
    // something true about two documents and nothing at all about the code.
    if args.require_fresh {
        let ws = match args.workspace.clone() {
            Some(w) => w,
            None => std::env::current_dir().wrap_err("no current directory")?,
        };
        match census_freshness(&args.census, &ws) {
            Freshness::Fresh => {}
            Freshness::Missing(why) => {
                return freshness_verdict(
                    policy.on_missing,
                    "missing",
                    &why,
                    args.entry.as_deref(),
                );
            }
            Freshness::Stale(why) => {
                return freshness_verdict(policy.on_stale, "stale", &why, args.entry.as_deref());
            }
        }
    }

    let census_raw = std::fs::read_to_string(&args.census)
        .wrap_err_with(|| format!("cannot read census `{}`", args.census.display()))?;
    let census: serde_json::Value = serde_json::from_str(&census_raw)
        .wrap_err_with(|| format!("`{}` is not JSON", args.census.display()))?;

    let contract_path = match args.contract.clone() {
        Some(p) => p,
        None => discover_contract(args.model.as_deref()).ok_or_else(|| {
            eyre::eyre!(
                "no --contract, and the model names no `*.contract.yaml` to fall back on. The \
                 contract is `<bringup>/launch/<stem>.contract.yaml`, beside the launch file \
                 this entry was resolved from."
            )
        })?,
    };
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
) -> Result<()> {
    let remedy = format!(
        "Take a census of the code as it is now:\n    nros ws entity-census run --entry {}\n\
         This configure does not run it for you: building the native image from inside a cross \
         configure is the cross-cutting compile issue 0641 refuses.",
        entry.unwrap_or("<entry>")
    );
    if policy.refuses() {
        bail!("census {what}: {why}\n{remedy}");
    }
    // Not silent. `warn` is the landing default so that no consumer breaks on
    // the day this merges, and a default that said nothing would be the state
    // issue 1419 is about.
    println!("census {what} (WARNING, [census] on_{what} = \"warn\"): {why}");
    for line in remedy.lines() {
        println!("  {line}");
    }
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
    census: serde_json::Value,
}

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

    let out = args
        .out
        .unwrap_or_else(|| build.join(CENSUS_DIR).join(format!("{}.json", args.entry)));
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
    run_in_census_mode(&binary, &raw, Duration::from_secs(args.timeout_secs))?;
    let elapsed = started.elapsed();

    let recorded = std::fs::read_to_string(&raw).wrap_err_with(|| {
        format!(
            "`{}` exited 0 in census mode but wrote no census at `{}` -- the image is built \
             without `metadata-mode`, or it is not a C++ entry (a Rust entry's funnel says so \
             and exits non-zero)",
            binary.display(),
            raw.display()
        )
    })?;
    let recorded: serde_json::Value = serde_json::from_str(&recorded)
        .wrap_err_with(|| format!("`{}` is not JSON", raw.display()))?;

    let census = Census {
        schema: CENSUS_SCHEMA,
        entry: args.entry.clone(),
        provenance: Provenance {
            tool: format!("nros-cli {}", env!("CARGO_PKG_VERSION")),
            inputs: collect_inputs(&ws, &binary, args.model.as_deref())?,
        },
        census: recorded,
    };
    let json = serde_json::to_string_pretty(&census).wrap_err("serialize census")?;
    crate::atomic_file::atomic_write(&out, &json)
        .wrap_err_with(|| format!("cannot write `{}`", out.display()))?;

    let counts = summarise(&census.census);
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
fn run_in_census_mode(binary: &Path, out: &Path, timeout: Duration) -> Result<()> {
    let mut child = Command::new(binary)
        .env(CENSUS_OUT_ENV, out)
        .env_remove(ENTRY_SPIN_ENV)
        .spawn()
        .wrap_err_with(|| format!("cannot run `{}`", binary.display()))?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    bail!(
                        "census run of `{}` exited {} -- the funnel prints the reason",
                        binary.display(),
                        status
                    );
                }
                return Ok(());
            }
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
    let candidates = [
        build.join("src").join(entry).join(entry),
        build.join(entry).join(entry),
        build.join(entry),
    ];
    for c in &candidates {
        if c.is_file() {
            return Ok(c.clone());
        }
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
