//! `nros-launch-resolve` — nano-ros's dedicated launch-resolution helper.
//!
//! Resolves a ROS 2 launch tree into a `SystemModel` YAML (RFC-0050), which
//! `nros sync` bakes into the generated entry.
//!
//! # Why this exists as its own binary (issue 0285)
//!
//! `nros sync` used to shell out to `play_launch resolve`, locating the tool
//! by BARE NAME through PATH. `play_launch` is also the name of an unrelated
//! ROS 2 record/replay tool, so on a machine that had that one installed, the
//! wrong binary won and every platform's fixture build failed with a clap error
//! from inside a cmake configure.
//!
//! Two things fix that, and this binary is both:
//!
//! 1. **A distinct name.** `nros-launch-resolve` cannot be confused with
//!    anything else. We cannot be shadowed, and — just as important — we never
//!    shadow a user's real `play_launch`, which would silently break their
//!    workflow.
//! 2. **A pinned build.** It compiles from the `ros-launch-resolve` submodule
//!    at a revision this repo records, versioned alongside `nros-cli`, so the
//!    CLI and the resolver can no longer drift apart.
//!
//! # Why a separate process at all
//!
//! Launch resolution has to support `.launch.py`, which requires executing
//! Python against the *user's* interpreter. That cannot be statically linked
//! into the portable `nros` binary (pyo3's `auto-initialize` embeds CPython and
//! pins libpython). Keeping it in a helper built on the user's machine is what
//! lets `nros` stay a libc-only binary — the constraint recorded in
//! `nros-cli-core/Cargo.toml` (phase-195.A) and revisited in RFC-0059.
//!
//! Since RFC-0060 the resolver is its own repository (layer 2), so "no rclrs,
//! no colcon-generated messages" is a property of the package graph rather
//! than of a `default-features = false` flag. Only CPython is required.

use clap::Parser;
use eyre::Result;
use ros_launch_resolve::{
    model::{ModelBuildInputs, build_checked_model},
    ros::launch_dump::LaunchDump,
    verbs::parse_launch_file,
};

#[derive(Parser)]
#[command(
    name = "nros-launch-resolve",
    about = "Resolve a ROS 2 launch tree into a nano-ros SystemModel YAML",
    long_about = None,
    // issue 0409 — `--version` reports the crate version AND the play_launch
    // commit this binary compiled in. The version alone cannot distinguish a
    // stale resolver from a current one (both are versioned in lockstep with
    // the CLI, both read the same number); the pin is what actually differs,
    // and `nros sync` compares it against its own before trusting the binary.
    version = concat!(env!("CARGO_PKG_VERSION"), " (play_launch ", env!("NROS_PLAY_LAUNCH_SHA"), ")"),
)]
struct Cli {
    /// Path to the launch file.
    ///
    /// (The package-name form the original `play_launch resolve` accepted was
    /// never wired through this thin main — and its `launch_file` positional
    /// silently swallowed the first `KEY:=VALUE` binding, so `host:=robot1`
    /// resolved the default configuration. Path-only, validated bindings.)
    package_or_path: String,

    /// Launch arguments, `KEY:=VALUE`.
    launch_arguments: Vec<String>,

    /// Issue 0320 — the bringup package root that `meta.inputs[].path` are
    /// recorded relative to. When omitted, falls back to the launch file's
    /// grandparent. Pass it to make model portability structural.
    #[arg(long, value_name = "PATH")]
    bringup_root: Option<std::path::PathBuf>,

    /// The integrator `system.toml` (deploy placement, transports, domain).
    #[arg(long, value_name = "system.toml")]
    system: Option<std::path::PathBuf>,

    /// Scheduling platform file (`.yaml`, or legacy `.toml`).
    #[arg(long)]
    sched: Option<std::path::PathBuf>,

    /// Overlay root for user-supplied contracts.
    #[arg(long, value_name = "PATH")]
    contracts: Option<std::path::PathBuf>,

    /// Disable the provider-sidecar contract channel.
    #[arg(long)]
    no_provider_contracts: bool,

    /// Scheduling target the platform file must declare.
    #[arg(long, default_value = "posix")]
    target: String,

    /// Output path for the SystemModel YAML. `-` writes to stdout.
    #[arg(long, short = 'o', default_value = "system_model.yaml")]
    out: String,

    /// Use the Python launch parser (maximum ROS compatibility) instead of the
    /// Rust one. Required for launch trees that `.launch.py` or `$(eval …)`.
    #[arg(long)]
    python_parser: bool,

    /// Print the merged scheduling plan with provenance per node.
    #[arg(long)]
    explain: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Delegate to the pinned pipeline. A thin translation rather than a
    // reimplementation, deliberately: the resolver is ~12k lines and a second
    // copy would drift — the failure issue 0285 is about.
    let launch_path = std::path::PathBuf::from(&cli.package_or_path);
    // Every trailing positional must be a binding — a malformed one used to
    // be dropped by `filter_map`, which resolves the default configuration
    // while looking like success.
    let arg_binding: std::collections::HashMap<String, String> = cli
        .launch_arguments
        .iter()
        .map(|a| {
            a.split_once(":=")
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .ok_or_else(|| eyre::eyre!("launch argument `{a}` is not `KEY:=VALUE`"))
        })
        .collect::<Result<_>>()?;
    let record = if cli.python_parser {
        eyre::bail!(
            "--python-parser is not wired through the library entry point yet; \
             use the `ros-launch-resolve` binary from the submodule for now"
        )
    } else {
        // The binding must reach the PARSE, not only the model metadata:
        // `<arg>` defaults and `if=`/`unless=` conditions evaluate here, so
        // a `host:=robot1`-style override that stops short of the parser
        // silently resolves the default configuration (phase-326 found this
        // resolving the multihost per-host models — both nodes survived).
        // `verbs::parse_launch_file`, NOT `play_launch_parser`'s — the verb
        // wrapper is where the Python half is discovered and `dlopen`ed
        // (issue 0897 W3), and it is deliberately the ONE place that happens.
        //
        // This binary called the parser directly and so never reached it, which
        // is not a missing nicety: after W2b removed the compile-time libpython
        // link, nothing installed a backend here at all, and every `$(eval …)`
        // or `.launch.py` failed with "this build has no Python backend" — on
        // hosts that have Python. `host-tests` was red on main from that pin
        // until this line changed. A failure to load stays non-fatal, so a host
        // with no usable interpreter still resolves XML and YAML.
        parse_launch_file(&launch_path, arg_binding.clone())
            .map_err(|e| eyre::eyre!("parsing {}: {e}", launch_path.display()))?
    };
    let dump: LaunchDump = serde_json::from_str(&serde_json::to_string(&record)?)?;

    let model = build_checked_model(ModelBuildInputs {
        dump: &dump,
        launch_path: Some(&launch_path),
        bringup_root: cli.bringup_root.as_deref(),
        arg_binding: arg_binding.into_iter().collect(),
        contracts: cli.contracts.as_deref(),
        no_provider_contracts: cli.no_provider_contracts,
        sched: cli.sched.as_deref(),
        system: cli.system.as_deref(),
        target: cli.target.as_str(),
        explain: cli.explain,
    })?;

    let yaml = model.to_yaml_string()?;
    if cli.out == "-" {
        print!("{yaml}");
    } else {
        std::fs::write(&cli.out, &yaml)
            .map_err(|e| eyre::eyre!("writing SystemModel to {}: {e}", cli.out))?;
        eprintln!(
            "SystemModel: {} ({} nodes, {} topics, {} tier(s))",
            cli.out,
            model.structure.nodes.len(),
            model.structure.topics.len(),
            model.execution.tiers.len(),
        );
    }
    Ok(())
}
