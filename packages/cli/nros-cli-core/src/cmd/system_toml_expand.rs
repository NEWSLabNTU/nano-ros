//! `nros dev image system-toml` -- a bringup's `system.toml` with its
//! `include = [..]` fragments expanded (RFC-0024 §5, phase-580 W3).
//!
//! For the roads that read the file without a TOML parser of their own: a
//! CMake configure (`file(STRINGS)` over the expansion) and the colcon task.
//! They ask this verb instead of reading the file raw, so a value stated in a
//! fragment reaches them, and every file it read is a configure input.

use std::path::PathBuf;

use clap::Args as ClapArgs;
use eyre::{Result, eyre};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// The `system.toml` to expand (or a directory holding one).
    pub path: PathBuf,
    /// Write the expansion here (only when its text changed), instead of
    /// printing it.
    #[arg(long, value_name = "FILE")]
    pub out: Option<PathBuf>,
    /// Write every file the expansion read, one absolute path per line, here:
    /// the file itself first, then each fragment.
    #[arg(long, value_name = "FILE")]
    pub files_out: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<()> {
    let path = if args.path.is_dir() {
        args.path.join("system.toml")
    } else {
        args.path.clone()
    };
    let expanded = nros_system_include::read(&path).map_err(|e| eyre!("{e}"))?;
    let text = expanded.text();
    match &args.out {
        Some(out) => write_if_changed(out, &text)?,
        None => print!("{text}"),
    }
    if let Some(f) = &args.files_out {
        let list: String = expanded
            .files
            .iter()
            .map(|p| format!("{}\n", p.display()))
            .collect();
        write_if_changed(f, &list)?;
    }
    Ok(())
}

/// Write `text` to `path` only when it differs, so an unchanged expansion
/// leaves no new mtime for a build edge to see.
fn write_if_changed(path: &std::path::Path, text: &str) -> Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| eyre!("{}: {e}", d.display()))?;
    }
    std::fs::write(path, text).map_err(|e| eyre!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_writes_the_expansion_and_every_file_read() {
        let td = tempfile::tempdir().unwrap();
        let shared = td.path().join("shared.toml");
        std::fs::write(&shared, "[[domain]]\nid = 7\n").unwrap();
        let b = td.path().join("b");
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(
            b.join("system.toml"),
            "include = [\"../shared.toml\"]\n[system]\nname = \"b\"\n",
        )
        .unwrap();
        let out = td.path().join("o/expanded.toml");
        let files = td.path().join("o/files.txt");
        run(Args {
            path: b.clone(),
            out: Some(out.clone()),
            files_out: Some(files.clone()),
        })
        .unwrap();
        let t: toml::Table = std::fs::read_to_string(&out).unwrap().parse().unwrap();
        assert_eq!(t["domain"][0]["id"].as_integer(), Some(7));
        assert!(!t.contains_key("include"));
        let list = std::fs::read_to_string(&files).unwrap();
        assert_eq!(list.lines().count(), 2, "{list}");
        assert!(
            list.lines().nth(1).unwrap().ends_with("shared.toml"),
            "{list}"
        );
        // A second run with nothing changed leaves the file's mtime alone.
        let m0 = std::fs::metadata(&out).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        run(Args {
            path: b,
            out: Some(out.clone()),
            files_out: None,
        })
        .unwrap();
        assert_eq!(std::fs::metadata(&out).unwrap().modified().unwrap(), m0);
    }
}
