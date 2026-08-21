//! Phase 212.M.12 — examples/ canonical-shape regression.
//!
//! Walks every `package.xml`-bearing leaf under `examples/` and asserts
//! the post-M canonical shape:
//!
//! * **every example leaf has a `package.xml`** (the `<exec_depend>`
//!   SSoT for codegen / bringup verb).
//! * **Node XOR Application classification** — every example
//!   Rust crate carries exactly one of `[package.metadata.nros.
//!   component]`, `[package.metadata.nros.application]`, or
//!   `[package.metadata.nros.entry]` (Phase 212.N.6 rename).
//! * **`<pkg>::<Class>` class string** — Node pkgs' `class` field
//!   starts with the Cargo `[package].name`-mangled identifier so
//!   codegen + humans land in the same crate (L.4 lint).
//! * **deploy target matches platform path** — every key under
//!   `[package.metadata.nros.deploy.<target>]` matches the platform
//!   that the example lives under (e.g. `qemu-arm-nuttx/*` → `nuttx`).
//! * **Path A bringup dirs free of code** — any dir holding
//!   `system.toml` carries neither `Cargo.toml` nor `CMakeLists.txt`
//!   nor `src/` (L.8 lint complement).
//! * **pre-212 files absent** — `nros.toml`, `component_nros.toml`,
//!   `gen-app-config.py`, `app_config.h.in`, `Kconfig`, `Make.defs`
//!   never live in a migrated example dir (M.10 cleanup gate).
//! * **no committed `metadata/*.json`** — the codegen build artifact is
//!   gitignored; a tracked one is a mistake (phase-329 W6, folded in from
//!   the retired `examples_canonical_shape.rs`).
//!
//! ### Per-wave skip policy
//!
//! Not every example tree has been migrated. Per the Phase 212.M
//! table, the following sub-trees are deliberately skipped (with a
//! `[SKIPPED]` reason so CI is auditable):
//!
//! * `examples/qemu-esp32-baremetal/` — M.7 BLOCKED (ESP-IDF). M.7
//!   fix landed at `e4204459a` (Arc swap) but the sweep itself hasn't
//!   migrated the example yet.
//! * `examples/qemu-arm-baremetal/` — bare-metal Cortex-M3, not in
//!   the M sweep table.
//! * `examples/qemu-riscv64-threadx/` — M.6 covered `threadx-linux/`
//!   only; `qemu-riscv64-threadx/` is not in the sweep.
//! * `examples/threadx-linux/c/` — M.6 covered `threadx-linux/{rust,
//!   cpp}/` only; the `c/` sub-tree remains pre-212.
//! * `examples/templates/` — sibling category (per Phase 131), not a
//!   migrated example surface.
//!
//! Adding a directory to the skip set without lifting the underlying
//! migration block requires a phase-doc update + a comment in
//! `IS_MIGRATED_WAVE` below.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Paths git TRACKS under `examples/`, relative to the project root.
///
/// An index lookup, never a walk (the repo rule `check-no-tracked-file-find`
/// enforces, and the same defect class as issue 0416): the working tree is full
/// of legitimate build output, so "the file exists" and "the file is committed"
/// are different questions. A gate that asks the first when it means the second
/// fires on every machine where the build has run.
fn tracked_example_paths() -> std::collections::HashSet<PathBuf> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(nros_tests::project_root())
        .args(["ls-files", "--", "examples"])
        .output()
        .expect("git ls-files -- examples");
    assert!(
        out.status.success(),
        "git ls-files failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let set: std::collections::HashSet<PathBuf> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(PathBuf::from)
        .collect();
    // A silent-empty result would make every "is it committed?" gate below pass
    // vacuously — the failure mode those gates exist to prevent.
    assert!(
        !set.is_empty(),
        "git tracks no files under examples/ — refusing to run the committed-file \
         gates against an empty set"
    );
    set
}

fn examples_dir() -> PathBuf {
    nros_tests::project_root().join("examples")
}

/// Recursively walk a directory skipping common build artefact dirs.
fn walk(root: &Path, mut visit: impl FnMut(&Path)) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        visit(&dir);
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if nros_tests::treewalk::is_skipped_dir(name) {
                continue;
            }
            stack.push(path);
        }
    }
}

/// Every dir under `examples/` containing a `package.xml`.
fn discover_example_leaves() -> Vec<PathBuf> {
    let mut leaves = Vec::new();
    walk(&examples_dir(), |dir| {
        if dir.join("package.xml").is_file() {
            leaves.push(dir.to_path_buf());
        }
    });
    leaves.sort();
    leaves
}

/// Every dir under `examples/` containing a `system.toml` (Path A
/// bringup pkg).
fn discover_bringup_dirs() -> Vec<PathBuf> {
    let mut bringups = Vec::new();
    walk(&examples_dir(), |dir| {
        if dir.join("system.toml").is_file() {
            bringups.push(dir.to_path_buf());
        }
    });
    bringups.sort();
    bringups
}

// ---------------------------------------------------------------------------
// Wave / skip policy
// ---------------------------------------------------------------------------

/// Top-level example trees that have been migrated through the Phase
/// 212.M sweep. Anything outside this list is skipped (with a
/// `[SKIPPED]` reason printed via `nros_tests::skip!`).
const MIGRATED_PREFIXES: &[&str] = &[
    // M.1 native/rust + M.2 native/cpp
    "examples/native/",
    // M.3 Zephyr/rust (C+C++ Zephyr DEFERRED to H.1 follow-up — skip
    // those sub-trees with the rust-only filter below).
    "examples/zephyr/rust/",
    // M.4 NuttX rust/c/cpp
    "examples/qemu-arm-nuttx/",
    // M.5 FreeRTOS rust/c/cpp (M.5.a + M.5.b landed)
    "examples/qemu-arm-freertos/",
    // M.6 ThreadX linux/{rust,cpp} — `c/` is NOT in M.6 scope; the
    // is_migrated() filter below carves it out.
    "examples/threadx-linux/rust/",
    "examples/threadx-linux/cpp/",
];

/// Suffix patterns inside an otherwise-migrated tree that are NOT
/// covered by the corresponding M.x sweep. Drawn from the explicit
/// per-wave DEFERRED entries in the Phase 212.M table.
const UNMIGRATED_LEAF_SUFFIXES: &[&str] = &[
    // M.1 native/rust covered talker / listener / service-* / action-*
    // / parameters / logging. NOT covered:
    "-rtic",
    "-async",
    // Variant families outside M.1's per-pkg list:
    "custom-msg",
    "custom-transport-listener",
    "custom-transport-talker",
    "lifecycle-node",
    "serial-listener",
    "serial-talker",
];

/// Trees explicitly NOT migrated; included here for documentation +
/// to give a precise `[SKIPPED]` message.
const UNMIGRATED_PREFIXES: &[(&str, &str)] = &[
    (
        "examples/qemu-esp32-baremetal/",
        "M.7 territory — ESP32 bare-metal, not in M sweep table",
    ),
    (
        "examples/qemu-arm-baremetal/",
        "bare-metal Cortex-M3 — not in M sweep table",
    ),
    (
        "examples/qemu-riscv64-threadx/",
        "M.6 covered threadx-linux only; qemu-riscv64-threadx not in sweep",
    ),
    (
        "examples/threadx-linux/c/",
        "M.6 covered threadx-linux/{rust,cpp} only; c/ remains pre-212",
    ),
    // M.13 (informal — sweep landed 2026-06-02) covered native/c via
    // package.xml + nano_ros_application() cmake fn. native/c is now
    // canonical-shape. Carve-out retired.
    // `examples/native/rust/bridge/` UNMIGRATED entry retired 2026-06-02:
    // the sole occupant (`tt-zenoh-to-xrce`) moved to `examples/bridges/`
    // per §212.L sibling-category rule. `examples/bridges/` carries no
    // `package.xml`, so discovery skips it without an explicit prefix.
    (
        "examples/templates/",
        "sibling category (Phase 131) — not a migrated example surface",
    ),
];

fn is_migrated(rel: &Path) -> bool {
    let s = rel.to_string_lossy();
    let s = s.as_ref();
    // Explicit un-migrated overrides take precedence. Match accepts
    // both `<rel>` and `<rel>/...` forms; trailing slashes in the
    // prefix table are normalised away.
    for (prefix, _reason) in UNMIGRATED_PREFIXES {
        let p = prefix.trim_end_matches('/');
        let p_no_examples = p.trim_start_matches("examples/");
        if s == p
            || s == p_no_examples
            || s.starts_with(&format!("{p}/"))
            || s.starts_with(&format!("{p_no_examples}/"))
        {
            return false;
        }
    }
    for prefix in MIGRATED_PREFIXES {
        let stripped = prefix.trim_start_matches("examples/");
        if s.starts_with(prefix) || s.starts_with(stripped) {
            // Within a migrated tree, exclude leaf suffixes the M.x
            // sweep explicitly deferred (e.g. *-rtic, *-async,
            // lifecycle-node, custom-*, serial-*).
            let leaf = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
            for suffix in UNMIGRATED_LEAF_SUFFIXES {
                if leaf.ends_with(suffix) || leaf == *suffix {
                    return false;
                }
            }
            return true;
        }
    }
    // Unknown tree → conservatively skip.
    false
}

fn rel_to_project(p: &Path) -> PathBuf {
    p.strip_prefix(nros_tests::project_root())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| p.to_path_buf())
}

// ---------------------------------------------------------------------------
// Cargo.toml parsing
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct ProofKindClassification {
    is_component: bool,
    is_application: bool,
    is_entry: bool,
    component_class: Option<String>,
    deploy_targets: BTreeSet<String>,
    package_name: Option<String>,
}

fn parse_cargo_toml(path: &Path) -> Result<ProofKindClassification, String> {
    let body = fs::read_to_string(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    // toml 0.9: the `FromStr` impl on `toml::Value` is value-shaped
    // (rejects top-level tables); use `toml::from_str` for full docs.
    let value: toml::Value =
        toml::from_str(&body).map_err(|e| format!("toml parse {}: {}", path.display(), e))?;

    let package_name = value
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(str::to_owned);

    let nros = value
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"));

    // Phase 212.N.12 — `node` is the canonical spelling for the
    // single-shape Node pkg surface; `component` is accepted as a
    // deprecated alias. Treat either as the "Node pkg" classification.
    // (`PackageMetadataNros::validate` in nros-cli rejects both at once;
    // M.12 inherits that mutex by virtue of accepting either, not both.)
    let component = nros.and_then(|n| n.get("component"));
    let node = nros.and_then(|n| n.get("node"));
    let application = nros.and_then(|n| n.get("application"));
    let entry = nros.and_then(|n| n.get("entry"));

    let component_class = component
        .or(node)
        .and_then(|c| c.get("class"))
        .and_then(|c| c.as_str())
        .map(str::to_owned);

    let mut deploy_targets = BTreeSet::new();
    if let Some(deploy_tbl) = nros.and_then(|n| n.get("deploy"))
        && let Some(tbl) = deploy_tbl.as_table()
    {
        for k in tbl.keys() {
            deploy_targets.insert(k.clone());
        }
    }

    Ok(ProofKindClassification {
        is_component: component.is_some() || node.is_some(),
        is_application: application.is_some(),
        is_entry: entry.is_some(),
        component_class,
        deploy_targets,
        package_name,
    })
}

/// Infer the canonical deploy-target name from the platform sub-dir
/// the example lives under.
fn expected_deploy_target_for(rel: &Path) -> Option<&'static str> {
    let s = rel.to_string_lossy();
    // M.1 + M.2 native examples use `[package.metadata.nros.application]`
    // with `deploy = ["native"]` (an array, not a subtable) — different
    // shape from the RTOS examples. Skip the subtable-keyed assertion
    // here; native classification is covered by Test 2.
    if s.contains("/native/") {
        None
    } else if s.contains("/qemu-arm-nuttx/") {
        Some("nuttx")
    } else if s.contains("/qemu-arm-freertos/") {
        Some("freertos")
    } else if s.contains("/zephyr/") {
        Some("zephyr")
    } else if s.contains("/threadx-linux/") {
        // Real key in tree is `threadx-linux`, not `threadx`.
        Some("threadx-linux")
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Test 1 — every example leaf has a package.xml
// ---------------------------------------------------------------------------

/// Issue #170 — every canonical leaf (`examples/<platform>/<language>/<case>`
/// carrying a `package.xml`) must ship a `README.md`.
///
/// The RFC-0026 copy-out contract hands a user a directory with *nothing above
/// it*, so the build/run instructions have to travel inside it; leaving them
/// only in the parent platform README means a copied-out `talker/` is mute.
/// Pages are generated by `scripts/docs/gen-example-readmes.py` (which never
/// overwrites a hand-written one). `workspaces/`, `templates/` and `bridges/`
/// keep their own README conventions and are out of scope here.
#[test]
fn every_canonical_leaf_has_readme() {
    const SKIP_TOP: &[&str] = &["workspaces", "templates", "bridges"];

    let mut missing = Vec::new();
    for leaf in discover_example_leaves() {
        let rel = rel_to_project(&leaf);
        let comps: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        // Only `examples/<platform>/<language>/<case>` — four components.
        if comps.len() != 4 || comps[0] != "examples" || SKIP_TOP.contains(&comps[1].as_str()) {
            continue;
        }
        if !leaf.join("README.md").is_file() {
            missing.push(rel);
        }
    }

    assert!(
        missing.is_empty(),
        "{} canonical example leaf/leaves ship no README.md — the copy-out \
         contract (RFC-0026 / #170) requires one. Run \
         `scripts/docs/gen-example-readmes.py`:\n{}",
        missing.len(),
        missing
            .iter()
            .map(|p| format!("  - {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// phase-338 W6 — every canonical Rust leaf is a **standalone workspace root**.
///
/// This is the other half of the portability story, and until now it was only
/// a convention. `example_portability` proves the copies are IDENTICAL, which
/// is the argument for not folding them into one canonical source; the reason
/// not to fold is that a user can `cp -r` a leaf out and build it (RFC-0026).
/// That second property had no test — only `every_canonical_leaf_has_readme`,
/// which checks the instructions travel, not that the thing still builds once
/// it lands somewhere else.
///
/// The mechanism is the empty `[workspace]` table each leaf `Cargo.toml`
/// carries. It does two jobs: makes the copied directory its own workspace
/// root, and stops the repo's outer workspace from adopting the leaf in place.
/// Drop it and the failure is indirect — cargo reports the leaf as a member of
/// whatever workspace it can find upward, which reads as a dependency or
/// feature-unification problem rather than a missing table.
///
/// Workspace MEMBERS are excluded — they belong to an enclosing workspace by
/// design and must NOT carry the table. That exclusion is decided STRUCTURALLY
/// (does an ancestor manifest declare `[workspace]`?) rather than by path name:
/// a first draft skipped anything under a `workspaces/` component and promptly
/// false-flagged `examples/templates/multi-node-workspace/src/*`, which is the
/// same kind of member living somewhere else. Ask the tree, not the path.
#[test]
fn every_standalone_rust_leaf_is_its_own_workspace_root() {
    /// Is some ancestor of `leaf` (up to `examples/`) a workspace root?
    fn belongs_to_enclosing_workspace(leaf: &Path, examples: &Path) -> bool {
        let mut dir = leaf.parent();
        while let Some(d) = dir {
            if !d.starts_with(examples) {
                break;
            }
            let manifest = d.join("Cargo.toml");
            if let Ok(text) = std::fs::read_to_string(&manifest)
                && text.lines().any(|l| l.trim_end() == "[workspace]")
            {
                return true;
            }
            dir = d.parent();
        }
        false
    }

    let examples = examples_dir();
    let mut missing = Vec::new();
    for leaf in discover_example_leaves() {
        let rel = rel_to_project(&leaf);
        if belongs_to_enclosing_workspace(&leaf, &examples) {
            continue;
        }
        let manifest = leaf.join("Cargo.toml");
        if !manifest.is_file() {
            continue; // C/C++ leaves are CMake-driven; nothing to assert here.
        }
        let text = match std::fs::read_to_string(&manifest) {
            Ok(t) => t,
            Err(e) => {
                missing.push(format!("{} (unreadable: {e})", rel.display()));
                continue;
            }
        };
        if !text.lines().any(|l| l.trim_end() == "[workspace]") {
            missing.push(rel.display().to_string());
        }
    }

    assert!(
        missing.is_empty(),
        "{} standalone example leaf/leaves carry no `[workspace]` table, so a \
         copied-out copy would be adopted by whatever workspace sits above it \
         (RFC-0026 copy-out contract; phase-338 W6 kept the per-platform copies \
         BECAUSE copy-out works — this is what makes that true):\n{}",
        missing.len(),
        missing
            .iter()
            .map(|p| format!("  - {p}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn every_example_leaf_has_package_xml() {
    // The discovery itself filters on package.xml presence, so the
    // assertion is sympathetic: every Cargo.toml or CMakeLists.txt in
    // a recognisable example dir must sit next to a package.xml.
    let mut missing = Vec::new();
    walk(&examples_dir(), |dir| {
        let has_cargo = dir.join("Cargo.toml").is_file();
        let has_cmake = dir.join("CMakeLists.txt").is_file();
        if !(has_cargo || has_cmake) {
            return;
        }
        let rel = rel_to_project(dir);
        if !is_migrated(&rel) {
            return;
        }
        // Bringup dirs explicitly should NOT carry source; skip them
        // here (handled by `path_a_bringup_dirs_have_no_source`).
        if dir.join("system.toml").is_file() {
            return;
        }
        // Phase 244 (D1/C5 entry+node-pkg shape) — an entry-only carrier crate
        // (`[package.metadata.nros.entry]`, node logic + interface deps in a
        // sibling node pkg) carries no `package.xml`: the node pkg is the
        // interface SSoT. Exempt it. A self-pkg crate that ALSO declares
        // `[…node]`/`[…application]` still needs its own `package.xml`.
        if has_cargo
            && let Ok(cls) = parse_cargo_toml(&dir.join("Cargo.toml"))
            && cls.is_entry
            && !cls.is_component
            && !cls.is_application
        {
            return;
        }
        if !dir.join("package.xml").is_file() {
            missing.push(rel);
        }
    });
    assert!(
        missing.is_empty(),
        "migrated example dirs missing package.xml:\n  {}",
        missing
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Test 2 — Node XOR Application classification
// ---------------------------------------------------------------------------

#[test]
fn component_or_application_classification_present() {
    let mut bad: Vec<(PathBuf, &'static str)> = Vec::new();
    for leaf in discover_example_leaves() {
        let rel = rel_to_project(&leaf);
        if !is_migrated(&rel) {
            continue;
        }
        let cargo = leaf.join("Cargo.toml");
        if !cargo.is_file() {
            // C / C++ leaves don't carry a `[package.metadata.nros.*]`
            // table — their classification rides on the cmake fn
            // (`nano_ros_component()` / `nano_ros_application()`)
            // invoked by `CMakeLists.txt`. Asserted by Test 6.
            continue;
        }
        let cls = match parse_cargo_toml(&cargo) {
            Ok(c) => c,
            Err(e) => {
                bad.push((rel, Box::leak(e.into_boxed_str())));
                continue;
            }
        };
        // Phase 212.N.6 added `[package.metadata.nros.entry]` as the
        // renamed-from-application shape for Entry pkgs (post-N.7).
        //
        // The SSoT is the CLI schema `PackageMetadataNros::validate`
        // (cargo_metadata_schema.rs): it makes {component/node, application}
        // mutually exclusive but DELIBERATELY leaves `entry` out of that
        // mutex. A collapsed self-dispatching Entry crate (issue-0100 W1–W7
        // Entry/Node collapse) legitimately declares BOTH
        // `[package.metadata.nros.entry]` (its deploy board) and
        // `[package.metadata.nros.node]` (the node it registers via
        // `nros::node!(…)` in the same crate). Mirror the CLI rule here:
        // `application` must stand alone; `entry` MAY coexist with a
        // node/component; every leaf must classify as at least one shape.
        if cls.is_application && (cls.is_component || cls.is_entry) {
            bad.push((rel, "declares application together with node/entry"));
        } else if !cls.is_component && !cls.is_application && !cls.is_entry {
            bad.push((rel, "declares NEITHER component nor application/entry"));
        }
        // else: node alone, application alone, entry alone, or the collapsed
        // node+entry — all valid per the CLI schema.
    }
    assert!(
        bad.is_empty(),
        "Node/Application classification failures:\n  {}",
        bad.iter()
            .map(|(p, why)| format!("{} — {}", p.to_string_lossy(), why))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Test 3 — <pkg>::<Class> class-string convention (L.4 lint surface)
// ---------------------------------------------------------------------------

#[test]
fn component_class_strings_match_package_name() {
    let mut mismatches = Vec::new();
    for leaf in discover_example_leaves() {
        let rel = rel_to_project(&leaf);
        if !is_migrated(&rel) {
            continue;
        }
        let cargo = leaf.join("Cargo.toml");
        if !cargo.is_file() {
            continue;
        }
        let Ok(cls) = parse_cargo_toml(&cargo) else {
            continue;
        };
        if !cls.is_component {
            continue;
        }
        let pkg = cls.package_name.as_deref().unwrap_or("");
        let class = cls.component_class.as_deref().unwrap_or("");
        // Cargo package names are `kebab-case` or `snake_case`; the
        // Rust module path mangles `-` → `_`. The class field carries
        // the Rust module-path form, so compare with `-` → `_`.
        let pkg_module = pkg.replace('-', "_");
        if !class.starts_with(&format!("{}::", pkg_module)) {
            mismatches.push(format!(
                "{}: class='{}' does not start with '{}::'",
                rel.to_string_lossy(),
                class,
                pkg_module
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "component class string mismatches (L.4 lint surface):\n  {}",
        mismatches.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Test 4 — deploy.<target> matches platform path
// ---------------------------------------------------------------------------

#[test]
fn deploy_targets_match_platform_path() {
    let mut mismatches = Vec::new();
    for leaf in discover_example_leaves() {
        let rel = rel_to_project(&leaf);
        if !is_migrated(&rel) {
            continue;
        }
        let cargo = leaf.join("Cargo.toml");
        if !cargo.is_file() {
            continue;
        }
        let Ok(cls) = parse_cargo_toml(&cargo) else {
            continue;
        };
        if cls.deploy_targets.is_empty() {
            // Application pkgs may omit deploy when they only ship
            // host-side; tolerated. Node pkgs without a deploy
            // table would be caught at codegen time, not here.
            continue;
        }
        let Some(expected) = expected_deploy_target_for(&rel) else {
            // STM32F4 + niche platforms — assertion skipped.
            continue;
        };
        if !cls.deploy_targets.contains(expected) {
            mismatches.push(format!(
                "{}: deploy targets {:?} do not include expected '{}'",
                rel.to_string_lossy(),
                cls.deploy_targets,
                expected
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "deploy-target/platform-path mismatches:\n  {}",
        mismatches.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Test 5 — Path A bringup dirs free of code (L.8 lint complement)
// ---------------------------------------------------------------------------

#[test]
fn path_a_bringup_dirs_have_no_source() {
    let mut leaks = Vec::new();
    for bringup in discover_bringup_dirs() {
        let rel = rel_to_project(&bringup);
        for forbidden in &["Cargo.toml", "CMakeLists.txt", "src"] {
            let p = bringup.join(forbidden);
            if p.exists() {
                leaks.push(format!(
                    "{} carries forbidden '{}' (Path A bringup must be metadata-only)",
                    rel.to_string_lossy(),
                    forbidden
                ));
            }
        }
    }
    assert!(
        leaks.is_empty(),
        "Path A bringup dirs leaking source/code:\n  {}",
        leaks.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Test 6 — pre-212 files forbidden in migrated example dirs (M.10 gate)
// ---------------------------------------------------------------------------

#[test]
fn pre_212_files_forbidden_in_migrated_examples() {
    // These files indicate pre-212 shapes; their continued presence
    // in a *migrated* tree is the M.10 cleanup gate.
    //
    // `Kconfig` and `Make.defs` are NuttX-specific pre-212 files; the
    // M.4 sweep dropped them per the M.4 acceptance line. `Makefile`
    // is also listed in the M.4 sweep but is a more generic name —
    // we still flag it inside the migrated `qemu-arm-nuttx/` tree,
    // but tolerate it elsewhere (e.g. NUTTX top-level makefiles).
    const ALWAYS_FORBIDDEN: &[&str] = &[
        "nros.toml",
        "component_nros.toml",
        "gen-app-config.py",
        "app_config.h.in",
    ];
    const NUTTX_FORBIDDEN: &[&str] = &["Kconfig", "Make.defs"];

    let tracked = tracked_example_paths();
    let mut violations = Vec::new();
    walk(&examples_dir(), |dir| {
        let rel = rel_to_project(dir);
        if !is_migrated(&rel) {
            return;
        }
        for forbidden in ALWAYS_FORBIDDEN {
            if dir.join(forbidden).is_file() {
                violations.push(format!("{}/{}", rel.to_string_lossy(), forbidden));
            }
        }
        if rel.to_string_lossy().contains("qemu-arm-nuttx/") {
            for forbidden in NUTTX_FORBIDDEN {
                if dir.join(forbidden).is_file() {
                    violations.push(format!("{}/{}", rel.to_string_lossy(), forbidden));
                }
            }
        }
        // M.10 list also names committed `metadata/*.json` (build
        // artifacts the codegen path used to drop next to a pkg).
        // They belong in `$OUT_DIR/nros-gen/` or `target/nros-metadata/`,
        // never tracked next to a Cargo.toml. Aligns with sibling
        // `phase212_examples_canonical_shape` test's same check.
        //
        // TRACKED, not present. `.gitignore:131` already ignores
        // `examples/**/metadata/*.json`, and `nros sync` WRITES one per Node
        // pkg — so testing for existence flagged 5+ legitimate build artifacts
        // on any machine where sync had run, while the thing the gate forbids
        // (a committed sidecar) is unreachable through that path. The message
        // always said "not committed"; now the check asks that.
        let metadata_dir = dir.join("metadata");
        if metadata_dir.is_dir()
            && let Ok(entries) = fs::read_dir(&metadata_dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let rel_path = rel_to_project(&path);
                if !tracked.contains(&rel_path) {
                    continue;
                }
                violations.push(format!(
                    "{}/metadata/{} (build artifact must live in target/, not committed)",
                    rel.to_string_lossy(),
                    path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                ));
            }
        }
    });
    assert!(
        violations.is_empty(),
        "pre-212 files survive in migrated example dirs (M.10 gate):\n  {}",
        violations.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Test 7 — un-migrated trees documented (status surface)
// ---------------------------------------------------------------------------

/// Surface-only test: prints the per-tree migration status so CI logs
/// document why certain sub-trees are skipped. Always passes — its
/// purpose is to make the skip set visible + auditable.
#[test]
fn unmigrated_trees_status_surface() {
    let mut found_any = false;
    for (prefix, reason) in UNMIGRATED_PREFIXES {
        let dir = nros_tests::project_root().join(prefix);
        if dir.exists() {
            found_any = true;
            println!("[STATUS] {} skipped: {}", prefix, reason);
        }
    }
    if !found_any {
        println!(
            "[STATUS] no un-migrated example trees present — \
             all M-table waves complete"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 8 — standalone leaves use the RFC-0048 ament shape (phase-287 W6/W7)
// ---------------------------------------------------------------------------

/// Platform trees whose standalone `{c,cpp}/<leaf>/CMakeLists.txt` are migrated to
/// the RFC-0048 ament shape. Native (W6 native) + every embedded family (W6
/// embedded: 49 canonical leaves, native-identical CMakeLists). Zephyr + workspace
/// are NOT here — they keep their own shapes until their migration waves land.
const AMENT_SHAPE_TREES: &[&str] = &[
    "native",
    "qemu-arm-freertos",
    "qemu-arm-nuttx",
    "qemu-riscv-nuttx",
    "qemu-riscv64-threadx",
    "threadx-linux",
];

/// Every `examples/<tree>/{c,cpp}/<leaf>/CMakeLists.txt` (for the migrated trees)
/// must be the RFC-0048 ament shape: `find_package(nano_ros REQUIRED)`, no leftover
/// interim/old-shape constructs (the `NANO_ROS_ROOT` resolve guard,
/// `nano_ros_bootstrap()`, the `nano_ros_entry()` verb, or a raw
/// `NanoRosBootstrap.cmake` include). Guards the W6 native + embedded migrations
/// against regression + a stray un-migrated leaf.
#[test]
fn standalone_leaves_use_ament_shape() {
    const FORBIDDEN: &[&str] = &[
        "nano_ros_bootstrap(",
        "if(NOT DEFINED NANO_ROS_ROOT",
        "NanoRosBootstrap.cmake",
        "nano_ros_entry(",
        "nano_ros_link(",
        // Retired post-287 — the deploy/rmw tuple lives in package.xml
        // `<export><nano_ros …/>` (RFC-0048 §4), never a cmake call.
        "nano_ros_deploy(",
    ];
    let mut bad: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for tree in AMENT_SHAPE_TREES {
        for lang in ["c", "cpp"] {
            let root = examples_dir().join(tree).join(lang);
            if !root.is_dir() {
                continue;
            }
            walk(&root, |dir| {
                let cml = dir.join("CMakeLists.txt");
                if !cml.is_file() {
                    return;
                }
                checked += 1;
                let rel = rel_to_project(&cml);
                let Ok(body) = fs::read_to_string(&cml) else {
                    bad.push(format!("{} — unreadable", rel.to_string_lossy()));
                    return;
                };
                if !body.contains("find_package(nano_ros") {
                    bad.push(format!(
                        "{} — missing `find_package(nano_ros REQUIRED)` (RFC-0048 ament shape)",
                        rel.to_string_lossy()
                    ));
                }
                for marker in FORBIDDEN {
                    if body.contains(marker) {
                        bad.push(format!(
                            "{} — carries superseded `{}` (re-run \
                             scripts/docs/migrate-example-cmake-ament.py)",
                            rel.to_string_lossy(),
                            marker
                        ));
                    }
                }
            });
        }
    }
    assert!(
        bad.is_empty(),
        "standalone leaves not in the RFC-0048 ament shape:\n  {}",
        bad.join("\n  ")
    );
    // Sanity: the migrated trees exist + were walked (guards a silent-empty pass if
    // the examples layout moves).
    assert!(
        checked >= 27,
        "expected >=27 migrated standalone leaves, walked only {checked} — layout moved?"
    );
}

/// phase-291 W4 (#211) — the zephyr-leaf Kconfig→`rustc-env` bake has ONE
/// implementation (`nros-zephyr-build`). Guard both directions:
///
/// - NO `examples/**/build.rs` may carry a copy of the retired ~81-line bake
///   (`bake_kconfig_str(` / `bake_kconfig_int(` / a local `fn kconfig_line` —
///   the copy markers of the pre-291 file), else the 13-way duplication (and
///   the XRCE-block drift it caused) creeps back with the next copy-paste.
/// - EVERY zephyr rust leaf build.rs (under `examples/zephyr/rust/` or a
///   `zephyr_entry*` pkg) must call the shared `bake_nros_config()` — a leaf
///   that drops the call silently regresses to the known-issue #17 empty
///   locator (multicast scouting, no `connect()` on native_sim NSOS).
#[test]
fn zephyr_leaf_buildrs_uses_shared_bake() {
    const COPY_MARKERS: &[&str] = &["bake_kconfig_str(", "bake_kconfig_int(", "fn kconfig_line"];
    let mut copies = Vec::new();
    let mut missing_call = Vec::new();
    let mut zephyr_leaves = 0usize;
    walk(&examples_dir(), |dir| {
        let build_rs = dir.join("build.rs");
        let Ok(body) = fs::read_to_string(&build_rs) else {
            return;
        };
        let rel = rel_to_project(&build_rs);
        for marker in COPY_MARKERS {
            if body.contains(marker) {
                copies.push(format!("{} (contains `{marker}`)", rel.display()));
            }
        }
        let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        // The workspace naming is TWO shapes, not one. `zephyr_entry` /
        // `zephyr_entry_robot1` are the plain entries; RFC-0066's feature
        // packages are `zephyr_rust_<feature>_entry`
        // (lifecycle/params/qos/safety). Matching only `zephyr_entry*` left
        // those four outside a guard whose doc comment says EVERY zephyr rust
        // leaf — they happened to call the shared bake, so the hole was silent.
        let is_zephyr_leaf = rel.starts_with("examples/zephyr/rust")
            || dir_name.starts_with("zephyr_entry")
            || (dir_name.starts_with("zephyr_") && dir_name.ends_with("_entry"));
        if is_zephyr_leaf {
            zephyr_leaves += 1;
            if !body.contains("nros_zephyr_build::bake_nros_config()") {
                missing_call.push(rel.display().to_string());
            }
        }
    });
    assert!(
        copies.is_empty(),
        "pre-291 bake copies under examples/ (use nros_zephyr_build::bake_nros_config()):\n  {}",
        copies.join("\n  ")
    );
    assert!(
        missing_call.is_empty(),
        "zephyr rust leaf build.rs missing the shared bake call:\n  {}",
        missing_call.join("\n  ")
    );
    // Guard a silent-empty pass — the floor is NOT a coverage target, it fires
    // when the discovery rule stops matching a shape.
    //
    // The floor moves with a real deletion or a real widening, never to make a
    // red go away. History: 13 (phase-291) -> 10 (phase-331 W3/W4 deleted four
    // themed micro-workspaces carrying a `zephyr_entry` leaf) -> 13 again here,
    // for two reasons that cancelled out to a red:
    //
    //   - `b169a0edb` (#537, phase-350 W3) retired `zephyr/rust/talker-aemv8r`
    //     with the rest of the FVP code, leaving SIX under `examples/zephyr/rust/`
    //     where the 10 assumed seven. A real deletion whose floor was not moved,
    //     so this test has been red on main since.
    //   - widening the rule above picks up the four `zephyr_rust_*_entry`
    //     feature packages it had been skipping.
    //
    // 6 (examples/zephyr/rust) + 3 (zephyr_entry, zephyr_entry_robot1,
    // realtime-rust's) + 4 (features/safety `zephyr_rust_*_entry`) = 13.
    assert!(
        zephyr_leaves >= 13,
        "expected >=13 zephyr rust leaf build.rs, walked only {zephyr_leaves} — layout moved?"
    );
}

// Test 11 — no committed `metadata/*.json` build artifacts (phase-329 W6: folded
// in from the retired `examples_canonical_shape.rs`, the ONLY check that file
// carried which this walker did not — its forbidden-file / taxonomy / class-prefix
// checks are already covered by tests 6 / 3 / 4 above, more precisely).
//
// `metadata/<node>.json` is generated into a package dir by a normal build and is
// gitignored (`examples/**/metadata/*.json`), so `is_file()` would be green where
// no build had run and red where one had — exactly backwards. The rule is about
// what is COMMITTED, so ask git, not the filesystem. Returns cleanly (no failure)
// on any git error: a violation must be positively demonstrated, never inferred
// from a broken query.
#[test]
fn no_committed_metadata_json_artifacts() {
    let root = nros_tests::project_root();
    let out = match std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["ls-files", "--", "examples/**/metadata/*.json"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        Ok(_) => nros_tests::skip!("git ls-files failed (non-zero) — cannot verify committed set"),
        Err(e) => nros_tests::skip!("git unavailable ({e}) — cannot verify committed set"),
    };
    let tracked: BTreeSet<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    assert!(
        tracked.is_empty(),
        "committed `metadata/*.json` build artifact(s) — these are generated into \
         the package dir by a build (gitignored `examples/**/metadata/*.json`) and \
         must NOT be tracked; they live in $OUT_DIR/nros-gen/ or target/nros-metadata/:\n  {}",
        tracked.into_iter().collect::<Vec<_>>().join("\n  ")
    );
}
