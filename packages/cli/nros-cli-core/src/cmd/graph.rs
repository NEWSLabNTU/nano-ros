//! `nros graph` — the workspace's packages in dependency order, each with the
//! workspace packages it depends on (RFC-0117 D2).
//!
//! The order is `provider_scan`'s topological order, the same one `nros build`
//! and `nros dev ws order` use, so the three cannot disagree.

use std::path::PathBuf;

use clap::Args as ClapArgs;
use eyre::{Result, WrapErr, bail};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Workspace to scan. Defaults to the current directory.
    #[arg(long)]
    pub workspace: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<()> {
    use cargo_nano_ros::provider_scan;

    let workspace = match args.workspace {
        Some(w) => w,
        None => std::env::current_dir().wrap_err("resolving cwd as the workspace root")?,
    };
    let workspace = workspace.canonicalize().unwrap_or(workspace);
    let (pkgs, scan) = provider_scan::scan_workspace_packages(&workspace)?;
    for e in &scan.errors {
        eprintln!("warning: {}: {}", e.path.display(), e.message);
    }
    if pkgs.is_empty() {
        bail!("no package.xml under {}", workspace.display());
    }
    let ordered = provider_scan::topological_order(&pkgs)
        .map_err(|cycle| eyre::eyre!("dependency cycle among workspace packages: {cycle}"))?;
    print!("{}", render(&ordered));
    Ok(())
}

/// One line per package, in build order: its name, then the workspace
/// packages it depends on, sorted.
pub fn render(ordered: &[cargo_nano_ros::provider_scan::WorkspacePackage]) -> String {
    let width = ordered.iter().map(|p| p.name.len()).max().unwrap_or(0);
    let mut out = String::new();
    for p in ordered {
        let mut deps: Vec<&str> = ordered
            .iter()
            .filter(|q| p.depends.contains(&q.name))
            .map(|q| q.name.as_str())
            .collect();
        deps.sort_unstable();
        if deps.is_empty() {
            out.push_str(&format!("{}\n", p.name));
        } else {
            out.push_str(&format!("{:<width$}  <- {}\n", p.name, deps.join(", ")));
        }
    }
    out
}
