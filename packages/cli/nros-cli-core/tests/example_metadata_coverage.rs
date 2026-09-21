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

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::orchestration::{source_metadata::ComponentLanguage, workspace::Workspace};
use ros_launch_manifest_model::SystemModel;

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
    // phase-445 W3 (RFC-0098 D3/D8) — a converted single-package Rust leaf
    // declares its node as a `[[component]]` row in the `system.toml` beside
    // its manifest, and `Workspace::component_declarations()` reads it from
    // there. Classifying it NotANode would silently stop checking exactly the
    // leaves that moved.
    if !cargo.is_empty() && read("system.toml").contains("[[component]]") {
        return Shape::RustNode;
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

/// phase-459 W0 - the derived-tiers fixture is what every later wave's gate
/// runs against, so this pins the two inputs those gates assume it carries.
///
/// `examples/workspaces/derived-tiers-cpp` mirrors the Autoware Safety Island:
/// four `SHAPE rclcpp` C++ components, one wall timer each, two at 30 Hz and
/// two at 10 Hz, `CALLBACK_GROUPS main` on every registration, and NO
/// `[tiers.*]` or `group_tiers` in its `system.toml`. Issue 1426 measured that
/// on such a workspace the rate-monotonic derivation is unreachable from any
/// authored input; W1 and W2 make it reachable, and their gates run here.
///
/// Two facts are checked, one per source:
///
/// 1. Discovery yields the four declarations (the precondition for
///    `nros sync`, as the gate above requires of every node package), and each
///    `CMakeLists.txt` carries the keyword. The keyword is checked as TEXT
///    because the static parser deliberately skips `CALLBACK_GROUPS` on
///    `nros_components_register_node` (`workspace.rs`, `SKIPN`): its one
///    consumer today reads the configure-time `nros-metadata.json`, which a
///    test does not produce (this repo does not compile inside tests). The W0
///    commit records the configure that verified the four
///    `"callback_groups": ["main"]` rows.
/// 2. The launch file and its `system.contract.yaml` sidecar resolve through
///    the pinned resolver into a model with four nodes, four timer-driven
///    paths (empty `input`), the 30/10 Hz publish rates the ranker orders by,
///    and no execution tiers - the input shape `derive_tiers_from_contracts`
///    keys on.
#[test]
fn derived_tiers_cpp_fixture_declares_four_groupful_components_and_resolves() {
    let root = repo_root();
    let ws = root.join("examples/workspaces/derived-tiers-cpp");
    // (package, node name, published endpoint, rate in Hz)
    const EXPECTED: [(&str, &str, &str, f64); 4] = [
        (
            "emergency_stop_pkg",
            "mrm_emergency_stop_operator",
            "control_cmd",
            30.0,
        ),
        ("stop_mode_pkg", "stop_mode_operator", "control", 30.0),
        (
            "comfortable_stop_pkg",
            "mrm_comfortable_stop_operator",
            "status",
            10.0,
        ),
        ("mrm_handler_pkg", "mrm_handler", "mrm_state", 10.0),
    ];

    for (pkg, node, _, _) in EXPECTED {
        let dir = ws.join("src").join(pkg);
        let discovered =
            Workspace::discover(&dir).unwrap_or_else(|e| panic!("{pkg}: discover failed: {e}"));
        let decls = discovered
            .component_declarations()
            .unwrap_or_else(|e| panic!("{pkg}: declarations failed: {e}"));
        let decl = decls
            .iter()
            .find(|d| d.config.package == pkg && d.config.component == node)
            .unwrap_or_else(|| panic!("{pkg}: no declaration for `{node}` among {decls:?}"));
        assert_eq!(decl.config.language, ComponentLanguage::Cpp, "{pkg}");
        assert_eq!(
            decl.shape.as_deref(),
            Some("rclcpp"),
            "{pkg}: the island's shape"
        );

        // Comment-stripped, as `classify` reads verbs: a keyword in a comment
        // is not a declaration.
        let calls: String = fs::read_to_string(dir.join("CMakeLists.txt"))
            .expect("CMakeLists.txt")
            .lines()
            .map(|l| l.split('#').next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            calls.contains("nros_components_register_node(")
                && calls.contains("CALLBACK_GROUPS main"),
            "{pkg}: the registration must declare `CALLBACK_GROUPS main`:\n{calls}"
        );
    }

    let system = fs::read_to_string(ws.join("src/demo_bringup/system.toml")).expect("system.toml");
    let authored: Vec<&str> = system
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| l.starts_with("[tiers.") || l.starts_with("group_tiers"))
        .collect();
    assert!(
        authored.is_empty(),
        "the fixture authors no tier and no binding; found {authored:?}"
    );

    let model = resolve_through_pinned_resolver(&root, &ws.join("src/demo_bringup"), "system");
    assert_eq!(
        model.structure.nodes.len(),
        4,
        "four nodes: {:?}",
        model.structure.nodes.keys().collect::<Vec<_>>()
    );
    assert!(
        model.execution.tiers.is_empty(),
        "no execution tier may reach the model from a workspace that authors none: {:?}",
        model.execution.tiers.keys().collect::<Vec<_>>()
    );
    for (_, node, endpoint, rate) in EXPECTED {
        let fqn = format!("/{node}");
        assert!(model.structure.nodes.contains_key(&fqn), "{fqn} resolved");
        let path = model
            .contracts
            .node_paths
            .get(&format!("{fqn}/on_timer"))
            .unwrap_or_else(|| {
                panic!(
                    "{fqn}/on_timer resolved as a node path; have {:?}",
                    model.contracts.node_paths.keys().collect::<Vec<_>>()
                )
            });
        assert!(
            path.input.is_empty(),
            "{fqn}/on_timer is timer-driven, so its input is empty: {:?}",
            path.input
        );
        assert!(
            path.output.iter().any(|o| o.ends_with(endpoint)),
            "{fqn}/on_timer publishes {endpoint}: {:?}",
            path.output
        );
        let ep = format!("{fqn}/{endpoint}");
        let contract = model
            .contracts
            .pub_endpoints
            .get(&ep)
            .unwrap_or_else(|| panic!("{ep} has a publish contract"));
        assert_eq!(
            contract.min_rate_hz,
            Some(rate),
            "{ep}: the rate the derivation ranks by (issue 1372: read from \
             `min_rate_hz`, equal to the trigger rate on purpose)"
        );
    }
}

/// Resolve `<bringup>/launch/<stem>.launch.xml` (with its `<stem>.contract.yaml`
/// sidecar) through the pinned resolver, by ABSOLUTE path (issue 0285).
fn resolve_through_pinned_resolver(repo: &Path, bringup: &Path, stem: &str) -> SystemModel {
    let resolver = common::pinned_launch_resolver();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    // Repo rule: temp files live in `$project/tmp/`, not the system temp dir.
    let out = repo.join("tmp").join(format!(
        "derived-tiers-cpp-{stem}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&out).expect("create model out dir");
    let model = out.join("system_model.yaml");
    let output = std::process::Command::new(&resolver)
        .arg(bringup.join(format!("launch/{stem}.launch.xml")))
        .arg("--bringup-root")
        .arg(bringup)
        .arg("-o")
        .arg(&model)
        .output()
        .expect("spawn nros-launch-resolve");
    assert!(
        output.status.success(),
        "nros-launch-resolve failed for {stem}:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(&model).expect("read the resolved model");
    let _ = fs::remove_dir_all(&out);
    SystemModel::from_yaml_str(&text).expect("the resolved model parses")
}
