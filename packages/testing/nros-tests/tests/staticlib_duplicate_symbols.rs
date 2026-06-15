//! Phase 241.D / RFC-0042 §D3 (issue #62, #70) — single-runtime link-determinism
//! validator.
//!
//! **History.** The pre-241.D3-rev model linked the C umbrella `libnros_c.a`
//! next to a STANDALONE RMW archive (`libnros_rmw_zenoh_staticlib.a`). Both were
//! self-contained Rust `crate-type=["staticlib"]` archives, so each bundled its
//! own copy of the shared dependency closure (nros-core, …) + the `nros_rmw_cffi`
//! C shim; the link reconciled the duplicates with `-Wl,--allow-multiple-definition`
//! — a blind mask that also hides real ODR violations. That old validator
//! diffed the duplicate set across the *pair* and asserted it was only shared-dep
//! bundling.
//!
//! **Now (241.D3-rev / phase-249 single-runtime).** The C umbrella bundles the
//! zenoh backend (rlib dep) into ONE archive (`cargo build -p nros-c --features
//! platform-posix,rmw-zenoh`). There is no second archive, so "duplicate symbols
//! across a pair" is moot — and the D3 goal it was a precondition for is now the
//! direct assertion: the single `libnros_c.a` links a host binary with
//! **`-u nros_rmw_zenoh_register`** and **NO `--allow-multiple-definition`**, with
//!   * the forced backend register entry actually included (`-u` did its job), and
//!   * exactly ONE cffi `REGISTRY` instance (a split registry is the #48
//!     `NoBackend` hazard).
//!
//! This is the host (posix) proxy for the cross C++ staticlib link; the dependency
//! closure is target-agnostic, so it is faithful + always reproducible. Consumes
//! the build-stage fixture (`scripts/build/link-determinism-fixture.sh` →
//! `build/link-determinism/libnros_c.a` + `.compile-ok`); skips when the fixture
//! or the host tools are absent (CLAUDE.md: skip on unmet preconditions, never
//! silent-pass).

use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root from packages/testing/nros-tests")
        .to_path_buf()
}

/// Resolve an available `nm` (prefer `llvm-nm`, fall back to GNU `nm`). The
/// symbol checks below match C symbols by exact name, so either tool works.
fn nm_tool() -> Option<String> {
    for nm in ["llvm-nm", "nm"] {
        if Command::new(nm).arg("--version").output().is_ok() {
            return Some(nm.to_string());
        }
    }
    None
}

fn tool(name: &str) -> Option<String> {
    Command::new(name)
        .arg("--version")
        .output()
        .ok()
        .map(|_| name.to_string())
}

/// The single-runtime fixture archive (`build/link-determinism/libnros_c.a`,
/// zenoh bundled), gated on the `.compile-ok` stamp.
fn single_archive(root: &std::path::Path) -> Option<PathBuf> {
    let fx = root.join("build/link-determinism");
    let lib = fx.join("libnros_c.a");
    (fx.join(".compile-ok").is_file() && lib.is_file()).then_some(lib)
}

/// Phase 241.D3-rev (issue #62 / #70) — the single-runtime link proof.
///
/// `--allow-multiple-definition` was needed only because the pre-D3 link pulled a
/// standalone RMW backend archive in with broad `--whole-archive` (to force its
/// register/ctor symbols), dragging in every member — including the shared
/// closure's strong defs, which then collided with the umbrella's copies. The
/// single-runtime fix bundles the backend INTO `libnros_c.a` and forces only the
/// backend register entry via `-u <symbol>`; there is one archive, one `std`, one
/// `REGISTRY`, so the host binary links with NO `--allow-multiple-definition`.
#[test]
fn single_archive_links_via_u_force_without_allow_multiple_definition() {
    let root = repo_root();
    let Some(cc) = tool("cc") else {
        nros_tests::skip!("cc not on PATH — D3 single-runtime link-proof needs a host C compiler");
    };
    let Some(lib) = single_archive(&root) else {
        nros_tests::skip!(
            "no single-runtime fixture archive (build/link-determinism/libnros_c.a) — run \
             `scripts/build/link-determinism-fixture.sh` first"
        );
    };

    let tmp = tempfile::tempdir().unwrap();
    let main_c = tmp.path().join("bare.c");
    std::fs::write(&main_c, "int main(void){return 0;}\n").unwrap();
    let exe = tmp.path().join("lkproof");

    // The single umbrella archive bundles the zenoh backend + the cffi C ABI
    // (`REGISTRY` + entry points), so the host binary links against it ALONE.
    // `-u nros_rmw_zenoh_register` forces the backend register entry (replacing
    // `--whole-archive`); NO `--allow-multiple-definition`.
    let out = Command::new(&cc)
        .arg(&main_c)
        .args(["-Wl,-u,nros_rmw_zenoh_register"])
        .arg(&lib)
        .args(["-lpthread", "-ldl", "-lm"])
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap_or_else(|e| panic!("spawn {cc}: {e}"));
    assert!(
        out.status.success(),
        "single-runtime host link FAILED with `-u nros_rmw_zenoh_register` and WITHOUT \
         `--allow-multiple-definition` — a real strong-symbol collision or a missing \
         definition in libnros_c.a (the single-runtime model should link clean):\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    // Symbol checks need an `nm`; skip them (link already proven) if absent.
    let Some(nm) = nm_tool() else {
        eprintln!(
            "D3 single-runtime: libnros_c.a links with `-u` + NO `--allow-multiple-definition` \
             (no nm on PATH — skipped the register/REGISTRY symbol assertions)."
        );
        return;
    };

    let syms = Command::new(&nm).arg(&exe).output().unwrap();
    let listing = String::from_utf8_lossy(&syms.stdout);
    assert!(
        listing
            .lines()
            .any(|l| l.ends_with(" T nros_rmw_zenoh_register")
                || l.ends_with(" t nros_rmw_zenoh_register")),
        "`-u nros_rmw_zenoh_register` did not pull the backend register entry into the image \
         — forcing the entry is the whole point of the `-u` replacement for `--whole-archive`",
    );
    let registry_defs = listing
        .lines()
        .filter(|l| {
            l.ends_with(" T REGISTRY")
                || l.ends_with(" D REGISTRY")
                || l.ends_with(" B REGISTRY")
                || l.ends_with(" t REGISTRY")
                || l.ends_with(" d REGISTRY")
                || l.ends_with(" b REGISTRY")
        })
        .count();
    assert_eq!(
        registry_defs, 1,
        "expected exactly ONE cffi `REGISTRY` instance in the linked image (single shared \
         registry), found {registry_defs} — a split registry is the #48 `NoBackend` hazard",
    );

    eprintln!(
        "D3 single-runtime: libnros_c.a (zenoh bundled) links with `-u nros_rmw_zenoh_register` \
         and NO `--allow-multiple-definition` — register entry included, exactly one REGISTRY. \
         The two-archive `--allow-multiple-definition` mask is retired."
    );
}
