//! `nros-launch-resolve` — nano-ros's dedicated launch-resolution helper.
//!
//! Resolves a ROS 2 launch tree into a `SystemModel` YAML (RFC-0050), which
//! `nros ws sync` bakes into the generated entry.
//!
//! # Why this exists as its own binary (issue 0285)
//!
//! `nros ws sync` used to shell out to `play_launch resolve`, locating the tool
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
//! 2. **A pinned build.** It compiles from the `play_launch` submodule at a
//!    revision this repo records, versioned alongside `nros-cli`, so the CLI
//!    and the resolver can no longer drift apart.
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
//! Linking `play_launch` with `default-features = false` drops `rclrs` and the
//! colcon-generated `play_launch_msgs`, so this needs no ROS, no ament and no
//! colcon — only CPython.

use clap::Parser;
use eyre::Result;
use play_launch::cli::options::{ParserBackend, ResolveArgs};

#[derive(Parser)]
#[command(
    name = "nros-launch-resolve",
    about = "Resolve a ROS 2 launch tree into a nano-ros SystemModel YAML",
    long_about = None,
)]
struct Cli {
    /// Package name, or a path to the launch file.
    package_or_path: String,

    /// Launch file name, when the first argument is a package name.
    launch_file: Option<String>,

    /// Launch arguments, `KEY:=VALUE`.
    launch_arguments: Vec<String>,

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

    // Delegate to the pinned resolve pipeline. Keeping this a thin translation
    // rather than a reimplementation is deliberate: the resolver is ~8k lines
    // and a second copy would drift, which is the failure this issue is about.
    let args = ResolveArgs {
        package_or_path: cli.package_or_path,
        launch_file: cli.launch_file,
        launch_arguments: cli.launch_arguments,
        contracts: cli.contracts,
        no_provider_contracts: cli.no_provider_contracts,
        sched: cli.sched,
        system: cli.system,
        target: cli.target,
        parser: if cli.python_parser {
            ParserBackend::Python
        } else {
            ParserBackend::Rust
        },
        out: cli.out,
        explain: cli.explain,
    };
    play_launch::commands::resolve::handle_resolve(&args)
}
