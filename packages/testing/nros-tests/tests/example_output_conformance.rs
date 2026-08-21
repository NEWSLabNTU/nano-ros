//! phase-342 W7 — every example prints its ROLE's readiness marker.
//!
//! # Why this is a test and not a lint
//!
//! The tests wait on readiness through
//! [`nros_tests::process::ManagedProcess::expect_ready`], which resolves the
//! marker from [`nros_tests::output::ready_marker`]. This gate calls THE SAME
//! function. A shell gate would have to restate the table, and a restated table
//! is the thing that produced issue 0481 in the first place — nine call sites
//! each picking a marker by hand, six picking wrong, ~90 s of timeouts that
//! passed in silence.
//!
//! So: one table, two readers. Tests ask "is this example ready yet?", this gate
//! asks "can this example ever say it is?", and they cannot disagree because the
//! answer comes from one place.
//!
//! # What it enforces
//!
//! The examples are implementations of the SAME standard ROS demo. Their
//! DELIVERY lines already comply with it — `"Publishing: …"` and `"I heard: […]"`
//! mirror `demo_nodes_cpp`, pinned by `TALKER_LOG_PREFIX` / `LISTENER_LOG_PREFIX`
//! — but their READINESS lines are a nano-ros addition and had drifted into five
//! spellings for the listener role alone:
//!
//! ```text
//! "Subscriber created for topic: …"      8 files
//! "Subscriber created"                   6
//! "Subscription created for topic: …"    4
//! "Waiting for messages\n"               2
//! "Waiting for messages..."              1
//! ```
//!
//! The talker role is the control: 21 of 21 print `"Publishing: …"`. Convergence
//! is achievable; the listener was the outlier, not the rule.
//!
//! # The baseline
//!
//! Sources that predate this gate are listed in [`KNOWN_DIVERGENT`] so it lands
//! green and the backlog can only SHRINK. Converge a source, delete its line —
//! and if a listed source starts complying, the gate SAYS SO and fails, because
//! a baseline nobody prunes becomes a permanent exemption.

use nros_tests::{
    matrix::Lang,
    output::{DemoRole, ready_marker},
};
use std::{collections::BTreeSet, fs, path::PathBuf};

/// Sources that do not yet print their role's marker. SHRINKING — see the module
/// docs. Paths are repo-relative, exactly as `git ls-files` prints them.
///
/// Every entry here is a silent 30-second timeout waiting to happen the moment a
/// test waits on that binary's readiness, which is why the list is a backlog and
/// not a config.
const KNOWN_DIVERGENT: &[&str] = &[
    // EMPTY, and that is the point (phase-342 W7 complete): every example whose
    // directory names a role prints that role's readiness marker. The list stays
    // so a future divergence has an obvious, reviewable place to be recorded —
    // and the gate's second arm makes an entry that stops diverging fail, so it
    // cannot quietly become an exemption.
];

/// The role an example directory plays, from its NAME — the same vocabulary
/// `examples/README.md` uses. `None` means "no readiness contract here": clients,
/// bringup dirs, and the feature demos whose readiness is workload-specific.
fn role_of(dir_name: &str) -> Option<DemoRole> {
    match dir_name {
        "listener" => Some(DemoRole::Listener),
        "talker" => Some(DemoRole::Talker),
        "service-server" => Some(DemoRole::ServiceServer),
        "action-server" => Some(DemoRole::ActionServer),
        _ => None,
    }
}

/// The language of an example path, from the `examples/<platform>/<lang>/…`
/// convention. `None` for trees that do not carry a language segment
/// (`examples/workspaces/…`, `examples/templates/…`).
fn lang_of(parts: &[&str]) -> Option<Lang> {
    parts.iter().find_map(|p| match *p {
        "rust" => Some(Lang::Rust),
        "c" => Some(Lang::C),
        "cpp" => Some(Lang::Cpp),
        "mixed" => Some(Lang::Mixed),
        _ => None,
    })
}

/// Tracked example sources, grouped by the directory that owns them.
fn tracked_sources() -> Vec<PathBuf> {
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
    let files: Vec<PathBuf> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(PathBuf::from)
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("rs") | Some("c") | Some("cpp") | Some("cc")
            )
        })
        .collect();
    // A silent-empty result would make this gate pass vacuously — the failure
    // mode it exists to prevent (borrowed from `example_shape.rs`).
    assert!(
        !files.is_empty(),
        "git tracks no example sources — refusing to run the conformance gate \
         against an empty set"
    );
    files
}

/// Which `(dir, role, lang)` triples this gate is responsible for, and whether
/// any source under that dir prints the role's marker.
fn survey() -> Vec<(String, DemoRole, Lang, bool)> {
    let root = nros_tests::project_root();
    // Keyed by DIR alone: one example dir has one role and one language, so the
    // dir is the identity. (Also avoids requiring `Ord` on the enums, which
    // belongs to them for their own reasons, not for this gate's convenience.)
    let mut by_dir: std::collections::BTreeMap<String, (DemoRole, Lang, bool)> = Default::default();

    for rel in tracked_sources() {
        let s = rel.to_string_lossy().to_string();
        let parts: Vec<&str> = s.split('/').collect();
        // The owning example dir is the one holding `src/`.
        let Some(src_at) = parts.iter().position(|p| *p == "src") else {
            continue;
        };
        if src_at == 0 {
            continue;
        }
        let dir_parts = &parts[..src_at];
        let Some(role) = role_of(dir_parts[dir_parts.len() - 1]) else {
            continue;
        };
        let Some(lang) = lang_of(dir_parts) else {
            continue;
        };
        let dir = dir_parts.join("/");
        let marker = ready_marker(role, lang);
        let prints = fs::read_to_string(root.join(&rel))
            .map(|body| body.contains(marker))
            .unwrap_or(false);
        let entry = by_dir.entry(dir).or_insert((role, lang, false));
        entry.2 = entry.2 || prints;
    }

    by_dir
        .into_iter()
        .map(|(d, (r, l, ok))| (d, r, l, ok))
        .collect()
}

#[test]
fn every_example_prints_its_role_readiness_marker() {
    let baseline: BTreeSet<&str> = KNOWN_DIVERGENT.iter().copied().collect();
    let surveyed = survey();

    assert!(
        !surveyed.is_empty(),
        "surveyed no example dirs — the role/lang conventions this gate keys on \
         have moved, and it is now checking nothing"
    );

    let mut violations = Vec::new();
    let mut fixed_but_baselined = Vec::new();
    for (dir, role, lang, prints) in &surveyed {
        let baselined = baseline.contains(dir.as_str());
        match (prints, baselined) {
            (false, false) => violations.push(format!(
                "  {dir}\n      role {role:?} requires `{}` — no source under it prints that",
                ready_marker(*role, *lang)
            )),
            (true, true) => fixed_but_baselined.push(dir.clone()),
            _ => {}
        }
    }

    assert!(
        violations.is_empty(),
        "example(s) do not print their role's readiness marker (phase-342 W7):\n{}\n\n  \
         The test harness waits on these via `expect_ready(role, …)`, which reads the \n  \
         SAME table this gate does. An example that does not print its marker makes \n  \
         every such wait burn its full timeout — silently, before issue 0481's fix; \n  \
         loudly now. Add the line (ADDITIVELY — do not replace an existing banner, \n  \
         phase-277 broke ~10 tests that way), or baseline it in KNOWN_DIVERGENT.",
        violations.join("\n")
    );

    assert!(
        fixed_but_baselined.is_empty(),
        "these example dirs now DO print their marker — delete them from \
         KNOWN_DIVERGENT:\n  {}\n\n  A baseline nobody prunes becomes a permanent \
         exemption, which is the opposite of a backlog.",
        fixed_but_baselined.join("\n  ")
    );
}

/// The gate above can only be trusted if it is actually looking at something.
/// Asserted separately so a convention change (a renamed role dir, a moved
/// language segment) fails as a coverage loss rather than as a silent pass.
#[test]
fn the_conformance_gate_covers_every_role_it_claims() {
    let surveyed = survey();
    let roles: BTreeSet<String> = surveyed
        .iter()
        .map(|(_, r, _, _)| format!("{r:?}"))
        .collect();
    let langs: BTreeSet<String> = surveyed
        .iter()
        .map(|(_, _, l, _)| format!("{l:?}"))
        .collect();

    for want in ["Listener", "Talker"] {
        assert!(
            roles.contains(want),
            "the conformance survey found no {want} example — role vocabulary moved, \
             and the gate is now blind to that role. Roles seen: {roles:?}"
        );
    }
    for want in ["Rust", "C", "Cpp"] {
        assert!(
            langs.contains(want),
            "the conformance survey found no {want} example — the \
             `examples/<platform>/<lang>/` convention moved. Langs seen: {langs:?}"
        );
    }
}
