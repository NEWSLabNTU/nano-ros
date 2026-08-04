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

    // Silent-empty guard, not a coverage target: it fires when the CLASSIFIER
    // stops recognising a shape, which looks identical to "there are no such
    // packages". The floor moves only when packages legitimately leave the tree
    // — phase-337 W7.a took the ten `examples/stm32f4/rust/*` packages (six of
    // them node-declaring `*_pkg` crates) out with their board, 75 -> 69.
    assert!(
        counted >= 65,
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

/// phase-308 W4 — every declared component has a producer.
///
/// This test used to be a LEDGER: C and C++ had no producer, so it counted them
/// and asserted the count had not grown. That premise is gone — phase-308's
/// CMake probe produces C/C++ sidecars, verified on
/// `examples/workspaces/cpp` — so the assertion inverts from "no more than N
/// unsupported" to "every component's language is producible".
///
/// What can still put a component out of reach is a PROPERTY of the package,
/// not its language: it may be deploy-bound (node and entry in one crate, so it
/// deps a board crate and cannot be host-compiled — issue 0288), or its build
/// may be un-configurable for the host (issue 0286's `probe_blocker`). Those
/// are reported by `nros sync` at the time, per component, with a reason. They
/// are not a language gap and this test is not the place for them.
#[test]
fn every_declared_component_language_has_a_producer() {
    let root = repo_root();
    let mut unproducible = Vec::new();
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
            // Rust → the cargo harness; C and C++ → the CMake probe. There is
            // no third case, and a new one must not land silently.
            let producible = matches!(
                decl.config.language,
                ComponentLanguage::Rust | ComponentLanguage::C | ComponentLanguage::Cpp
            );
            if !producible {
                let rel = pkg
                    .strip_prefix(&root)
                    .unwrap_or(&pkg)
                    .display()
                    .to_string();
                unproducible.push(format!("{rel} [{:?}]", decl.config.language));
            }
        }
    }
    unproducible.sort();
    unproducible.dedup();
    assert!(
        unproducible.is_empty(),
        "{} component(s) declare a language with no metadata producer:\n  {}",
        unproducible.len(),
        unproducible.join("\n  ")
    );
}
