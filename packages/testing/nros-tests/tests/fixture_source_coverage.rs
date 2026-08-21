//! phase-350 W6 (issues 0535 / 0540) — every fixture SOURCE is a manifest row
//! or a tracked exception, beyond the `examples/` tree.
//!
//! `examples_fixture_coverage.rs` walks `examples/**` for `package.xml` and
//! gates those leaves. That is where the fixture surface USED to live, and it
//! is no longer where all of it lives — which is how three separate gaps hid:
//!
//! * `bins/int32-observer` sat retired-but-present for months with no row, no
//!   builder and no consumer (issue 0540). Nothing looked at `bins/`.
//! * `logging-smoke-zephyr-native-sim` and `ros-edition-pose-pub`, both LIVE
//!   fixtures in the same directory, had no row either — found only because
//!   0540's investigation enumerated the directory by hand.
//! * The 74 west fixtures were outside the manifest entirely (issue 0535); the
//!   `examples/` gate read their dirs as covered because it restated their
//!   matrix in its own constants.
//!
//! The rule this file enforces is the one those three share: **a fixture source
//! is a manifest row, or a tracked exception with a reason.** Both directions,
//! like its sibling — an uncovered source fails, and an exception that has come
//! true fails too, so the list cannot rot into a false negative (which is
//! exactly how `fixture-inventory.py` decayed, issue 0538).

use std::{collections::BTreeSet, fs};

/// Crates under `packages/testing/nros-tests/bins/` deliberately built outside
/// `examples/fixtures.toml`, each with the reason and the lane that owns it.
///
/// An entry here that GAINS a manifest row fails this test — a stale exception
/// is the failure mode, not a tidy-up.
const BINS_ALLOWLIST: &[(&str, &str)] = &[(
    "ros-edition-pose-pub",
    "RFC-0058 ROS-edition axis: built per distro by `just ros_editions \
     build-fixture` into build/ros-editions/<distro>-<rmw>, which is a \
     PER-RUN global (NROS_ROS_EDITION), not a fixture coordinate. Deliberately \
     gated out of `just ci` (needs docker + a built image).",
)];

/// Every `dir = "..."` value in the manifest, of every row kind.
fn manifest_dirs(root: &std::path::Path) -> BTreeSet<String> {
    let text = fs::read_to_string(root.join("examples/fixtures.toml"))
        .expect("read examples/fixtures.toml");
    let mut out = BTreeSet::new();
    for line in text.lines() {
        let l = line.trim();
        let Some(rest) = l.strip_prefix("dir") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        out.insert(
            rest.trim()
                .trim_matches('"')
                .trim_end_matches('/')
                .to_string(),
        );
    }
    out
}

/// Every crate under `packages/testing/nros-tests/bins/` is a manifest row or a
/// tracked exception — the hole issue 0540 fell through.
#[test]
fn every_test_bin_is_a_row_or_a_tracked_exception() {
    let root = nros_tests::project_root();
    let bins = root.join("packages/testing/nros-tests/bins");
    if !bins.is_dir() {
        nros_tests::skip!("bins dir missing at {}", bins.display());
    }

    let dirs = manifest_dirs(&root);
    let allow: BTreeSet<&str> = BINS_ALLOWLIST.iter().map(|(n, _)| *n).collect();

    let mut uncovered = Vec::new();
    let mut stale_exceptions = Vec::new();
    let mut present = BTreeSet::new();

    for e in fs::read_dir(&bins).expect("read bins/").flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        // A crate, not a stray directory.
        if !p.join("Cargo.toml").is_file() {
            continue;
        }
        present.insert(name.to_string());

        let rel = format!("packages/testing/nros-tests/bins/{name}");
        let covered = dirs.contains(&rel);
        let allowed = allow.contains(name);

        if !covered && !allowed {
            uncovered.push(name.to_string());
        }
        if covered && allowed {
            stale_exceptions.push(name.to_string());
        }
    }

    let dangling: Vec<&str> = BINS_ALLOWLIST
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| !present.contains(*n))
        .collect();

    let mut msg = String::new();
    if !uncovered.is_empty() {
        msg.push_str(&format!(
            "\n{} test bin(s) with NO `dir =` row in examples/fixtures.toml and no \
             tracked exception. A bin here can otherwise sit with no row, no builder \
             and no consumer indefinitely — that is issue 0540. Add a row, or add a \
             BINS_ALLOWLIST entry naming the lane that builds it:\n",
            uncovered.len()
        ));
        for u in &uncovered {
            msg.push_str(&format!("  - {u}\n"));
        }
    }
    if !stale_exceptions.is_empty() {
        msg.push_str(&format!(
            "\n{} BINS_ALLOWLIST entr(ies) now HAVE a manifest row (stale exception — \
             remove them; a list that keeps excusing what is already covered is how \
             `fixture-inventory.py` rotted, issue 0538):\n",
            stale_exceptions.len()
        ));
        for s in &stale_exceptions {
            msg.push_str(&format!("  - {s}\n"));
        }
    }
    if !dangling.is_empty() {
        msg.push_str(&format!(
            "\n{} BINS_ALLOWLIST entr(ies) name a crate that no longer exists:\n",
            dangling.len()
        ));
        for d in &dangling {
            msg.push_str(&format!("  - {d}\n"));
        }
    }

    assert!(
        msg.is_empty(),
        "phase-350 W6 test-bin coverage gate ({} crate(s) scanned, {} tracked \
         exception(s)):{}",
        present.len(),
        allow.len(),
        msg,
    );
}

/// Build roots that are deliberately NOT manifest rows, each with its reason.
///
/// This is the audit's residue, written down instead of remembered. Every entry
/// is a real artifact some test consumes, produced by a recipe rather than a
/// fixture row, and each is here because the alternative was worse — not
/// because nobody got to it. The test asserts they still exist as declared, so
/// a recipe that quietly stops producing one is caught.
const NON_MANIFEST_BUILD_ROOTS: &[(&str, &str)] = &[
    (
        "scripts/build/idf-fixtures.sh",
        "esp-idf smoke: `idf.py` needs a full IDF env and ~7 min/ELF, so the \
         esp32 lane self-gates and builds it best-effort. Consumed by \
         cli_bringup_esp_idf.rs.",
    ),
    (
        "just/ros-editions.just",
        "RFC-0058 edition axis: a PER-RUN global (NROS_ROS_EDITION), not a \
         fixture coordinate, and docker-gated out of `just ci`.",
    ),
    (
        "just/zephyr-setup.just",
        "FVP: the Arm model is license-gated ([gated.arm-fvp]), so nano-ros \
         never downloads it and no lane can build these. phase-350 W3 / \
         issue 0537 decides whether the two runnerless artifacts survive.",
    ),
];

/// The declared non-manifest producers must still exist. A renamed or deleted
/// recipe leaves this list describing a world that is gone.
#[test]
fn declared_non_manifest_build_roots_still_exist() {
    let root = nros_tests::project_root();
    let missing: Vec<&str> = NON_MANIFEST_BUILD_ROOTS
        .iter()
        .map(|(p, _)| *p)
        .filter(|p| !root.join(p).exists())
        .collect();
    assert!(
        missing.is_empty(),
        "{} declared non-manifest fixture producer(s) no longer exist — the \
         exception list describes a world that is gone:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
}
