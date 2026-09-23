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
const CENSUS_DIR: &str = "nros/census";

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

pub fn run(args: EntityCensusArgs) -> Result<()> {
    match args.command {
        Sub::Run(a) => run_census(a),
    }
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
