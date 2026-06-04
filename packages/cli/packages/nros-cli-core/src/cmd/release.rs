//! Maintainer-only `nros release …` subcommands. Compiled in only when
//! `--features release` is set; hidden from end-user help. Phase 111.B.

use clap::{Args as ClapArgs, Subcommand};
use eyre::{Result, eyre};

#[derive(Debug, Subcommand)]
pub enum Args {
    /// Diff each crate's `version =` against crates.io and emit a
    /// topo-sorted publish plan.
    Detect(DetectArgs),
    /// Execute the publish plan (`--dry-run` by default).
    Publish(PublishArgs),
    /// Create + push the workspace git tag.
    Tag(TagArgs),
    /// Build, package, and tag every C / C++ release artifact.
    CLibs(CLibsArgs),
}

#[derive(Debug, ClapArgs)]
pub struct DetectArgs {
    #[arg(long)]
    pub check: bool,
}

#[derive(Debug, ClapArgs)]
pub struct PublishArgs {
    #[arg(long)]
    pub execute: bool,
}

#[derive(Debug, ClapArgs)]
pub struct TagArgs;

#[derive(Debug, ClapArgs)]
pub struct CLibsArgs;

pub fn run(_args: Args) -> Result<()> {
    Err(eyre!(
        "`nros release` is reserved for the Phase 111.B follow-up; \
         the CLI ships the verb skeleton but the publish pipeline \
         is not implemented."
    ))
}
