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
    // No root `CMakeLists.txt` (RFC-0098 D9, phase-445 W5) — the marker makes
    // the directory a workspace, and `nros build` generates the cmake root and
    // the entry under `build/`.
    (
        ".colcon_workspace",
        include_str!("../../../../examples/templates/multi-node-workspace-cpp/.colcon_workspace"),
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
    // No root `Cargo.toml` (RFC-0098 D9) — the marker is what makes the
    // directory a workspace, and `nros build` generates the entry.
    (
        ".colcon_workspace",
        include_str!("../../../../examples/templates/multi-node-workspace/.colcon_workspace"),
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

/// Apply the RMW choice to one file's text. The ONE anchor is the bringup's
/// `[system] rmw` — both trees generate their root and entry from it.
fn parameterize(_lang: &str, rmw: &str, rel: &str, text: &str) -> Result<String> {
    let mut out = text.to_string();
    // Both trees: the bringup's declared rmw follows the choice. It is the ONLY
    // anchor:
    // * no C++ root — the root `CMakeLists.txt` whose `set(NROS_RMW cyclonedds)`
    //   was a second place the choice had to agree is gone (RFC-0098 D9,
    //   phase-445 W5); `nros build` writes the cmake root under `build/` with
    //   `BACKEND` taken from the image's rmw;
    // * no Rust entry — it is GENERATED by `nros build` (RFC-0065 D4), and its
    //   board crate and RMW features come from the selection facade `nros sync`
    //   writes from `system.toml`'s `rmw`. The hand-written `robot_entry` this
    //   used to rewrite was the second and third place that choice had to agree.
    if rel == "src/demo_bringup/system.toml" {
        out = rewrite_once(rel, out, "rmw = \"zenoh\"", &format!("rmw = \"{rmw}\""))?;
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
            "Next steps:\n  cd {0}\n  export NROS_REPO_DIR=<path-to-nano-ros>\n  nros sync\n  nros build native     # prints its build dir: build/<coord>/cmake\n  ./build/<coord>/cmake/native_entry",
            cfg.dir.display()
        ),
        _ => format!(
            "Next steps:\n  cd {0}\n  export NROS_REPO_DIR=<path-to-nano-ros>\n  nros sync\n  nros build native\n  ./build/posix/native_entry/target/debug/native_entry",
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
    fn cpp_rmw_choice_lands_in_the_one_place_that_names_it() {
        // The cmake root and the entry are generated, so `system.toml`'s `rmw`
        // is the only place the choice is written (phase-445 W5).
        for rmw in ["cyclonedds", "zenoh"] {
            let tmp = scaffold("cpp", rmw);
            let sys = std::fs::read_to_string(tmp.path().join("ws/src/demo_bringup/system.toml"))
                .unwrap();
            assert!(sys.contains(&format!("rmw = \"{rmw}\"")), "{rmw}: {sys}");
        }
        let tmp = scaffold("cpp", "cyclonedds");
        let sys =
            std::fs::read_to_string(tmp.path().join("ws/src/demo_bringup/system.toml")).unwrap();
        assert!(!sys.contains("rmw = \"zenoh\""));
    }

    #[test]
    fn a_cpp_workspace_has_no_root_build_file_and_no_entry_package() {
        // RFC-0098 D9 / RFC-0065 D4 — the C++ twin of the Rust test below.
        let tmp = scaffold("cpp", "zenoh");
        let ws = tmp.path().join("ws");
        assert!(!ws.join("CMakeLists.txt").exists(), "no root build file");
        assert!(ws.join(".colcon_workspace").is_file(), "the root marker");
        assert!(
            !ws.join("src/robot_entry").exists(),
            "the entry is generated"
        );
        let sys = std::fs::read_to_string(ws.join("src/demo_bringup/system.toml")).unwrap();
        assert!(sys.contains("[image.native]"), "an image to build: {sys}");
    }

    #[test]
    fn rust_cyclonedds_lands_in_the_one_place_that_names_it() {
        // The entry is generated, so `system.toml`'s `rmw` is the ONLY place
        // the choice is written — no hand-written entry to keep in step.
        let tmp = scaffold("rust", "cyclonedds");
        let sys =
            std::fs::read_to_string(tmp.path().join("ws/src/demo_bringup/system.toml")).unwrap();
        assert!(sys.contains("rmw = \"cyclonedds\""));
        assert!(!sys.contains("rmw = \"zenoh\""));
    }

    #[test]
    fn a_rust_workspace_has_no_root_build_file_and_no_entry_package() {
        // RFC-0098 D9 / RFC-0065 D4: a marker, node packages and a bringup.
        let tmp = scaffold("rust", "zenoh");
        let ws = tmp.path().join("ws");
        assert!(!ws.join("Cargo.toml").exists(), "no root build file");
        assert!(ws.join(".colcon_workspace").is_file(), "the root marker");
        assert!(
            !ws.join("src/robot_entry").exists(),
            "the entry is generated"
        );
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
