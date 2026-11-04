//! `nros sdk-path` — phase-365 W2: print the CONSTRUCTED install path of a
//! provisioned tool.
//!
//! nano-ros decides where a provisioned tool goes: `nros setup` writes
//! `<store>/<tool>/<version>` because `nros-sdk-index.toml` named that version.
//! The layout is our own OUTPUT, so a consumer constructs the path from the same
//! two inputs — it does not search the store for it.
//!
//! This is the bridge for the three non-Rust mechanisms (cmake, `just`, shell),
//! exactly as `nros model-path` is the bridge for the SystemModel rule: the
//! derivation lives ONCE, in `sdk_store::tool_dir`, and everyone else asks.
//!
//! Why asking beats searching, measured (issue 0625): a search can return
//! something we did not install (the legacy unversioned `corrosion/{lib,share}`
//! prefix), something a DIFFERENT project installed — the store is shared while
//! the pin is per-project — or nothing. On 2026-08-16, in a tree pinning
//! `corrosion 0.6.1-nros1`, one `lane=all` configure produced 155 resolutions of
//! 0.5.1 against 28 of 0.6.1, with the search's own ordering verified correct.
//!
//! `--require` is the difference between "where would it be" and "it is there":
//! a miss FAILS with the provisioning command instead of falling back to another
//! version, because a silent substitution is the bug this phase removes.

use std::path::PathBuf;

use clap::Parser;
use eyre::{Result, bail};

use crate::orchestration::{sdk_index::SdkIndex, sdk_store};

#[derive(Debug, Parser)]
pub struct Args {
    /// Tool name, as spelled by `[tool.<name>]` in the SDK index — or, with
    /// `--source`, a `[source.<name>]`.
    pub tool: String,

    /// Ask about a `[source.*]` instead of a `[tool.*]`.
    ///
    /// A source with `location = "store"` is provisioned to
    /// `$NROS_HOME/sources/<name>/<version>`, DERIVED from the index exactly
    /// as a tool's prefix is. A consumer that needs to find one must ask rather
    /// than spell it — the same rule this command enforces for tools.
    ///
    /// A `location = "workspace"` source has no store path, so this reports
    /// that rather than inventing one.
    // phase-440. The ask-don't-spell rule is the reason
    // `msg_to_cyclone_idl.py` no longer carries a literal
    // `third-party/ros/rosidl`.
    #[arg(long)]
    pub source: bool,

    /// Path to the SDK index.
    #[arg(long, default_value = "nros-sdk-index.toml")]
    pub index: PathBuf,

    /// Fail unless the directory exists — for a consumer about to use it.
    #[arg(long)]
    pub require: bool,
}

pub fn run(args: Args) -> Result<()> {
    let (_, index) = crate::cmd::setup::load_resolved_index(&args.index)?;

    if args.source {
        return run_source(&index, &args);
    }

    match tool_path(&index, &args.tool, args.require) {
        Ok(p) => {
            println!("{}", p.display());
            Ok(())
        }
        Err(ToolMiss::Unknown { known }) => bail!(
            "no `[tool.{}]` in {} — the index pins: {}",
            args.tool,
            args.index.display(),
            known.join(", ")
        ),
        Err(ToolMiss::ProjectNotDir(p)) => bail!(
            "[tools] {} = {} (the project's system.toml) is not a directory",
            args.tool,
            p.display()
        ),
        Err(ToolMiss::NotInstalled { version, tried }) => {
            // Name every path tried. "Not installed" and "installed where I did
            // not look" printed the same line before, and they need different
            // fixes.
            let tried: Vec<String> = tried.iter().map(|p| format!("  {}", p.display())).collect();
            bail!(
                "{} is pinned to {} but no install was found. Looked at:\n{}\n\
                 Provision it:  nros setup --tool {}\n\
                 (NOT substituting another version on purpose: the store is shared \
                 between projects while the pin is per-project, so a substitution \
                 silently gives this build another project's toolchain.)",
                args.tool,
                version,
                tried.join("\n"),
                args.tool
            )
        }
    }
}

/// Why [`tool_path`] has no answer.
#[derive(Debug)]
pub(crate) enum ToolMiss {
    /// The index has no `[tool.<name>]`; `known` is every tool it pins.
    Unknown { known: Vec<String> },
    /// The project's `[tools]` row names something that is not a directory.
    ProjectNotDir(PathBuf),
    /// Pinned, but neither constructed candidate is a directory.
    NotInstalled {
        version: String,
        tried: Vec<PathBuf>,
    },
}

/// Where a `[tool.<name>]` of the index is — THE answer `nros dev sdk path` and
/// `nros locate tool <name>` both print (RFC-0103 D3 for a tool row: the
/// project's `[tools]` row, then the store at the pin, then fail).
///
/// Constructed, never enumerated (issue 1546): the candidates are
/// `<store>/<tool>/<pinned version>` and the legacy flat prefix, both built
/// from the index. With `require`, a pinned tool that is not on disk is a miss;
/// without it the canonical path is answered even when absent, because the
/// caller asked where the tool GOES.
pub(crate) fn tool_path(
    index: &SdkIndex,
    tool: &str,
    require: bool,
) -> std::result::Result<PathBuf, ToolMiss> {
    let Some(entry) = index.tool.get(tool) else {
        return Err(ToolMiss::Unknown {
            known: index.tool.keys().cloned().collect(),
        });
    };

    // RFC-0103 D3 rung 3 — the project's own `[tools]` row, before the pin.
    if let Some(p) = crate::cmd::locate::project_dir(None)
        .and_then(|d| nros_build_paths::locate::project_entry(&d, "tools", tool))
    {
        if require && !p.is_dir() {
            return Err(ToolMiss::ProjectNotDir(p));
        }
        return Ok(p);
    }

    // Issue 0628 — answer with the layout that is actually PRESENT.
    //
    // `tool_dir` is where an installer WRITES; a consumer needs somewhere it can
    // read. Two provisioning paths wrote two shapes historically, so a host
    // provisioned before `install-corrosion` learned to ask this command has the
    // pinned version under the flat prefix. Constructing only the versioned one
    // made that host look unprovisioned, and cmake fetched from the network
    // while printing advice to install what was already installed.
    //
    // Still construction: two candidates from the same inputs, no enumeration.
    if let Some(p) = sdk_store::tool_dir_usable(index, tool) {
        return Ok(p);
    }
    if require {
        return Err(ToolMiss::NotInstalled {
            version: entry.version.clone(),
            tried: sdk_store::tool_dir_candidates(index, tool),
        });
    }
    // Without `require` this still answers the CANONICAL path when nothing is
    // installed: an empty answer is harder to act on than a path that does not
    // exist yet.
    sdk_store::tool_dir(index, tool).ok_or(ToolMiss::Unknown {
        known: index.tool.keys().cloned().collect(),
    })
}

/// `--source` — the store path of a `[source.*]`, phase-440.
///
/// Deliberately narrower than the tool arm: there is no legacy flat shape to
/// fall back to, because store sources are new, so "where it goes" and "where a
/// consumer reads it" are the same directory and one derivation answers both.
fn run_source(index: &SdkIndex, args: &Args) -> Result<()> {
    let Some(src) = index.source.get(&args.tool) else {
        let known: Vec<&str> = index.source.keys().map(String::as_str).collect();
        bail!(
            "no `[source.{}]` in {} — the index names: {}",
            args.tool,
            args.index.display(),
            known.join(", ")
        );
    };

    let Some(dir) = sdk_store::source_dir(index, &args.tool) else {
        // Not an error the caller can fix by provisioning: this source lives in
        // the workspace by declaration, so say WHICH declaration rather than
        // printing a store path nothing will ever write.
        bail!(
            "`[source.{}]` has location = \"workspace\" (dest = {}), so it has no \
             store path. Ask for its workspace path, or set location = \"store\" \
             in {}.",
            args.tool,
            src.dest.as_deref().unwrap_or("<unset>"),
            args.index.display()
        );
    };

    if args.require && !dir.is_dir() {
        bail!(
            "`[source.{}]` {} is not provisioned at {} — run: nros setup --source {}",
            args.tool,
            src.version,
            dir.display(),
            args.tool
        );
    }
    println!("{}", dir.display());
    Ok(())
}
