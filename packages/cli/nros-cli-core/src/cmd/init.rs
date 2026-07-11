//! `nros init` — RFC-0048 §6 / phase-287 W5.
//!
//! Generate a project's `CMakePresets.json` that `include`s the per-board preset
//! fragments `nros setup <board>` wrote under `~/.nros/presets/`. After this a
//! `cmake --preset <board>` cross-configures a nano-ros ament package with no
//! hand-set `-DCMAKE_TOOLCHAIN_FILE` / `-Dnano_ros_ROOT`. Idempotent — re-run
//! after `nros setup` of a new board to pick up its fragment.

use std::path::PathBuf;

use clap::Args as ClapArgs;
use eyre::Result;

use crate::orchestration::cmake_preset;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Project directory to write `CMakePresets.json` into (default: current dir).
    #[arg(default_value = ".")]
    pub dir: PathBuf,
}

pub fn run(args: Args) -> Result<()> {
    let (path, n) = cmake_preset::write_project_presets(&args.dir)?;
    if n == 0 {
        eprintln!(
            "nros init: wrote {} (no board presets yet — run `nros setup <board>` first)",
            path.display()
        );
    } else {
        eprintln!(
            "nros init: wrote {} including {n} board preset(s); `cmake --preset <board>` is ready",
            path.display()
        );
    }
    Ok(())
}
