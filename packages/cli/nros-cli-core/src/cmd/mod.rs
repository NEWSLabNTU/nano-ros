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
pub mod check;
pub mod check_workspace;
pub mod codegen;
pub mod codegen_cyclonedds_descriptors;
pub mod codegen_system;
pub mod completions;
pub mod config;
pub mod doctor;
pub mod emit_package_xml;
pub mod explain;
pub mod generate;
pub mod metadata;
pub mod migrate;
pub mod new;
pub mod new_system;
pub mod plan;
pub mod scaffold_deploy;
pub mod setup;
pub mod version;
pub mod ws;
// Phase 222.C — build / run_target / deploy / monitor / launch
// modules deleted along with their `Cmd` variants. The historical
// `emit_deprecation_warning` helper above is retained because
// `cmd::doctor::match_deprecated_verb` still flags users' stale
// `nros build` / `nros launch` / … shell references after the
// deletion (those references error at clap-parse now; the doctor
// surface explains the migration).

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

    // Phase 222.C — `Build` / `Deploy` / `Launch` / `Run` / `Monitor`
    // variants deleted (deprecation soak complete in nros 0.4.x;
    // SemVer break visible via the 0.5.0 bundle bump). Users invoke
    // the platform tool directly:
    //   build  → cargo build / cmake --build / west build / idf.py build
    //   run    → cargo run -p <entry_pkg> / west <runner> run /
    //            probe-rs run / idf.py monitor
    //   deploy → the platform's native flash+run combo (west flash,
    //            idf.py flash, probe-rs run)
    //   monitor → probe-rs attach / idf.py monitor / picocom
    //   launch → `cargo run -p <entry_pkg>` (composed Entry pkg IS
    //            the launch product per Phase 212.N + 222.D)
    /// Resolve + fetch a board's toolchain/SDK packages (Phase 187)
    Setup(setup::Args),

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
