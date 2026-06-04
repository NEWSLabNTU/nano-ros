//! Shared library backing the `nros` CLI.
//!
//! `nros-cli` owns the canonical user-facing command surface. Codegen
//! implementation details still live in the `cargo_nano_ros` library until
//! that library is renamed or split.

pub mod cmd;
pub mod orchestration;

use eyre::Result;

/// Top-level dispatcher entry point — every binary front-end lands here.
///
/// `argv` is the post-clap parsed command structure. Each variant maps
/// 1:1 to a `nros <verb>` invocation.
pub fn run(cmd: cmd::Cmd) -> Result<()> {
    match cmd {
        cmd::Cmd::New(args) => cmd::new::run(args),
        cmd::Cmd::Generate(args) => cmd::generate::run(args),
        cmd::Cmd::GenerateRust(args) => cmd::generate::run_rust(args),
        cmd::Cmd::Codegen(args) => cmd::codegen::run(args),
        cmd::Cmd::CodegenSystem(args) => cmd::codegen_system::run(args),
        cmd::Cmd::Metadata(args) => cmd::metadata::run(args),
        cmd::Cmd::Migrate(sub) => match sub {
            cmd::MigrateSub::Workspace(args) => cmd::migrate::run(args),
        },
        cmd::Cmd::Plan(args) => cmd::plan::run(args),
        cmd::Cmd::Check(args) => cmd::check::run(args),
        cmd::Cmd::Explain(args) => cmd::explain::run(args),
        cmd::Cmd::Config(args) => cmd::config::run(args),
        cmd::Cmd::Build(args) => cmd::build::run(args),
        cmd::Cmd::Deploy(args) => cmd::deploy::run(args),
        cmd::Cmd::Launch(args) => cmd::launch::run(args),
        cmd::Cmd::Setup(args) => cmd::setup::run(args),
        cmd::Cmd::Run(args) => cmd::run_target::run(args),
        cmd::Cmd::Monitor(args) => cmd::monitor::run(args),
        cmd::Cmd::Doctor(args) => cmd::doctor::run(args),
        cmd::Cmd::Board(args) => cmd::board::run(args),
        cmd::Cmd::Ws(args) => cmd::ws::run(args),
        cmd::Cmd::Version => cmd::version::run(),
        cmd::Cmd::Completions(args) => cmd::completions::run(args),
        #[cfg(feature = "release")]
        cmd::Cmd::Release(args) => cmd::release::run(args),
    }
}
