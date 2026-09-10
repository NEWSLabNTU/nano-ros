//! The launcher does not link the CLI's dependency graph — phase-443 W3.
//!
//! The acceptance clause says *"measured, not asserted"*, and that word is the
//! whole test. "It is a small crate" is a claim a manifest comment can make and
//! then quietly stop being true: `nros-launcher`'s manifest asks a reader not
//! to add dependencies, and nothing so far checks that anybody listened. A
//! dependency arrives one `cargo add` at a time, and the one that finally makes
//! the launcher unbuildable on a bare host will look exactly like the four
//! before it.
//!
//! So this reads the RESOLVE — `packages/cli/Cargo.lock`, the file that decides
//! what actually gets compiled — and walks the launcher's link closure out of
//! it. No `cargo` subprocess: the lock is a checked-in file, so this needs no
//! network, no registry and no build (the "no compilation inside tests" rule),
//! and it measures the same thing on a runner as on a laptop.
//!
//! ## Why the walk is seeded from the manifest, not from the lock's own node
//!
//! `Cargo.lock` records a WORKSPACE MEMBER's dev-dependencies in the same
//! `dependencies` list as its real ones — `tempfile` and its 20-crate rustix
//! subtree would be counted as things the launcher links, which they are not.
//! Registry packages have no dev-dependencies in the lock at all (cargo never
//! resolves them), so seeding the walk with the launcher's own
//! `[dependencies]` table gives exactly the closure that ends up in the binary.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// The `packages/cli` workspace root, from this crate's manifest directory.
fn cli_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("nros-launcher sits inside packages/cli")
        .to_path_buf()
}

/// `name -> dependency names`, read out of the lock.
fn lock_graph() -> BTreeMap<String, Vec<String>> {
    let raw = std::fs::read_to_string(cli_root().join("Cargo.lock")).expect("read Cargo.lock");
    let doc: toml::Value = toml::from_str(&raw).expect("parse Cargo.lock");
    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pkg in doc["package"].as_array().expect("[[package]] array") {
        let name = pkg["name"].as_str().expect("package name").to_string();
        let deps: Vec<String> = pkg
            .get("dependencies")
            .and_then(toml::Value::as_array)
            .map(|a| {
                a.iter()
                    // Entries are `"name"` or `"name version"` or
                    // `"name version (source)"` — the name is the first word.
                    .filter_map(|d| d.as_str())
                    .map(|d| d.split_whitespace().next().unwrap_or(d).to_string())
                    .collect()
            })
            .unwrap_or_default();
        // A name can appear twice when two semver-incompatible versions are
        // resolved; the union is the right answer for a reachability question.
        graph.entry(name).or_default().extend(deps);
    }
    graph
}

/// Everything reachable from `seeds`, `seeds` included.
fn closure(graph: &BTreeMap<String, Vec<String>>, seeds: &[String]) -> BTreeSet<String> {
    let mut seen: BTreeSet<String> = seeds.iter().cloned().collect();
    let mut queue: VecDeque<String> = seeds.iter().cloned().collect();
    while let Some(n) = queue.pop_front() {
        for c in graph.get(&n).into_iter().flatten() {
            if seen.insert(c.clone()) {
                queue.push_back(c.clone());
            }
        }
    }
    seen
}

/// The crates named in a manifest's `[dependencies]` table.
fn declared_dependencies(manifest: &std::path::Path) -> Vec<String> {
    let raw = std::fs::read_to_string(manifest)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest.display()));
    let doc: toml::Value = toml::from_str(&raw).expect("parse manifest");
    doc.get("dependencies")
        .and_then(toml::Value::as_table)
        .map(|t| t.keys().cloned().collect())
        .unwrap_or_default()
}

/// The launcher's link closure, computed the way the module header describes.
fn launcher_closure(graph: &BTreeMap<String, Vec<String>>) -> BTreeSet<String> {
    let seeds =
        declared_dependencies(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"));
    assert!(
        !seeds.is_empty(),
        "the launcher's own [dependencies] table could not be read — this test \
         would then measure nothing and pass"
    );
    closure(graph, &seeds)
}

/// A ceiling, not the exact number.
///
/// Exact would fail on any transitive bump in `serde` or `toml` and teach the
/// next person to update the constant without reading it, which is how a
/// ratchet becomes a rubber stamp. The measured closure at the time of writing
/// is **27**; the headroom absorbs a transitive bump and nothing like a new
/// direct dependency (`clap` alone is 12).
const CEILING: usize = 45;

/// The clause itself: the CLI's graph is not in the launcher's.
///
/// Named crates rather than only a count, because the count could be met while
/// linking exactly the wrong thing — `nros-cli-core` is one edge and 192
/// crates, and a `nros-launcher` that grew a convenience import of it would
/// still be "one dependency more".
#[test]
fn the_launcher_does_not_link_the_clis_dependency_graph() {
    let graph = lock_graph();
    let launcher = launcher_closure(&graph);

    for banned in [
        // The toolchain itself, and the reason this crate was split out.
        "nros-cli-core",
        // Argument parsing: RFC-0095 D8 — the launcher parses no `argv`, so a
        // `clap` here would be evidence that it started to.
        "clap",
        // Templating, package discovery, launch parsing: the toolchain's work,
        // none of which a launcher can have a reason to do.
        "minijinja",
        "nros-pkg-index",
        "nros-launch-parser",
        "nros-entry-lower",
    ] {
        assert!(
            !launcher.contains(banned),
            "`{banned}` is in the launcher's link closure. The launcher must \
             build and run on a host with no toolchain installed, and each of \
             these is a piece of the toolchain.\nclosure: {launcher:?}"
        );
    }
}

/// The ratchet. The clause above says *which* crates are forbidden; this says
/// the closure did not grow past the point where "deliberately tiny" stops
/// describing it — a dependency nobody thought to ban is still a dependency.
#[test]
fn the_launcher_closure_stays_small_and_is_a_fraction_of_the_toolchains() {
    let graph = lock_graph();
    let launcher = launcher_closure(&graph);
    let toolchain = closure(&graph, &["nros-cli-core".to_string()]);

    assert!(
        launcher.len() <= CEILING,
        "the launcher's link closure is {} crates (ceiling {CEILING}). Adding a \
         dependency here is a decision about whether `nros` still starts on a \
         host with nothing installed — make it deliberately, in the manifest's \
         own words, and move this number with a reason.\nclosure: {launcher:?}",
        launcher.len()
    );
    // The ratio is what RFC-0097 D4 is about: the launcher must be able to
    // outlive the toolchain, which it cannot do while sharing its graph.
    assert!(
        launcher.len() * 3 <= toolchain.len(),
        "the launcher links {} crates and the toolchain {} — the split has \
         stopped buying anything",
        launcher.len(),
        toolchain.len()
    );
}
