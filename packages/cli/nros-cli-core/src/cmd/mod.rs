//! Subcommand dispatch surface.
//!
//! Each verb lives in its own submodule and exposes:
//!   * a clap `Args` struct (when the verb takes options)
//!   * a `run(args) -> Result<()>` function
//!
//! `Cmd` is the clap-derived enum the binary front-ends parse into;
//! [`crate::run`] dispatches it.

use clap::Subcommand;

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
pub mod generate_px4;
pub mod metadata;
pub mod migrate;
pub mod new;
pub mod new_system;
pub mod plan;
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

    /// Generate CDR `px4_msgs::msg::*` from a PX4-Autopilot `.msg` tree (no
    /// ament dep) for the XRCE companion path (Phase 233 / RFC-0039 Track B).
    #[command(name = "generate-px4-msgs")]
    GeneratePx4Msgs(generate_px4::Args),

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
