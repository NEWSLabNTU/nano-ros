//! `nros generate <lang>` — message bindings codegen.
//!
//! Phase 111.A.5. Wraps the existing `cargo_nano_ros` library API so the
//! canonical `nros` command reuses the same codegen implementation.

use cargo_nano_ros::{
    GenerateCStandaloneConfig, GenerateConfig, generate_c_from_package_xml,
    generate_from_package_xml, parse_rename,
};
use clap::{Args as ClapArgs, ValueEnum};
use eyre::{Result, eyre};
use std::{collections::HashMap, path::PathBuf};

use crate::abi_guard::{self, Verb};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Lang {
    Rust,
    C,
    Cpp,
    /// Generate Rust + C + C++ in one shot
    All,
}

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Target language
    #[arg(value_enum)]
    pub lang: Lang,

    /// Path to `package.xml`
    #[arg(long, default_value = "package.xml")]
    pub manifest: PathBuf,

    /// Output directory for generated bindings
    #[arg(long, short = 'o', default_value = "generated")]
    pub output: PathBuf,

    /// ROS 2 edition (`humble` | `iron`)
    #[arg(long, default_value = "humble")]
    pub ros_edition: String,

    /// Explicit per-field capacity config (`nros-codegen.toml`, RFC-0033).
    /// Wins over any file discovered by walking up from the manifest dir.
    #[arg(long)]
    pub codegen_config: Option<PathBuf>,

    /// Overwrite existing bindings
    #[arg(long)]
    pub force: bool,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Generate `.cargo/config.toml` with `[patch.crates-io]` entries
    /// (Rust only)
    #[arg(long, alias = "config")]
    pub generate_config: bool,

    /// Path to nros crates directory for generated Cargo config patches
    /// (Rust only)
    #[arg(long, conflicts_with = "nano_ros_git")]
    pub nano_ros_path: Option<PathBuf>,

    /// Use nros git repository for generated Cargo config patches
    /// (Rust only)
    #[arg(long, conflicts_with = "nano_ros_path")]
    pub nano_ros_git: bool,

    /// Rename a generated package: --rename old_pkg=new_crate_name
    /// (Rust only)
    #[arg(long, value_parser = parse_rename)]
    pub rename: Vec<(String, String)>,

    /// Phase 212.K.7.1 (deprecated, no-op) — accepted for backwards
    /// compatibility with K.5 invocation sites. Generated msg crates
    /// are now RMW-agnostic; the `<pkg>/cyclonedds` Cargo feature is
    /// no longer emitted. Use `nros codegen cyclonedds-descriptors`
    /// for host-side descriptor C emit.
    #[arg(long, hide = true)]
    pub no_cyclonedds: bool,

    /// Phase 212.K.7.1 (deprecated, no-op) — see `--no-cyclonedds`.
    #[arg(long, hide = true)]
    pub cyclonedds_idlc: Option<PathBuf>,
}

#[derive(Debug, ClapArgs)]
pub struct RustArgs {
    /// Path to `package.xml`
    #[arg(long, default_value = "package.xml")]
    pub manifest: PathBuf,

    /// Output directory for generated bindings
    #[arg(long, short = 'o', default_value = "generated")]
    pub output: PathBuf,

    /// ROS 2 edition (`humble` | `iron`)
    #[arg(long, default_value = "humble")]
    pub ros_edition: String,

    /// Explicit per-field capacity config (`nros-codegen.toml`, RFC-0033).
    /// Wins over any file discovered by walking up from the manifest dir.
    #[arg(long)]
    pub codegen_config: Option<PathBuf>,

    /// Overwrite existing bindings
    #[arg(long)]
    pub force: bool,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Generate `.cargo/config.toml` with `[patch.crates-io]` entries
    #[arg(long, alias = "config")]
    pub generate_config: bool,

    /// Path to nros crates directory for generated Cargo config patches
    #[arg(long, conflicts_with = "nano_ros_git")]
    pub nano_ros_path: Option<PathBuf>,

    /// Use nros git repository for generated Cargo config patches
    #[arg(long, conflicts_with = "nano_ros_path")]
    pub nano_ros_git: bool,

    /// Rename a generated package: --rename old_pkg=new_crate_name
    #[arg(long, value_parser = parse_rename)]
    pub rename: Vec<(String, String)>,

    /// Phase 212.K.7.1 (deprecated, no-op) — see `Args::no_cyclonedds`.
    #[arg(long, hide = true)]
    pub no_cyclonedds: bool,

    /// Phase 212.K.7.1 (deprecated, no-op) — see `Args::cyclonedds_idlc`.
    #[arg(long, hide = true)]
    pub cyclonedds_idlc: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<()> {
    // Phase 218.E — ABI version guard. The manifest path roots the
    // consumer workspace; the guard walks up looking for Cargo.lock.
    let verb = match args.lang {
        Lang::Rust => Verb::GenerateRust,
        Lang::C => Verb::GenerateC,
        Lang::Cpp => Verb::GenerateCpp,
        Lang::All => Verb::GenerateRust,
    };
    abi_guard::check_workspace(&args.manifest, verb)?;

    match args.lang {
        Lang::Rust => generate_rust(&args),
        Lang::C => generate_c(&args),
        Lang::Cpp => Err(eyre!(
            "`nros generate cpp` standalone mode is not yet wired up. \
             Use the CMake `nano_ros_generate_interfaces(... LANGUAGE CPP)` \
             integration for C++ codegen."
        )),
        Lang::All => {
            generate_rust(&args)?;
            generate_c(&args)?;
            // C++ standalone path missing — see comment above.
            Ok(())
        }
    }
}

pub fn run_rust(args: RustArgs) -> Result<()> {
    // Phase 218.E — ABI version guard.
    abi_guard::check_workspace(&args.manifest, Verb::GenerateRust)?;
    // Phase 212.K.7.1 — `--no-cyclonedds` / `--cyclonedds-idlc` are
    // accepted-but-no-op flags; the per-msg-crate cyclonedds emit has
    // been removed. Drain the values so unused-warning silence isn't
    // load-bearing.
    let _ = (args.no_cyclonedds, args.cyclonedds_idlc);
    generate_rust_from_config(GenerateConfig {
        manifest_path: args.manifest,
        output_dir: args.output,
        generate_config: args.generate_config,
        nano_ros_path: args.nano_ros_path,
        nano_ros_git: args.nano_ros_git,
        force: args.force,
        verbose: args.verbose,
        ros_edition: args.ros_edition,
        renames: args.rename.into_iter().collect(),
        codegen_config: args.codegen_config,
    })
}

fn generate_rust(args: &Args) -> Result<()> {
    // Phase 212.K.7.1 — see `run_rust`.
    let _ = (args.no_cyclonedds, &args.cyclonedds_idlc);
    generate_rust_from_config(GenerateConfig {
        manifest_path: args.manifest.clone(),
        output_dir: args.output.clone(),
        generate_config: args.generate_config,
        nano_ros_path: args.nano_ros_path.clone(),
        nano_ros_git: args.nano_ros_git,
        force: args.force,
        verbose: args.verbose,
        ros_edition: args.ros_edition.clone(),
        renames: args.rename.clone().into_iter().collect::<HashMap<_, _>>(),
        codegen_config: args.codegen_config.clone(),
    })
}

fn generate_rust_from_config(cfg: GenerateConfig) -> Result<()> {
    generate_from_package_xml(cfg)
}

fn generate_c(args: &Args) -> Result<()> {
    let cfg = GenerateCStandaloneConfig {
        manifest_path: args.manifest.clone(),
        output_dir: args.output.clone(),
        force: args.force,
        verbose: args.verbose,
        ros_edition: args.ros_edition.clone(),
        codegen_config: args.codegen_config.clone(),
    };
    generate_c_from_package_xml(cfg)
}
