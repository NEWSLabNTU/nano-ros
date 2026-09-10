//! `nros sdk-root` — print the nano-ros SDK root this toolchain resolves
//! (phase-447 A2, RFC-0099 D3).
//!
//! The bridge for cmake and shell, exactly as `nros sdk-path` is the bridge to
//! `sdk_store::tool_dir` and `nros model-path` to the SystemModel rule: the
//! ladder lives ONCE, in [`crate::orchestration::nano_ros_root`], and everyone
//! else asks.
//!
//! ## Why a scaffolded `CMakeLists.txt` must ask rather than remember
//!
//! The scaffold cannot bake the answer in. A store moves — `$NROS_HOME` is a
//! variable, `nros pin` swaps a toolchain under a project, and a project is
//! committed and then built on another machine that has neither the same store
//! nor the same version. A baked absolute path is right exactly once, on the
//! machine that ran `nros new`, and wrong silently everywhere else. Asking the
//! `nros` that is on `PATH` is right by construction, because the toolchain
//! answering is the toolchain that would do the codegen.
//!
//! ## Why this needs no index, and no network
//!
//! `nros sdk-path` loads `nros-sdk-index.toml` (and may fetch it) because a
//! tool's prefix is a function of the PIN. The SDK root is not — it is a
//! function of where this binary lives — so this command reads no index, opens
//! no socket, and answers in a directory that is not a workspace. That matters:
//! a configure-time `execute_process` that could block on the network would be
//! a new failure mode in every scaffolded project.

use std::path::PathBuf;

use clap::Parser;
use eyre::{Result, bail};

use crate::orchestration::nano_ros_root;

#[derive(Debug, Parser)]
pub struct Args {
    /// Where to start the walk-up rung. Defaults to the current directory —
    /// which is what a cmake caller wants, since `execute_process` runs in the
    /// project being configured.
    #[arg(long)]
    pub workspace: Option<PathBuf>,

    /// An explicit root, taking precedence over everything. Present so a caller
    /// forwarding a user's `-DNANO_ROS_ROOT` uses ONE ladder rather than
    /// branching around it.
    #[arg(long)]
    pub nano_ros_path: Option<PathBuf>,

    /// Print which rung answered, on stderr. stdout stays the bare path, so a
    /// cmake `execute_process` can keep capturing it unchanged.
    #[arg(long)]
    pub explain: bool,
}

pub fn run(args: Args) -> Result<()> {
    let workspace = match args.workspace {
        Some(w) => w,
        None => std::env::current_dir()?,
    };
    let Some(root) = nano_ros_root::resolve(args.nano_ros_path, &workspace) else {
        bail!("{}", nano_ros_root::not_found_help());
    };
    if args.explain {
        // Named by comparison, not by re-walking: `shipped()` is the only rung
        // whose answer is a property of the BINARY, and it is the one a reader
        // is surprised by.
        let via = if nano_ros_root::shipped().as_deref() == Some(root.as_path()) {
            "this toolchain's own share/nano-ros"
        } else {
            "a nano-ros checkout"
        };
        eprintln!("nros sdk-root: {} — via {via}", root.display());
    }
    println!("{}", root.display());
    Ok(())
}
