// RFC-0129 D8.
//! `nros fix` — apply the mechanical remedies a build's refusals name.
//!
//! Loads the workspace's configuration files the way `nros build` does,
//! collects every refusal that carries a fix, and prints the change as a
//! unified diff. It writes only with `--apply`, so a script and a person get
//! the same behaviour, and it never builds. A refusal whose remedy needs
//! judgement carries no fix; it is printed so the reader knows what is left.

use std::path::PathBuf;

use clap::Args as ClapArgs;
use eyre::{Result, WrapErr};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// The workspace (default: the current directory)
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,

    /// Write the change; without it, only the diff prints
    #[arg(long)]
    pub apply: bool,
}

pub fn run(args: Args) -> Result<()> {
    let ws = match args.path {
        Some(p) => p,
        None => std::env::current_dir().wrap_err("resolving cwd as the workspace root")?,
    };
    let found = crate::fix::workspace_findings(&ws);
    let (fixable, by_hand): (Vec<_>, Vec<_>) = found.into_iter().partition(|f| f.fix.is_some());
    let edits = crate::fix::apply(&fixable)?;
    for (path, old, new) in &edits {
        print!("{}", crate::fix::unified_diff(&ws, path, old, new));
    }
    if args.apply {
        for (path, _, new) in &edits {
            crate::atomic_file::atomic_write(path, new)
                .wrap_err_with(|| format!("write {}", path.display()))?;
        }
    }
    let n = edits.len();
    match (n, args.apply) {
        (0, _) if by_hand.is_empty() => eprintln!("nros fix: nothing to fix"),
        (0, _) => {}
        (_, true) => eprintln!("nros fix: wrote {n} file(s)"),
        (_, false) => {
            eprintln!("nros fix: {n} file(s) would change; run `nros fix --apply` to write them")
        }
    }
    if !by_hand.is_empty() {
        eprintln!(
            "\nnros fix: {} refusal(s) need a change by hand:\n\n{}",
            by_hand.len(),
            crate::fix::render_all(&by_hand)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    /// A retired spelling, the subject of these tests (the gate exempts this line).
    const OLD: &str = "ZPICO_MAX_QUERYABLES"; // retired spelling under test

    fn ws() -> tempfile::TempDir {
        let td = tempfile::tempdir().unwrap();
        let b = td.path().join("src/app_bringup");
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(
            b.join("system.toml"),
            format!("[image.talker]\nboard = \"linux\"\nenv = {{ {OLD} = \"4\" }}\n"),
        )
        .unwrap();
        td
    }

    fn system(td: &tempfile::TempDir) -> String {
        std::fs::read_to_string(td.path().join("src/app_bringup/system.toml")).unwrap()
    }

    #[test]
    fn without_apply_nothing_is_written() {
        let td = ws();
        let before = system(&td);
        run(Args {
            path: Some(td.path().to_path_buf()),
            apply: false,
        })
        .unwrap();
        assert_eq!(system(&td), before);
    }

    #[test]
    fn with_apply_the_row_moves_to_the_knobs_table_and_the_build_preflight_passes() {
        let td = ws();
        assert!(crate::fix::preflight(td.path()).is_err());
        run(Args {
            path: Some(td.path().to_path_buf()),
            apply: true,
        })
        .unwrap();
        let text = system(&td);
        assert!(
            text.contains("[image.talker.knobs.entity]\nmax_queryables = 4"),
            "{text}"
        );
        crate::fix::preflight(td.path()).unwrap();
    }
}
