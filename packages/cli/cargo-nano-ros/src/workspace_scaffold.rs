//! phase-368 W8 — `nros new <name> --workspace`: the one-shot minimal
//! workspace scaffold (node pkgs + bringup + entry), C++ or Rust.
//!
//! The file contents are `include_str!` of the canonical copy-out templates
//! (`examples/templates/multi-node-workspace{,-cpp}`) — ONE copy, compiled
//! in, so the scaffold can never drift from the tree the fixture lane builds
//! and the E2E tests run. The cost of that choice is a freshness edge: these
//! files are rustc inputs the cargo crate graph does not name, so
//! `scripts/gen-cli-source-dirs.py` scans for parent-relative `include_str!`
//! literals and folds their directories into the CLI source stamp (the
//! issue-0627 closure) — an embedded template edit re-stales the CLI like any
//! other CLI source.
//!
//! RMW selection is a rewrite of KNOWN ANCHORS in the embedded text, not a
//! templating language. Each anchor is asserted present at scaffold time, so
//! if a template edit moves one the scaffold fails loudly instead of writing
//! a workspace whose halves disagree — the same "three edits that must agree"
//! trap (system.toml `rmw`, the board dep's feature, the `nros` facade's
//! type-descriptor feature) this verb exists to close.

use eyre::{Result, bail};

/// (relative path, contents) — the C++ workspace template, verbatim.
const CPP_FILES: &[(&str, &str)] = &[
    (
        ".gitignore",
        include_str!("../../../../examples/templates/multi-node-workspace-cpp/.gitignore"),
    ),
    (
        "CMakeLists.txt",
        include_str!("../../../../examples/templates/multi-node-workspace-cpp/CMakeLists.txt"),
    ),
    (
        "README.md",
        include_str!("../../../../examples/templates/multi-node-workspace-cpp/README.md"),
    ),
    (
        "src/demo_bringup/.gitignore",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/demo_bringup/.gitignore"
        ),
    ),
    (
        "src/demo_bringup/launch/system.launch.xml",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/demo_bringup/launch/system.launch.xml"
        ),
    ),
    (
        "src/demo_bringup/package.xml",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/demo_bringup/package.xml"
        ),
    ),
    (
        "src/demo_bringup/system.toml",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/demo_bringup/system.toml"
        ),
    ),
    (
        "src/listener_pkg/.gitignore",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/listener_pkg/.gitignore"
        ),
    ),
    (
        "src/listener_pkg/CMakeLists.txt",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/listener_pkg/CMakeLists.txt"
        ),
    ),
    (
        "src/listener_pkg/include/listener_pkg/Listener.hpp",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/listener_pkg/include/listener_pkg/Listener.hpp"
        ),
    ),
    (
        "src/listener_pkg/package.xml",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/listener_pkg/package.xml"
        ),
    ),
    (
        "src/listener_pkg/src/Listener.cpp",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/listener_pkg/src/Listener.cpp"
        ),
    ),
    (
        "src/robot_entry/.gitignore",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/robot_entry/.gitignore"
        ),
    ),
    (
        "src/robot_entry/CMakeLists.txt",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/robot_entry/CMakeLists.txt"
        ),
    ),
    (
        "src/robot_entry/package.xml",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/robot_entry/package.xml"
        ),
    ),
    (
        "src/robot_entry/src/main.cpp",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/robot_entry/src/main.cpp"
        ),
    ),
    (
        "src/talker_pkg/.gitignore",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/talker_pkg/.gitignore"
        ),
    ),
    (
        "src/talker_pkg/CMakeLists.txt",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/talker_pkg/CMakeLists.txt"
        ),
    ),
    (
        "src/talker_pkg/include/talker_pkg/Talker.hpp",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/talker_pkg/include/talker_pkg/Talker.hpp"
        ),
    ),
    (
        "src/talker_pkg/package.xml",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/talker_pkg/package.xml"
        ),
    ),
    (
        "src/talker_pkg/src/Talker.cpp",
        include_str!(
            "../../../../examples/templates/multi-node-workspace-cpp/src/talker_pkg/src/Talker.cpp"
        ),
    ),
];

/// (relative path, contents) — the Rust workspace template, verbatim.
const RUST_FILES: &[(&str, &str)] = &[
    (
        ".gitignore",
        include_str!("../../../../examples/templates/multi-node-workspace/.gitignore"),
    ),
    (
        "Cargo.toml",
        include_str!("../../../../examples/templates/multi-node-workspace/Cargo.toml"),
    ),
    (
        "README.md",
        include_str!("../../../../examples/templates/multi-node-workspace/README.md"),
    ),
    (
        "src/demo_bringup/.gitignore",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/demo_bringup/.gitignore"
        ),
    ),
    (
        "src/demo_bringup/launch/system.launch.xml",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/demo_bringup/launch/system.launch.xml"
        ),
    ),
    (
        "src/demo_bringup/package.xml",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/demo_bringup/package.xml"
        ),
    ),
    (
        "src/demo_bringup/system.toml",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/demo_bringup/system.toml"
        ),
    ),
    (
        "src/listener_pkg/.gitignore",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/listener_pkg/.gitignore"
        ),
    ),
    (
        "src/listener_pkg/Cargo.toml",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/listener_pkg/Cargo.toml"
        ),
    ),
    (
        "src/listener_pkg/package.xml",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/listener_pkg/package.xml"
        ),
    ),
    (
        "src/listener_pkg/src/lib.rs",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/listener_pkg/src/lib.rs"
        ),
    ),
    (
        "src/robot_entry/.gitignore",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/robot_entry/.gitignore"
        ),
    ),
    (
        "src/robot_entry/Cargo.toml",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/robot_entry/Cargo.toml"
        ),
    ),
    (
        "src/robot_entry/package.xml",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/robot_entry/package.xml"
        ),
    ),
    (
        "src/robot_entry/src/main.rs",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/robot_entry/src/main.rs"
        ),
    ),
    (
        "src/talker_pkg/.gitignore",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/talker_pkg/.gitignore"
        ),
    ),
    (
        "src/talker_pkg/Cargo.toml",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/talker_pkg/Cargo.toml"
        ),
    ),
    (
        "src/talker_pkg/package.xml",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/talker_pkg/package.xml"
        ),
    ),
    (
        "src/talker_pkg/src/lib.rs",
        include_str!(
            "../../../../examples/templates/multi-node-workspace/src/talker_pkg/src/lib.rs"
        ),
    ),
];

pub struct WorkspaceScaffold {
    /// Workspace directory to create.
    pub dir: std::path::PathBuf,
    /// "cpp" | "rust" (the C node-pkg walkthrough shares the cpp shape).
    pub lang: String,
    /// "cyclonedds" | "zenoh" | "xrce".
    pub rmw: String,
    pub force: bool,
}

/// Replace `needle` with `to` exactly once, failing loudly when the anchor is
/// gone — a silent no-op here writes a workspace whose RMW halves disagree.
fn rewrite_once(file: &str, text: String, needle: &str, to: &str) -> Result<String> {
    match text.match_indices(needle).count() {
        1 => Ok(text.replacen(needle, to, 1)),
        n => bail!(
            "workspace template anchor `{needle}` found {n}x in `{file}` (expected exactly 1) — \
             the embedded template drifted; fix the anchor in workspace_scaffold.rs"
        ),
    }
}

/// Apply the RMW choice to one file's text. Anchors mirror the committed
/// template defaults: the C++ root defaults to cyclonedds (overridable via
/// `-DNROS_RMW`), the Rust tree to zenoh.
fn parameterize(lang: &str, rmw: &str, rel: &str, text: &str) -> Result<String> {
    let mut out = text.to_string();
    match (lang, rel) {
        // Both trees: the bringup's declared rmw follows the choice.
        (_, "src/demo_bringup/system.toml") => {
            out = rewrite_once(rel, out, "rmw = \"zenoh\"", &format!("rmw = \"{rmw}\""))?;
        }
        // C++: one word — the workspace BACKEND default.
        ("cpp", "CMakeLists.txt") => {
            out = rewrite_once(
                rel,
                out,
                "set(NROS_RMW cyclonedds)",
                &format!("set(NROS_RMW {rmw})"),
            )?;
        }
        // Rust: the entry's board feature + (cyclone only) the facade's
        // type-descriptor feature.
        ("rust", "src/robot_entry/Cargo.toml") => {
            let board = match rmw {
                "zenoh" => "nros-board-linux = { version = \"*\" }".to_string(),
                other => format!(
                    "nros-board-linux = {{ version = \"*\", default-features = false, \
                     features = [\"rmw-{other}\"] }}"
                ),
            };
            out = rewrite_once(rel, out, "nros-board-linux = { version = \"*\" }", &board)?;
            if rmw == "cyclonedds" {
                out = rewrite_once(
                    rel,
                    out,
                    "\"rmw-cffi\",",
                    "\"rmw-cffi\", \"rmw-cyclonedds\",",
                )?;
            }
        }
        _ => {}
    }
    Ok(out)
}

pub fn scaffold_workspace(cfg: &WorkspaceScaffold) -> Result<()> {
    let files: &[(&str, &str)] = match cfg.lang.as_str() {
        "cpp" => CPP_FILES,
        "rust" => RUST_FILES,
        other => bail!(
            "`nros new <name> --workspace --lang {other}` is not supported yet — \
             use `cpp` (the default) or `rust`. C node pkgs join an existing \
             workspace via `nros new --component --lang c`."
        ),
    };
    match cfg.rmw.as_str() {
        "cyclonedds" | "zenoh" | "xrce" => {}
        other => bail!("unknown --rmw `{other}` (cyclonedds | zenoh | xrce)"),
    }
    if cfg.dir.exists() && !cfg.force {
        bail!(
            "`{}` already exists — pass --force to scaffold into it anyway",
            cfg.dir.display()
        );
    }
    for (rel, raw) in files {
        let text = parameterize(&cfg.lang, &cfg.rmw, rel, raw)?;
        let dest = cfg.dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, text)?;
    }
    eprintln!(
        "nros new --workspace: scaffolded {} ({} files, lang={}, rmw={})",
        cfg.dir.display(),
        files.len(),
        cfg.lang,
        cfg.rmw,
    );
    let next = match cfg.lang.as_str() {
        "cpp" => format!(
            "Next steps:\n  cd {0}\n  cmake -S . -B build -DNANO_ROS_ROOT=<path-to-nano-ros>\n  cmake --build build\n  ./build/src/robot_entry/robot_entry",
            cfg.dir.display()
        ),
        _ => format!(
            "Next steps:\n  cd {0}\n  NROS_REPO_DIR=<path-to-nano-ros> nros sync\n  cargo run",
            cfg.dir.display()
        ),
    };
    eprintln!("{next}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scaffold(lang: &str, rmw: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        scaffold_workspace(&WorkspaceScaffold {
            dir: tmp.path().join("ws"),
            lang: lang.into(),
            rmw: rmw.into(),
            force: false,
        })
        .expect("scaffold");
        tmp
    }

    #[test]
    fn cpp_default_bakes_the_backend_word_and_the_bringup_rmw() {
        let tmp = scaffold("cpp", "cyclonedds");
        let root = std::fs::read_to_string(tmp.path().join("ws/CMakeLists.txt")).unwrap();
        assert!(root.contains("set(NROS_RMW cyclonedds)"));
        let sys =
            std::fs::read_to_string(tmp.path().join("ws/src/demo_bringup/system.toml")).unwrap();
        assert!(sys.contains("rmw = \"cyclonedds\""));
        assert!(!sys.contains("rmw = \"zenoh\""));
    }

    #[test]
    fn cpp_zenoh_choice_lands_in_both_files() {
        let tmp = scaffold("cpp", "zenoh");
        let root = std::fs::read_to_string(tmp.path().join("ws/CMakeLists.txt")).unwrap();
        assert!(root.contains("set(NROS_RMW zenoh)"));
        let sys =
            std::fs::read_to_string(tmp.path().join("ws/src/demo_bringup/system.toml")).unwrap();
        assert!(sys.contains("rmw = \"zenoh\""));
    }

    #[test]
    fn rust_cyclonedds_applies_all_three_edits() {
        let tmp = scaffold("rust", "cyclonedds");
        let sys =
            std::fs::read_to_string(tmp.path().join("ws/src/demo_bringup/system.toml")).unwrap();
        assert!(sys.contains("rmw = \"cyclonedds\""));
        let entry =
            std::fs::read_to_string(tmp.path().join("ws/src/robot_entry/Cargo.toml")).unwrap();
        assert!(
            entry.contains("features = [\"rmw-cyclonedds\"]"),
            "board feature: {entry}"
        );
        assert!(
            entry.contains("\"rmw-cffi\", \"rmw-cyclonedds\","),
            "facade feature"
        );
    }

    #[test]
    fn rust_zenoh_is_the_committed_default_untouched() {
        let tmp = scaffold("rust", "zenoh");
        let entry =
            std::fs::read_to_string(tmp.path().join("ws/src/robot_entry/Cargo.toml")).unwrap();
        assert!(entry.contains("nros-board-linux = { version = \"*\" }"));
        assert!(!entry.contains("rmw-cyclonedds"));
    }

    #[test]
    fn existing_dir_refused_without_force() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("ws");
        std::fs::create_dir_all(&dir).unwrap();
        let err = scaffold_workspace(&WorkspaceScaffold {
            dir,
            lang: "cpp".into(),
            rmw: "cyclonedds".into(),
            force: false,
        })
        .unwrap_err();
        assert!(err.to_string().contains("--force"));
    }

    #[test]
    fn every_embedded_file_set_is_nonempty_and_relative() {
        for (name, set) in [("cpp", CPP_FILES), ("rust", RUST_FILES)] {
            assert!(set.len() >= 15, "{name} template shrank to {}", set.len());
            for (rel, text) in set.iter() {
                assert!(!rel.starts_with('/'), "{rel} not relative");
                assert!(!text.is_empty(), "{rel} empty");
            }
        }
    }
}
