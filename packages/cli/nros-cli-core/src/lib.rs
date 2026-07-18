//! Shared library backing the `nros` CLI.
//!
//! `nros-cli` owns the canonical user-facing command surface. Codegen
//! implementation details still live in the `cargo_nano_ros` library until
//! that library is renamed or split.

pub mod abi_guard;
pub mod cmd;
// RFC-0052 / phase-296 R3 — legacy-bake deprecation notices (removed in R4).
pub mod deprecation;
// Phase 219.A — Entry-pkg codegen (`nros codegen entry`). The shared
// pkg-index walk + launch.xml parser also live here so the cmake-fn
// path (`nano_ros_entry(LAUNCH …)`), the Rust proc-macro
// (via `nros-build`'s re-export), and any future C/C++ tooling all
// dispatch through one implementation.
pub mod codegen;
// phase-262 W2 — `launch_parser` extracted to the `nros-launch-parser` leaf crate
// (depends only on nros-pkg-index, not the launch-manifest submodule). Re-exported
// so `nros_cli_core::launch_parser::*` consumers are unchanged.
pub use nros_launch_parser as launch_parser;
pub mod orchestration;
// phase-262 W1 — `pkg_index` extracted to the `nros-pkg-index` leaf crate so the
// nros-macros proc-macro path doesn't pull all of nros-cli-core. Re-exported here
// so every `nros_cli_core::pkg_index::*` consumer (+ `crate::pkg_index` internal
// refs, e.g. launch_parser) is unchanged.
pub use nros_pkg_index as pkg_index;

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
        cmd::Cmd::Sync(args) => cmd::ws::run_sync(args),
        cmd::Cmd::GeneratePx4Msgs(args) => cmd::generate_px4::run(args),
        cmd::Cmd::Codegen(args) => cmd::codegen::run(args),
        cmd::Cmd::CodegenSystem(args) => cmd::codegen_system::run(args),
        cmd::Cmd::Metadata(args) => cmd::metadata::run(args),
        cmd::Cmd::Plan(args) => cmd::plan::run(args),
        cmd::Cmd::Check(args) => cmd::check::run(args),
        cmd::Cmd::Explain(args) => cmd::explain::run(args),
        cmd::Cmd::Config(args) => cmd::config::run(args),
        cmd::Cmd::Setup(args) => cmd::setup::run(args),
        cmd::Cmd::Init(args) => cmd::init::run(args),
        cmd::Cmd::Doctor(args) => cmd::doctor::run(args),
        cmd::Cmd::Board(args) => cmd::board::run(args),
        cmd::Cmd::Ws(args) => cmd::ws::run(args),
        cmd::Cmd::Version => cmd::version::run(),
        cmd::Cmd::Completions(args) => cmd::completions::run(args),
        #[cfg(feature = "release")]
        cmd::Cmd::Release(args) => cmd::release::run(args),
    }
}
