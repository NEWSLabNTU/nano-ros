//! A fixture row's `rmw` must be what the artifact actually LINKED (issue 0831).
//!
//! `row_coord()` puts an RMW in every row's coordinate, `nros_lane` selects on
//! it, and tier 2 reports coverage per coordinate. None of that ever looked at
//! the binary. It was wrong for two rows and had been for as long as they
//! existed: `workspace-rust-native-cyclonedds` and `workspace-rust-native-xrce`
//! built ZENOH, because on the cargo driver the backend came from the
//! `nros sync` selection facade (off `[system] rmw`) and nothing consulted the
//! image the row named. Measured on the artifact at the time: 0 occurrences of
//! Cyclone's `dds_`, 777 of zenoh-pico's `_z_`.
//!
//! The issue's own prescription: "add a runtime assertion rather than trusting
//! the coordinate — the artifact knows". This is it. The claim is now checked
//! against the thing it describes, so a regression is a red here rather than a
//! silently green coordinate.
//!
//! **Scope: `[[workspace_fixture]]` rows, and each row's OWN binary.** Both
//! halves of that narrowing were learned by getting it wrong — the first cut
//! walked every executable under every row's artifact root and produced 18
//! findings, none of them this bug:
//!
//! * A BRIDGE legitimately links two backends. `bridge-zenoh-to-xrce-fwd` is
//!   zenoh on one side and XRCE on the other; "exclusivity" is not a rule that
//!   applies to it.
//! * A MULTI-ROW LEAF shares one `target/` across rows with different RMWs
//!   (issue 0517). `int32-sink` has an xrce row and a cyclonedds row over the
//!   same directory, so no single binary there can satisfy both, and blaming
//!   either is a false accusation.
//! * A RENAME leaves an ORPHAN. Repointing these two rows at their new images
//!   left the old `native_entry` beside the new `native_xrce_entry` — issue
//!   0215's class, and not what this gate is about.
//!
//! Naming the row's binary from the manifest (field 13's `<image>_entry`, or
//! field 4's `entry`) removes all three at once, and is the same derivation the
//! resolvers use.
//!
//! **Reads PREBUILT artifacts only** (AGENTS.md "No compilation inside tests").
//! A lane that built no readable artifact SKIPS loudly, never passes silently.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

/// Symbols that prove a backend is linked: the backend's own C namespace, which
/// nothing else defines. `(rmw name, symbol prefix)`.
const BACKENDS: &[(&str, &str)] = &[("zenoh", "_z_"), ("cyclonedds", "dds_"), ("xrce", "uxr_")];

/// Count all three namespaces in ONE `nm` pass.
///
/// One pass rather than one per backend, and it matters: the first cut ran up
/// to seven `nm` invocations per binary across every artifact tree and blew the
/// 60 s nextest timeout. `nm` on an 8 MB binary is not cheap and there are
/// hundreds of them.
fn backend_symbols(bin: &Path) -> Option<BTreeMap<&'static str, usize>> {
    let out = Command::new("nm").arg(bin).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(count_nm_output(&String::from_utf8_lossy(&out.stdout)))
}

/// Count each backend's namespace in `nm` output.
///
/// Split out from the `nm` invocation so it can be exercised against known
/// input — see `the_symbol_counter_reads_local_and_global_text_symbols`. This
/// is the part that was wrong twice while writing the gate, in both directions.
fn count_nm_output(text: &str) -> BTreeMap<&'static str, usize> {
    let mut counts: BTreeMap<&'static str, usize> = BACKENDS.iter().map(|(n, _)| (*n, 0)).collect();
    for line in text.lines() {
        // "<addr> <type> <symbol>". Parsed rather than substring-matched: the
        // type letter's CASE is the difference between a global symbol (`T`)
        // and a local one (`t`), and matching only `T` called the bridge
        // workspace's 350 local `dds_` symbols an absent backend. Static linking
        // decides which case a backend's symbols get; the gate must not.
        let mut parts = line.split_whitespace();
        let Some(sym) = parts.next_back() else {
            continue;
        };
        let Some(kind) = parts.next_back() else {
            continue;
        };
        if !kind.eq_ignore_ascii_case("t") {
            continue;
        }
        for (name, prefix) in BACKENDS {
            if sym.starts_with(prefix) {
                *counts.get_mut(name).expect("seeded above") += 1;
            }
        }
    }
    counts
}

/// The gate's negative control, on the normal path.
///
/// AGENTS.md "a gate must run its own selftest": *a negative control nobody
/// runs decays into a comment*. This one is not decorative — the counter was
/// wrong TWICE while the gate was being written, and each time silently wrong
/// in the direction that makes the gate useless:
///
/// * matching the substring `" T dds_"` missed LOCAL symbols, so a binary with
///   350 `t dds_` entries read as "links no cyclonedds";
/// * counting any line merely CONTAINING a prefix would match undefined
///   imports and data symbols, so a binary that only references a backend
///   would read as carrying it, and the gate could not fail.
///
/// So it asserts both directions against known input, every run.
#[test]
fn the_symbol_counter_reads_local_and_global_text_symbols() {
    let sample = "0000000000001000 T z_open\n\
                  0000000000001010 t _z_send_frame\n\
                  00000000000cb740 t dds_alloc\n\
                  0000000000142df0 T dds_create_participant\n\
                  00000000001e12a0 d cyclonedds_root_cfgelems\n\
                  0000000000002000 U uxr_run_session\n\
                  0000000000002010 T uxr_init_session\n\
                                   w some_weak_symbol\n";
    let c = count_nm_output(sample);

    // LOCAL (`t`) and GLOBAL (`T`) both count — the bug that called a linked
    // backend absent.
    assert_eq!(c["cyclonedds"], 2, "one `t dds_` + one `T dds_`");
    // `z_open` does not start with `_z_`; only the prefixed one counts.
    assert_eq!(c["zenoh"], 1, "only `_z_`-prefixed text symbols");
    // `U` is UNDEFINED — an import, not evidence the backend is linked; `d` is
    // data. Counting either would make the gate pass on a binary that merely
    // references a backend it does not carry.
    assert_eq!(c["xrce"], 1, "the `U` import must not count, the `T` must");

    // And the counter must be able to report ZERO, which is what the main
    // test's "links none of it" assertion keys on.
    let none = count_nm_output("0000000000001000 T unrelated_symbol\n");
    assert_eq!(none["cyclonedds"], 0);
    assert_eq!(none["zenoh"], 0);
    assert_eq!(none["xrce"], 0);
}

/// The binary this row declares, if it is built.
///
/// Named from the manifest rather than found by walking: a walk cannot tell a
/// row's own artifact from an orphan left by a rename or from a sibling row's
/// binary in a shared `target/`, and both mistakes read as this bug.
fn row_binary(fixture_id: &str, root: &Path) -> Option<PathBuf> {
    let record = nros_tests::fixtures::current_workspace_fixture_record(fixture_id).ok()?;
    let fields: Vec<&str> = record.split('\x1f').collect();
    // A GENERATED row names an image, whose target is `<image>_entry`; a
    // hand-written one names the entry directly. Same derivation as
    // `assert_generated_entry_name`.
    let name = match fields.get(13).filter(|f| !f.is_empty()) {
        Some(image) => format!("{image}_entry"),
        None => fields.get(4).filter(|f| !f.is_empty())?.to_string(),
    };
    // Two layouts: a cargo profile dir, or the top of a cmake binary dir.
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((d, depth)) = stack.pop() {
        let candidate = d.join(&name);
        if candidate.is_file() {
            return Some(candidate);
        }
        if depth >= 2 {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir()
                && !matches!(
                    e.file_name().to_string_lossy().as_ref(),
                    "deps" | "build" | "incremental" | ".fingerprint"
                )
            {
                stack.push((p, depth + 1));
            }
        }
    }
    None
}

/// Does this workspace declare a `[[bridge]]`?
///
/// A bridge links TWO backends on purpose — `from = "zenoh:zen"`,
/// `to = "cyclonedds:dds"` — so the exclusivity half of this gate does not
/// apply to it. Read from the bringup rather than kept as a list of row ids:
/// the declaration is the reason, and a list would need editing every time a
/// bridge workspace is added or renamed.
///
/// The PRESENCE half still applies, and should: a bridge row declaring
/// `cyclonedds` and linking none of it is the same defect as anywhere else.
fn declares_a_bridge(ws_dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(ws_dir.join("src")) else {
        return false;
    };
    rd.flatten().any(|e| {
        std::fs::read_to_string(e.path().join("system.toml"))
            .map(|t| t.lines().any(|l| l.trim() == "[[bridge]]"))
            .unwrap_or(false)
    })
}

#[test]
fn a_rows_rmw_is_the_backend_its_artifact_linked() {
    let mut checked = 0usize;
    let mut unreadable = 0usize;
    let mut wrong: Vec<String> = Vec::new();

    for row in nros_tests::fixtures::lane::manifest_rows() {
        if row.kind != "workspace_fixture" {
            continue;
        }
        let declared = row.coord.2.as_str();
        // Only the backends with a symbol signature. `uorb` and friends are not
        // skipped quietly for convenience — they have no C namespace to key on,
        // and inventing one would be a check that cannot fail.
        if !BACKENDS.iter().any(|(n, _)| *n == declared) {
            continue;
        }
        let root = nros_tests::project_root().join(&row.artifact_root);
        if !root.is_dir() {
            continue; // not built for this lane
        }

        {
            let Some(bin) = row_binary(&row.id, &root) else {
                continue; // not built for this lane
            };
            let Some(counts) = backend_symbols(&bin) else {
                unreadable += 1;
                continue;
            };
            // No backend symbols at all: a stripped binary, or a helper that
            // links none. Reading that as "the declared backend is missing"
            // would be a false accusation.
            if counts.values().sum::<usize>() == 0 {
                unreadable += 1;
                continue;
            }
            checked += 1;

            if counts[declared] == 0 {
                wrong.push(format!(
                    "{}: row declares rmw `{declared}`, but {} links none of it",
                    row.label(),
                    bin.display()
                ));
                continue;
            }
            // Exclusivity. Two backends in one image is not a lie about which it
            // has, but it is not a working image either: the runtime refuses to
            // choose ("more than one RMW backend is registered and no $NROS_RMW
            // selector was set"), so the coordinate still describes nothing that
            // runs. The fix is a facade carve-out — issue 0270's shape, because
            // cargo cannot subtract a default.
            let extra: Vec<&str> = BACKENDS
                .iter()
                .map(|(n, _)| *n)
                .filter(|n| *n != declared && counts[n] > 0)
                .collect();
            if !extra.is_empty() && !declares_a_bridge(&nros_tests::project_root().join(&row.dir)) {
                wrong.push(format!(
                    "{}: row declares rmw `{declared}`, but {} ALSO links {} — \
                     the runtime refuses to pick between registered backends",
                    row.label(),
                    bin.display(),
                    extra.join(" and ")
                ));
            }
        }
    }

    if checked == 0 {
        nros_tests::skip!(
            "no workspace artifact with readable backend symbols was built for \
             this lane ({unreadable} unreadable) — build fixtures first"
        );
    }

    assert!(
        wrong.is_empty(),
        "{} artifact(s) do not link the RMW their fixture row claims \
         (checked {checked}):\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}
