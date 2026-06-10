//! `nros generate-px4-msgs` — Phase 233.1 (RFC-0039 Track B).
//!
//! Generate CDR-serializable `px4_msgs::msg::*` Rust types directly from a
//! PX4-Autopilot `.msg` tree (`msg/` + `msg/versioned/`), with no external ament
//! `px4_msgs` dependency. A nano-ros node uses these types over `nros-rmw-xrce`
//! to talk to the same Micro XRCE-DDS Agent PX4's `uxrce_dds_client` connects to
//! (the `/fmu/out/*` / `/fmu/in/*` topics).

use std::path::PathBuf;

use clap::Args as ClapArgs;
use eyre::{Result, bail, eyre};
use rosidl_codegen::{CapacityResolver, RosEdition};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// PX4-Autopilot tree (defaults to `$PX4_AUTOPILOT_DIR`).
    #[arg(long)]
    pub px4: Option<PathBuf>,

    /// Output directory for the generated `px4_msgs` crate.
    #[arg(long, short)]
    pub output: PathBuf,

    /// ROS 2 edition (`humble` | `iron`).
    #[arg(long, default_value = "humble")]
    pub ros_edition: String,

    /// `px4_msgs` crate version (defaults to the pinned PX4 release).
    #[arg(long = "crate-version", default_value = "1.17.0")]
    pub crate_version: String,

    /// Optional `nros-codegen.toml` for per-field message capacities.
    #[arg(long)]
    pub codegen_config: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<()> {
    let px4 = args
        .px4
        .or_else(|| std::env::var_os("PX4_AUTOPILOT_DIR").map(PathBuf::from))
        .ok_or_else(|| {
            eyre!("generate-px4-msgs: --px4 <DIR> required (or set PX4_AUTOPILOT_DIR)")
        })?;

    let edition = match args.ros_edition.to_lowercase().as_str() {
        "humble" => RosEdition::Humble,
        "iron" => RosEdition::Iron,
        other => bail!("unknown ROS edition '{other}' (humble | iron)"),
    };

    let resolver = match &args.codegen_config {
        Some(p) => CapacityResolver::from_file(p)
            .map_err(|e| eyre!("codegen config {}: {e}", p.display()))?,
        None => CapacityResolver::empty(),
    };

    let generated = rosidl_bindgen::generator::generate_px4_msgs(
        &px4,
        &args.output,
        &args.crate_version,
        edition,
        &resolver,
    )?;

    println!(
        "generated px4_msgs ({} messages) at {}",
        generated.message_count,
        args.output.join("px4_msgs").display()
    );
    Ok(())
}
