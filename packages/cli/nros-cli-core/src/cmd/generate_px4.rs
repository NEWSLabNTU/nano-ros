//! `nros generate-px4-msgs` — Phase 233.1 (RFC-0039 Track B).
//!
//! Generate CDR-serializable `px4_msgs::msg::*` Rust types directly from a
//! PX4-Autopilot `.msg` tree (`msg/` + `msg/versioned/`), with no external ament
//! `px4_msgs` dependency. A nano-ros node uses these types over `nros-rmw-xrce`
//! to talk to the same Micro XRCE-DDS Agent PX4's `uxrce_dds_client` connects to
//! (the `/fmu/out/*` / `/fmu/in/*` topics).

use std::path::PathBuf;

use clap::Args as ClapArgs;
use eyre::{Result, eyre};
use rosidl_codegen::{CapacityResolver, RosEdition};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// PX4-Autopilot tree (defaults to `$PX4_AUTOPILOT_DIR`).
    #[arg(long)]
    pub px4: Option<PathBuf>,

    /// Output directory for the generated `px4_msgs` crate.
    #[arg(long, short)]
    pub output: PathBuf,

    /// ROS 2 edition (`humble` | `iron` | `jazzy`).
    #[arg(long, default_value = rosidl_codegen::DEFAULT_ROS_EDITION)]
    pub ros_edition: String,

    /// `px4_msgs` crate version (defaults to the pinned PX4 release).
    #[arg(long = "crate-version", default_value = "1.17.0")]
    pub crate_version: String,

    /// Optional `nros-codegen.toml` for per-field message capacities.
    #[arg(long)]
    pub codegen_config: Option<PathBuf>,

    /// Output language: `rust` (a `px4_msgs` crate, the XRCE-companion default)
    /// or `cpp` (headers + FFI glue for an in-firmware PX4 module — issue 0362).
    #[arg(long, default_value = "rust")]
    pub lang: String,

    /// Emit only these messages (issue 0362 approach B — a bridge carries a
    /// handful of topics, not PX4's ~200). Accepts the message name
    /// (`VehicleStatus`) or the uORB topic spelling (`vehicle_status`). Nested
    /// types are pulled in automatically. Default: every message.
    #[arg(long, value_delimiter = ',')]
    pub topics: Vec<String>,
}

pub fn run(args: Args) -> Result<()> {
    let px4 = args
        .px4
        .or_else(|| std::env::var_os("PX4_AUTOPILOT_DIR").map(PathBuf::from))
        .ok_or_else(|| {
            eyre!("generate-px4-msgs: --px4 <DIR> required (or set PX4_AUTOPILOT_DIR)")
        })?;

    let edition = RosEdition::parse(&args.ros_edition).ok_or_else(|| {
        eyre!(
            "unknown ROS edition '{}' (humble | iron | jazzy)",
            args.ros_edition
        )
    })?;

    let resolver = match &args.codegen_config {
        Some(p) => CapacityResolver::from_file(p)
            .map_err(|e| eyre!("codegen config {}: {e}", p.display()))?,
        None => CapacityResolver::empty(),
    };

    match args.lang.as_str() {
        "rust" => {
            if !args.topics.is_empty() {
                eyre::bail!(
                    "--topics is only supported with --lang cpp (the Rust crate is emitted whole)"
                );
            }
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
        }
        "cpp" => {
            // Issue 0362 — the RIHS01 hash is load-bearing on the wire, and Humble
            // has no type hash at all. Warn rather than silently emit a placeholder
            // an rmw_zenoh peer will never match.
            if !edition.uses_type_hash() {
                eprintln!(
                    "warning: --ros-edition {} predates REP-2011, so the emitted TYPE_HASH is a \
                     placeholder. A peer that keys discovery on the type hash (rmw_zenoh) needs \
                     --ros-edition iron|jazzy.",
                    args.ros_edition
                );
            }
            let generated = rosidl_bindgen::generator::generate_px4_msgs_cpp(
                &px4,
                &args.output,
                &args.crate_version,
                edition,
                &resolver,
                &args.topics,
            )?;
            println!(
                "generated px4_msgs C++ ({} messages) at {}",
                generated.message_count,
                generated.output_dir.display()
            );
        }
        other => eyre::bail!("unknown --lang '{other}' (rust | cpp)"),
    }
    Ok(())
}
