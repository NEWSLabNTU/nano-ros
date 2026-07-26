//! phase-307 W5 — the coverage gate.
//!
//! The counting mechanism must not quietly regress to "works for the two
//! examples someone tested". This walks EVERY example package in the tree and
//! asserts that a package which declares a node is a metadata-mode candidate —
//! i.e. `Workspace::component_declarations()` yields a declaration for it, the
//! precondition for `nros sync` producing its `source-metadata.json`.
//!
//! Why discovery and not the sidecar itself: producing a sidecar compiles a
//! host probe per package, and this repo does not compile inside tests (a test
//! that shells cargo is a build step wearing a test's clothes). Compilation is
//! proven once, end-to-end, by the W6 lanes. What regresses silently — and what
//! this gate catches — is a package SHAPE dropping out of discovery, which is
//! exactly the W1 defect: `[package.metadata.nros.node]` was parsed for years
//! and never became a declaration, so `nros metadata --build` had no candidates
//! in any real workspace and nobody noticed.
//!
//! Platform-agnosticism is structural, not sampled. The producer compiles a
//! HOST probe from the package's own sources, so a zephyr / freertos / nuttx /
//! threadx / esp32 / bare-metal node package is discovered by the same code
//! path as a native one. This gate makes that claim falsifiable by enumerating
//! all of them rather than a chosen few — a platform whose packages stop being
//! discovered fails here, not in a QEMU lane three phases later.

use std::{
    fs,
    path::{Path, PathBuf},
};

use nros_cli_core::orchestration::{source_metadata::ComponentLanguage, workspace::Workspace};

/// Package shapes, classified from the manifests alone (no build).
#[derive(Debug, PartialEq, Eq)]
enum Shape {
    /// Declares a node through Cargo metadata — the canonical Rust Node pkg.
    RustNode,
    /// Declares a node through `nano_ros_node_register` in CMake — C / C++.
    CmakeNode,
    /// Declares a node through a standalone/folded `[component]` table.
    ComponentToml,
    /// Not a node package: Entry pkgs, message packages, single-binary
    /// examples. Nothing to count, nothing to produce.
    NotANode,
}

fn repo_root() -> PathBuf {
    // <repo>/packages/cli/nros-cli-core/tests/ → <repo>
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

fn classify(pkg_dir: &Path) -> Shape {
    let read = |name: &str| fs::read_to_string(pkg_dir.join(name)).unwrap_or_default();
    let cargo = read("Cargo.toml");
    for key in [
        "[package.metadata.nros.node]",
        "[package.metadata.nros.nodes",
        "[package.metadata.nros.component]",
        "[package.metadata.nros.components",
    ] {
        if cargo.contains(key) {
            return Shape::RustNode;
        }
    }
    // Comment-stripped: an entry CMakeLists that MENTIONS the verb in a
    // comment ("their nano_ros_node_register has no DEPLOY") is not a node
    // package, and the CLI's own static parser agrees. Matching raw text made
    // six entry packages look like unproduced nodes.
    let cmake_calls: String = read("CMakeLists.txt")
        .lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    if cmake_calls.contains("nano_ros_node_register(") {
        return Shape::CmakeNode;
    }
    if read("nros.toml").contains("[component]") || pkg_dir.join("component_nros.toml").is_file() {
        return Shape::ComponentToml;
    }
    Shape::NotANode
}

/// Every `package.xml` under `examples/`, excluding build output.
fn example_packages(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(&root.join("examples"), &mut out);
    out.sort();
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !path.is_dir() {
            continue;
        }
        // Build output is not source. `target-*` covers the per-RMW dirs.
        if name.starts_with('.')
            || name == "build"
            || name == "generated"
            || name.starts_with("target")
        {
            continue;
        }
        if path.join("package.xml").is_file() {
            out.push(path.clone());
        }
        walk(&path, out);
    }
}

/// The gate: every node-declaring example package must be a metadata-mode
/// candidate. A failure here means some package shape fell out of discovery and
/// its entity count silently reverted to the SystemModel's timer-blind lower
/// bound — the issue-0257 failure mode, re-armed.
#[test]
fn every_node_declaring_example_is_a_metadata_candidate() {
    let root = repo_root();
    let packages = example_packages(&root);
    assert!(
        packages.len() > 100,
        "walked only {} example packages — the enumeration broke, and a gate \
         that enumerates nothing passes vacuously",
        packages.len()
    );

    let mut missing = Vec::new();
    let mut counted = 0usize;
    for pkg in &packages {
        let shape = classify(pkg);
        if shape == Shape::NotANode {
            continue;
        }
        counted += 1;
        let rel = pkg.strip_prefix(&root).unwrap_or(pkg).display().to_string();
        let ws = match Workspace::discover(pkg) {
            Ok(ws) => ws,
            Err(err) => {
                missing.push(format!("{rel}: discover failed: {err}"));
                continue;
            }
        };
        match ws.component_declarations() {
            Ok(decls) if !decls.is_empty() => {}
            Ok(_) => missing.push(format!(
                "{rel}: {shape:?} declares a node but yields no \
                                           component declaration"
            )),
            Err(err) => missing.push(format!("{rel}: declarations failed: {err}")),
        }
    }

    assert!(
        counted >= 75,
        "only {counted} node-declaring example packages found; the tree has far \
         more, so the classifier stopped recognising a shape"
    );
    assert!(
        missing.is_empty(),
        "{} of {counted} node-declaring example packages are not metadata-mode \
         candidates:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
}

/// Producer coverage, stated as a ledger rather than a silence.
///
/// Rust node packages have a producer (W1/W2). C and C++ ones do not until W3
/// lands, so they are counted and named here instead of being skipped quietly —
/// a component with no producer is a component whose entity count no bake can
/// know. When W3 lands, `unsupported` drops to zero and the assertion below
/// tightens from "these and only these" to "none".
#[test]
fn cpp_producer_gap_is_tracked_not_hidden() {
    let root = repo_root();
    let mut unsupported = Vec::new();
    for pkg in example_packages(&root) {
        if classify(&pkg) == Shape::NotANode {
            continue;
        }
        let Ok(ws) = Workspace::discover(&pkg) else {
            continue;
        };
        let Ok(decls) = ws.component_declarations() else {
            continue;
        };
        for decl in decls {
            if decl.config.language != ComponentLanguage::Rust {
                let rel = pkg
                    .strip_prefix(&root)
                    .unwrap_or(&pkg)
                    .display()
                    .to_string();
                unsupported.push(format!("{rel} [{:?}]", decl.config.language));
            }
        }
    }
    unsupported.sort();
    unsupported.dedup();

    // The C/C++ node packages in the tree today. The number is asserted, not
    // the names, so adding a C++ example is not a spurious failure — but a
    // JUMP in the count means a whole family lost its producer coverage
    // without anyone deciding to accept that.
    assert!(
        unsupported.len() <= 12,
        "{} example node packages have no metadata producer (phase-307 W3 \
         covers C/C++); if this grew, C/C++ examples are outpacing the \
         producer:\n  {}",
        unsupported.len(),
        unsupported.join("\n  ")
    );
}
