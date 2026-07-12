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

fn project_root() -> PathBuf {
    nros_tests::project_root()
}

fn examples_dir() -> PathBuf {
    project_root().join("examples")
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
            match name {
                "target" | "build" | "generated" | ".cargo" | "node_modules" | ".git" => continue,
                n if n.starts_with("build-") || n.starts_with("target-") => continue,
                _ => stack.push(path),
            }
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
    // stm32f4/rust/{talker, *-rtic} migrated 2026-06-02 — they
    // carry [package.metadata.nros.application] deploy = ["stm32f4"]
    // (M.11-equivalent sweep). The Embassy variant stays carved out
    // in UNMIGRATED_PREFIXES below pending M-F.5 async-Node work.
    "examples/stm32f4/rust/",
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
    // stm32f4/rust/{talker, *-rtic} migrated 2026-06-02 by adding
    // [package.metadata.nros.application] deploy = ["stm32f4"].
    // Carve-out narrowed to the Embassy variant only.
    (
        "examples/stm32f4/rust/talker-embassy/",
        "stm32f4 Embassy variant — pre-212 shape, no package.xml; \
         falls under M-F.5 async-Node work",
    ),
    // Issue #34 — listener-embassy is the Embassy *listener* variant, same
    // category as talker-embassy above (pre-212 shape, non-linking — skip_build
    // in examples/fixtures.toml, known-issue #13). It was omitted from the
    // carve-out, so m12 wrongly required a package.xml for it.
    (
        "examples/stm32f4/rust/listener-embassy/",
        "stm32f4 Embassy variant — pre-212 shape, no package.xml; \
         falls under M-F.5 async-Node work",
    ),
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
    p.strip_prefix(project_root())
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
        let metadata_dir = dir.join("metadata");
        if metadata_dir.is_dir()
            && let Ok(entries) = fs::read_dir(&metadata_dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    violations.push(format!(
                        "{}/metadata/{} (build artifact must live in target/, not committed)",
                        rel.to_string_lossy(),
                        path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                    ));
                }
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
        let dir = project_root().join(prefix);
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
