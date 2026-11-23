//! The grammar migrations `nros configure --migrate` applies to a project's
//! contracts and `system.toml` files, and the remedy nros names when the
//! checker refuses an old spelling.
//!
//! The rewrites are rlm's own, called as a library: a contract goes through
//! `ros_launch_manifest_types::migrate::migrate_str`, a `system.toml` through
//! `ros_launch_manifest_model::system_config::migrate_system_toml_str`. Those
//! are the functions `play_launch migrate` calls, so nros and play_launch
//! rewrite a file the same way. This module only finds the files, repeats a
//! file's passes until nothing changes (one rewrite can expose the next, as
//! in play_launch's verb), and writes the result.
//!
//! The checker's refusal of an old spelling names `play_launch migrate`,
//! which a nano-ros user does not have. [`name_nros_remedy`] rewrites that
//! remedy into the command nano-ros ships.

use std::path::{Path, PathBuf};

use eyre::{Result, WrapErr};
use ros_launch_manifest_types::migrate::{self, Migration};

/// The command that rewrites old spellings in a nano-ros project.
pub const REMEDY: &str = "nros configure --migrate";

/// Passes per file: enough for every chain of rewrites rlm's table has (the
/// longest is two), with room to spare. play_launch's verb uses the same.
const MAX_PASSES: usize = 4;

/// Directory names the walk does not enter: build trees, installs, logs and
/// VCS metadata hold copies, not sources. A configure copies each bringup's
/// contract under `build/`, and that copy is regenerated from the source.
const SKIP_DIRS: &[&str] = &[
    "build",
    "install",
    "log",
    "target",
    "node_modules",
    "play_log",
    "generated",
];

/// One rewrite applied to a file: the line it is on (in the text that pass
/// read), the rlm rewrite id, and the path of the key it changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedEdit {
    pub line: usize,
    pub rewrite: &'static str,
    pub path: String,
}

/// What migrating one file did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMigration {
    pub file: PathBuf,
    /// The migrated text. Equal to the input when `edits` is empty.
    pub output: String,
    pub edits: Vec<AppliedEdit>,
    /// Places a rewrite applies but cannot edit safely. rlm leaves these for
    /// a hand edit.
    pub skipped: Vec<String>,
}

impl FileMigration {
    #[must_use]
    pub fn changed(&self) -> bool {
        !self.edits.is_empty()
    }
}

/// True for a file the migration reads: a contract (`*.contract.yaml`) or a
/// `system.toml`.
#[must_use]
pub fn is_migratable_file(name: &str) -> bool {
    name.ends_with(".contract.yaml") || is_system_toml(name)
}

fn is_system_toml(name: &str) -> bool {
    name.ends_with("system.toml")
}

/// Migrate one file's text. `name` decides which of rlm's two migrations
/// applies. Passes repeat until one changes nothing.
///
/// # Errors
///
/// rlm's refusal of the first pass (the text is not YAML or TOML, or a
/// rewrite would leave text that no longer parses). A later pass's refusal
/// keeps the passes before it.
pub fn migrate_text(name: &str, src: &str) -> Result<(String, Vec<AppliedEdit>, Vec<String>)> {
    let mut text = src.to_string();
    let mut edits = Vec::new();
    let mut skipped = Vec::new();
    for pass in 0..MAX_PASSES {
        let migrated: std::result::Result<Migration, _> = if is_system_toml(name) {
            ros_launch_manifest_model::system_config::migrate_system_toml_str(&text)
        } else {
            migrate::migrate_str(&text)
        };
        let m = match migrated {
            Ok(m) => m,
            Err(e) if pass == 0 => eyre::bail!("{}", e.0),
            Err(_) => break,
        };
        skipped = m.skipped.clone();
        if !m.changed() {
            break;
        }
        edits.extend(m.edits.iter().map(|e| AppliedEdit {
            line: e.line,
            rewrite: e.rewrite,
            path: e.path.clone(),
        }));
        text = m.output;
    }
    // One rewrite can edit a key and its value: report the key once.
    edits.dedup();
    Ok((text, edits, skipped))
}

/// Every contract and `system.toml` under `root`, skipping build output,
/// hidden directories and [`SKIP_DIRS`]. Sorted.
pub fn find_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk(root, &mut out)?;
    out.sort();
    out.dedup();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir).wrap_err_with(|| format!("reading {}", dir.display()))?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if name.starts_with('.')
                || name.starts_with("build-")
                || SKIP_DIRS.contains(&name.as_str())
            {
                continue;
            }
            walk(&e.path(), out)?;
        } else if ft.is_file() && is_migratable_file(&name) {
            out.push(e.path());
        }
    }
    Ok(())
}

/// The contracts and `system.toml` files under `root` that the migration would
/// change, asked of the same rewrites [`migrate_tree`] applies. Read-only. A
/// file that cannot be read or that rlm cannot migrate is left out.
#[must_use]
pub fn pending_files(root: &Path) -> Vec<PathBuf> {
    find_files(root)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            std::fs::read_to_string(p).is_ok_and(|src| {
                migrate_text(&name, &src).is_ok_and(|(_, edits, _)| !edits.is_empty())
            })
        })
        .collect()
}

/// Migrate every contract and `system.toml` under `root` and write each file
/// that changed. Returns one entry per file that changed or has a place left
/// for a hand edit.
///
/// # Errors
///
/// A file that cannot be read or written, or that rlm refuses to migrate.
pub fn migrate_tree(root: &Path) -> Result<Vec<FileMigration>> {
    let mut done = Vec::new();
    for file in find_files(root)? {
        let src = std::fs::read_to_string(&file)
            .wrap_err_with(|| format!("reading {}", file.display()))?;
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (output, edits, skipped) = migrate_text(&name, &src)
            .wrap_err_with(|| format!("{REMEDY}: cannot migrate {}", file.display()))?;
        if edits.is_empty() && skipped.is_empty() {
            continue;
        }
        if output != src {
            std::fs::write(&file, &output)
                .wrap_err_with(|| format!("writing {}", file.display()))?;
        }
        done.push(FileMigration {
            file,
            output,
            edits,
            skipped,
        });
    }
    Ok(done)
}

/// `msg` with the checker's `play_launch migrate ...` remedy replaced by
/// [`REMEDY`]. The checker names a tool nano-ros does not ship. Text without
/// that remedy comes back unchanged.
#[must_use]
pub fn name_nros_remedy(msg: &str) -> String {
    const TOOL: &str = "play_launch migrate";
    if !msg.contains(TOOL) {
        return msg.to_string();
    }
    let mut out = String::with_capacity(msg.len());
    let mut rest = msg;
    while let Some(at) = rest.find(TOOL) {
        // Inside backticks, the whole command (with its arguments) goes.
        let before = &rest[..at];
        if before.ends_with('`')
            && let Some(close) = rest[at..].find('`')
        {
            out.push_str(before);
            out.push_str(REMEDY);
            rest = &rest[at + close..];
            // ` --write` after the closing backtick never happens in rlm's
            // text; the closing backtick stays.
            continue;
        }
        out.push_str(before);
        out.push_str(REMEDY);
        rest = &rest[at + TOOL.len()..];
    }
    out.push_str(rest);
    out
}

/// True when `msg` is the checker refusing a spelling a migration rewrites.
#[must_use]
pub fn names_a_migration(msg: &str) -> bool {
    msg.contains("play_launch migrate")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The talker contract `nros new` wrote in the nros-0.6.0-nros11 release
    /// (`new_workspace.rs`, `contract_node_rows` and `bringup_contract` at
    /// that tag), for a project named `rs`. Every project scaffolded by that
    /// release carries this shape.
    const NROS11_TALKER_CONTRACT: &str = "\
# What this image's node DECLARES, beside system.toml. The build sizes its
# buffers and its callback table from this, so it must list everything the
# node creates: when you add a timer, a publisher, a subscription or a
# parameter in the code, state it here too.
# `nros build --explain-sizing` lists anything still left to a default.

version: 1

nodes:
  rs_node:
    params: {}
    paths:
      on_tick:
        trigger: { timer: { rate_hz: 1 } }
        output: [chatter]
    pub:
      chatter:
        qos:
          durability: volatile

topics:
  /chatter:
    type: std_msgs/msg/String
    pub: [rs_node/chatter]
    # Read outside this workspace, by `ros2 topic echo` or a ROS 2 node.
    external: sub
";

    #[test]
    fn the_checker_refuses_the_nros11_scaffold_contract() {
        let err = ros_launch_manifest_types::parse_manifest_str(NROS11_TALKER_CONTRACT)
            .expect_err("rlm refuses `rate_hz`");
        assert!(format!("{err}").contains("rate_hz"), "{err}");
    }

    #[test]
    fn the_migration_turns_the_nros11_contract_into_one_the_checker_reads() {
        let (out, edits, skipped) =
            migrate_text("system.contract.yaml", NROS11_TALKER_CONTRACT).unwrap();
        assert!(skipped.is_empty(), "{skipped:?}");
        // rlm's rate rewrite edits the key and the value; the key is reported
        // once, on the line the scaffold wrote the timer.
        assert_eq!(edits.len(), 1, "{edits:?}");
        assert!(
            edits
                .iter()
                .all(|e| e.line == 14 && e.rewrite == "rate-units"),
            "{edits:?}"
        );
        assert!(out.contains("trigger: { timer: { rate: 1Hz } }"), "{out}");
        // Only the rewritten token moved: comments and layout survive.
        assert_eq!(
            out.replace("rate: 1Hz", "rate_hz: 1"),
            NROS11_TALKER_CONTRACT
        );
        ros_launch_manifest_types::parse_manifest_str(&out).expect("the migrated text parses");
    }

    #[test]
    fn a_current_contract_is_left_alone() {
        let current = NROS11_TALKER_CONTRACT.replace("rate_hz: 1", "rate: 1Hz");
        let (out, edits, _) = migrate_text("system.contract.yaml", &current).unwrap();
        assert!(edits.is_empty(), "{edits:?}");
        assert_eq!(out, current);
    }

    #[test]
    fn a_system_toml_takes_the_system_toml_rewrites() {
        let src = "[system]\nrmw = \"zenoh\"\n\n[tiers.ctrl]\npriority = 3\n";
        let (out, edits, _) = migrate_text("system.toml", src).unwrap();
        assert_eq!(edits.len(), 1, "{edits:?}");
        assert!(out.contains("[tasks.ctrl]"), "{out}");
        assert!(out.contains("rmw = \"zenoh\""), "{out}");
    }

    #[test]
    fn the_walk_skips_build_copies_and_rewrites_the_source() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src/rs_node_bringup/system.contract.yaml");
        let copy = dir
            .path()
            .join("build/nros/launch/rs_node_bringup/launch/system.contract.yaml");
        for p in [&src, &copy] {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, NROS11_TALKER_CONTRACT).unwrap();
        }
        let done = migrate_tree(dir.path()).unwrap();
        assert_eq!(done.len(), 1, "{done:?}");
        assert_eq!(done[0].file, src);
        assert!(std::fs::read_to_string(&src).unwrap().contains("rate: 1Hz"));
        assert_eq!(
            std::fs::read_to_string(&copy).unwrap(),
            NROS11_TALKER_CONTRACT,
            "a build copy is regenerated, never migrated"
        );
        // A second run finds nothing left.
        assert!(migrate_tree(dir.path()).unwrap().is_empty());
    }

    /// The checker's refusal, verbatim from `nros configure` on an nros11
    /// scaffold.
    const REFUSAL: &str = "at 'nodes.rs_node.paths.on_tick.trigger.timer.rate_hz': \
        Renamed `rate:` in v0.3.21 and refused since v0.3.27 (phase 104 S3): a rate is \
        a value with its unit (`trigger: { timer: { rate: 100Hz } }`). \
        `play_launch migrate <contract> --write` rewrites it.. Every contract in this \
        file is now UNCHECKED";

    #[test]
    fn the_refusal_names_the_remedy_nano_ros_ships() {
        assert!(names_a_migration(REFUSAL));
        let named = name_nros_remedy(REFUSAL);
        assert!(!named.contains("play_launch"), "{named}");
        assert!(
            named.contains("`nros configure --migrate` rewrites it."),
            "{named}"
        );
        // The rest of the checker's text is unchanged.
        assert!(named.starts_with("at 'nodes.rs_node."), "{named}");
        assert!(named.ends_with("Every contract in this file is now UNCHECKED"));
    }

    #[test]
    fn a_bare_mention_is_renamed_too_and_other_text_is_untouched() {
        assert_eq!(
            name_nros_remedy("or run play_launch migrate on it"),
            "or run nros configure --migrate on it"
        );
        let other = "error: something unrelated";
        assert_eq!(name_nros_remedy(other), other);
        assert!(!names_a_migration(other));
    }
}
