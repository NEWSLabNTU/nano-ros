//! phase-439 W6 — `nros ws message-bound-knobs`: the Rust twin of
//! `nros_derive_message_bound_knobs`, over JSON bound tables.
//!
//! The production caller is `nros build` itself (the west road's resolve
//! seed), which calls [`crate::message_bound_knobs::compose`] directly. This
//! verb exists so `tests/cmake-message-bounds-tests.sh` can run BOTH composers
//! on every case it builds and compare the bytes: the seed removes a configure
//! pass only when it is byte-identical to the cmake answer, and a hand-kept
//! list of cases would cover the shapes someone thought of.
//!
//! Exits 0 having written `--output`, or 3 having written nothing when the
//! cmake text for these inputs would name a build-dir path (see the module
//! docs of [`crate::message_bound_knobs`]).

use std::path::PathBuf;

use clap::Args as ClapArgs;
use eyre::{Result, WrapErr};

use crate::message_bound_knobs::{BoundTable, compose};

#[derive(Debug, ClapArgs)]
pub struct MessageBoundKnobsArgs {
    /// One `nros_message_bounds.json` per interface package in the closure.
    #[arg(long = "table", value_name = "JSON")]
    pub tables: Vec<PathBuf>,

    /// The entity-inventory fragment the configure would include, if any.
    #[arg(long, value_name = "CMAKE")]
    pub entity_inventory: Option<PathBuf>,

    /// The small/large class split.
    #[arg(long, default_value_t = rosidl_codegen::bounds::DEFAULT_SMALL_CLASS_CEILING)]
    pub ceiling: usize,

    /// Where to write the composed fragment.
    #[arg(long, value_name = "PATH")]
    pub output: PathBuf,
}

pub fn run(args: MessageBoundKnobsArgs) -> Result<()> {
    let mut tables = Vec::with_capacity(args.tables.len());
    for p in &args.tables {
        let doc =
            std::fs::read_to_string(p).wrap_err_with(|| format!("reading {}", p.display()))?;
        let rows = rosidl_codegen::bounds::bounds_from_json(&doc)
            .map_err(|e| eyre::eyre!("{}: {e}", p.display()))?;
        let package = serde_json::from_str::<serde_json::Value>(&doc)
            .ok()
            .and_then(|v| {
                v.get("package")
                    .and_then(|s| s.as_str())
                    .map(str::to_string)
            })
            .ok_or_else(|| eyre::eyre!("{}: no `package`", p.display()))?;
        tables.push(BoundTable { package, rows });
    }
    let entity = match &args.entity_inventory {
        Some(p) if p.exists() => {
            Some(std::fs::read_to_string(p).wrap_err_with(|| format!("reading {}", p.display()))?)
        }
        _ => None,
    };
    match compose(&tables, entity.as_deref(), args.ceiling) {
        Some(text) => {
            std::fs::write(&args.output, text)
                .wrap_err_with(|| format!("writing {}", args.output.display()))?;
            Ok(())
        }
        None => std::process::exit(3),
    }
}
