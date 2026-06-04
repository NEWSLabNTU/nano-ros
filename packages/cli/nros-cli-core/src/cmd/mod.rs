//! Subcommand dispatch surface.
//!
//! Each verb lives in its own submodule and exposes:
//!   * a clap `Args` struct (when the verb takes options)
//!   * a `run(args) -> Result<()>` function
//!
//! `Cmd` is the clap-derived enum the binary front-ends parse into;
//! [`crate::run`] dispatches it.

use clap::Subcommand;

/// Phase 222.B.2 — env opt-out for the deprecated-verb stderr warning.
///
/// CI lanes that still drive `nros build` / `run` / `deploy` / `monitor` / `launch`
/// (e.g. transitional smoke tests that haven't migrated to the native
/// platform tool yet) can set `NROS_SUPPRESS_DEPRECATION=1` to silence
/// the one-line warning. The verb still runs; the deprecation signal
/// just stops appearing in build logs. The env name MUST match across
/// all four verbs — same opt-out, same noise gate.
pub const SUPPRESS_DEPRECATION_ENV: &str = "NROS_SUPPRESS_DEPRECATION";

/// Phase 222.B.2 — emit the per-verb deprecation warning to stderr unless
/// `NROS_SUPPRESS_DEPRECATION=1` is set. Each of the four deprecated
/// verbs (`build` / `run` / `deploy` / `monitor` / `launch`) calls this at the top of
/// its `run()` body, then continues with the existing wrapper logic so
/// user scripts don't break mid-stream. Deletion is Phase 222.C
/// (nros 0.5.0).
pub fn emit_deprecation_warning(verb: &str, replacement: &str) {
    if std::env::var_os(SUPPRESS_DEPRECATION_ENV).is_some_and(|v| v == "1") {
        return;
    }
    eprintln!(
        "warning: `nros {verb}` is deprecated and will be removed in nros 0.5.0. \
         Use {replacement} instead. Set {SUPPRESS_DEPRECATION_ENV}=1 to silence this warning. \
         See Phase 222 for rationale: \
         https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/phase-222-cli-surface-and-ux.md"
    );
}

pub mod board;
pub mod bringup;
pub mod build;
pub mod check;
pub mod check_workspace;
pub mod codegen;
pub mod codegen_cyclonedds_descriptors;
pub mod codegen_system;
pub mod completions;
pub mod config;
pub mod deploy;
pub mod doctor;
pub mod emit_package_xml;
pub mod explain;
pub mod generate;
pub mod launch;
pub mod metadata;
pub mod migrate;
pub mod monitor;
pub mod new;
pub mod new_system;
pub mod plan;
pub mod run_target;
pub mod scaffold_deploy;
pub mod setup;
pub mod version;
pub mod ws;

#[cfg(feature = "release")]
pub mod release;

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Scaffold a new nano-ros project (talker / listener / service / action)
    New(new::Args),

    /// Generate Rust / C / C++ message bindings from `package.xml`
    Generate(generate::Args),

    /// Generate Rust message bindings from `package.xml`
    #[command(name = "generate-rust")]
    GenerateRust(generate::RustArgs),

    /// Build-tool C/C++ binding generation (`--args-file` / `resolve-deps`).
    /// The interface the cmake / build.rs consumers speak (Phase 195.A — folds
    /// in the former standalone `nros-codegen` binary).
    Codegen(codegen::Args),

    /// Host-time system bake (Phase 212.E) — read `<bringup>/system.toml` +
    /// `<bringup>/launch/system.launch.xml` and emit the baked compile-time
    /// C config + component registration glue consumed by every embedded
    /// RTOS adapter.
    #[command(name = "codegen-system")]
    CodegenSystem(codegen_system::Args),

    /// Collect component source metadata for orchestration planning
    Metadata(metadata::Args),

    /// Migrate a pre-212 workspace to the new shape (Phase 212.I).
    ///
    /// Hidden from `nros --help`: this is an internal maintainer tool
    /// that runs once per pre-212 workspace and retires. End users start
    /// from the post-212 shape (`nros new system <bringup>`) and never
    /// touch this verb. Kept callable via `cargo run -p nros-cli` for
    /// the in-tree fixture sweep.
    #[command(subcommand, hide = true)]
    Migrate(MigrateSub),

    /// Resolve launch files, manifests, and metadata into nros-plan.json
    Plan(plan::Args),

    /// Validate a generated nros-plan.json
    Check(check::Args),

    /// Render a generated nros-plan.json in human-readable form
    Explain(explain::Args),

    /// Inspect or validate the current project's resolved configuration
    #[command(subcommand)]
    Config(config::Args),

    /// Build the current project (auto-detects cargo / cmake / west)
    /// (deprecated — see cargo build / cmake --build / west build / idf.py build; will be removed in nros 0.5.0)
    Build(build::Args),

    /// Run a [deploy.<name>] target from the root nros.toml (Phase 172 WP-A)
    /// (deprecated — see the platform's native flash+run combo (west flash, idf.py flash, probe-rs run, …); will be removed in nros 0.5.0)
    Deploy(deploy::Args),

    /// Spawn a bringup pkg's components on the host (deprecated —
    /// composed Entry pkg IS the launch product; use `cargo run -p
    /// <entry_pkg>`; will be removed in nros 0.5.0)
    ///
    /// Phase 212.N locked the Entry pkg shape: Node pkg libs are
    /// FUSED into one Entry binary at link time, the same way ROS 2's
    /// modern composable-node pattern fuses them into one container.
    /// The single Entry binary IS the launch product. `nros launch`'s
    /// one-process-per-`[[component]]` model fights that — it tries
    /// to split the fused binary back apart, and breaks against any
    /// Bringup pkg whose Entry pkg actually composes the Node pkgs.
    /// The Phase 222.D decision is to delete the verb in 0.5.0 and
    /// direct users at `cargo run -p <entry_pkg>` instead.
    ///
    /// Multi-Entry / mixed-host orchestration (codegen a per-Bringup
    /// `launch.sh`) waits for real demand in a follow-on phase.
    Launch(launch::Args),

    /// Resolve + fetch a board's toolchain/SDK packages (Phase 187)
    Setup(setup::Args),

    /// Build, flash, and monitor the current project on the selected target
    /// (deprecated — see cargo run / west <runner> run / probe-rs run / idf.py monitor; will be removed in nros 0.5.0)
    #[command(name = "run")]
    Run(run_target::Args),

    /// Attach to a running target's serial / RTT / semihosting output
    /// (deprecated — see probe-rs attach / idf.py monitor / picocom; will be removed in nros 0.5.0)
    Monitor(monitor::Args),

    /// Health-check the workspace (SDK paths, toolchains, env)
    Doctor(doctor::Args),

    /// Inspect supported boards
    #[command(subcommand)]
    Board(board::Args),

    /// Workspace-level msg-pkg utilities (Phase 210.B.3 + 210.D.1 — env, sync, …).
    Ws(ws::Args),

    /// Print toolchain + library versions
    Version,

    /// Generate shell completions (bash | zsh | fish | powershell)
    Completions(completions::Args),

    /// Maintainer-only release subcommands (hidden unless built with
    /// `--features release`)
    #[cfg(feature = "release")]
    #[command(subcommand)]
    Release(release::Args),
}

#[derive(Debug, Subcommand)]
pub enum MigrateSub {
    /// Migrate a pre-212 workspace (`nros.toml` + `component_nros.toml` +
    /// committed `metadata/*.json`) to the post-212 layout.
    Workspace(migrate::Args),
}
