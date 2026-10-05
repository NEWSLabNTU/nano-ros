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
//!
//! ## Symbols are not behaviour (issue 1310)
//!
//! Issue 0831's prescription — "add a runtime assertion rather than trusting
//! the coordinate — the artifact knows" — was read as *inspect the artifact*,
//! and `nm` proves only that a backend was LINKED. Issue 1295 then hit these
//! same two rows: `workspace-rust-native-cyclonedds` linked Cyclone (this gate
//! green, 350+ `dds_` symbols) and could not create a publisher, because the
//! `nros` umbrella never got the `rmw-cyclonedds` MARKER that forwards
//! `needs-type-descriptors`. Two bugs, one gate, the same two rows, and the
//! second was invisible to it.
//!
//! So the second test below RUNS the row's binary. The split is deliberate:
//! `nm` answers "was it linked" for every row cheaply, and the run answers
//! "does it work" for the rows this host can start.

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

/// Issue 1690 — map `f` over `items` on a bounded pool of threads, results in
/// input order.
///
/// Both tests here visit EVERY `workspace_fixture` row, and they did it one row
/// after another, so their wall time was the SUM over rows: the runtime test was
/// killed at nextest's 60 s terminate with no message (solo too, so not load),
/// and the `nm` test sat at 54.8 s. A sum grows with every row anybody adds; a
/// pool's wall time is bounded by the slowest row times the row count over the
/// pool width. Rows are independent by construction — each run gets its own
/// `ROS_DOMAIN_ID` and its own process group — so nothing is shared to order.
///
/// Width is the host's parallelism capped at [`POOL_CAP`]: the rows open RMW
/// sessions, and a 64-way box gains nothing from 64 concurrent discoveries.
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    let width = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, POOL_CAP)
        .min(items.len().max(1));
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|s| {
        for _ in 0..width {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    let r = f(item);
                    out.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(r);
                }
            });
        }
    });
    out.into_inner()
        .unwrap_or_else(|e| e.into_inner())
        .into_iter()
        .map(|r| r.expect("every index was visited"))
        .collect()
}

/// [`par_map`]'s width ceiling.
const POOL_CAP: usize = 8;

/// [`par_map`]'s negative control, on the normal path: results come back in
/// INPUT order, every item is visited exactly once, and the items run
/// concurrently. The last is the property issue 1690 needed — a serial map of
/// these eight 300 ms items takes 2.4 s, which this refuses.
#[test]
fn the_row_pool_runs_rows_concurrently_and_keeps_their_order() {
    let items: Vec<usize> = (0..8).collect();
    let t0 = std::time::Instant::now();
    let out = par_map(&items, |i| {
        std::thread::sleep(std::time::Duration::from_millis(300));
        i * 10
    });
    let took = t0.elapsed();
    assert_eq!(out, (0..8).map(|i| i * 10).collect::<Vec<_>>());
    let width = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(POOL_CAP);
    if width >= 2 {
        assert!(
            took < std::time::Duration::from_millis(300 * 8 - 300),
            "8 x 300 ms on a pool of {width} took {took:?} -- the rows ran serially"
        );
    }
    assert!(par_map(&[] as &[usize], |i| *i).is_empty());
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

/// The row's built binary, if [`row_binary`] can locate it.
///
/// issue 1620 — also counts the rows this run's lane PROMISED
/// (coordinate-scoped, in lane, no light-tier opt-out) into `promised`, so an
/// audit that located none of them FAILS instead of skipping. Deliberately not
/// a per-row failure: `row_binary` is a locator over `artifact_root`, and for
/// rows whose images land elsewhere (the Zephyr workspace rows share
/// `examples/workspaces/<ws>/target` with their host siblings while the image
/// lands in the Zephyr build root) "not located" is not evidence of "not
/// built". Zero located out of N promised is — either the lane did not build
/// what it promised, or the locator has rotted, and both are findings.
fn built_row_binary(
    row: &nros_tests::fixtures::lane::Row,
    promised: &mut usize,
) -> Option<PathBuf> {
    if nros_tests::fixtures::lane::absent_row_breaks_promise(row) {
        *promised += 1;
    }
    let root = nros_tests::project_root().join(&row.artifact_root);
    root.is_dir().then(|| row_binary(&row.id, &root)).flatten()
}

/// The audit located nothing. In a gated run that promised rows, that is a
/// broken promise and FAILS (issue 0584 part 2 — no `[SKIPPED]` marker, so no
/// rewrite can count it as a skip). Otherwise nothing was promised, and a skip
/// is the honest verdict.
fn none_located(promised: usize, why: &str) -> ! {
    assert!(
        promised == 0,
        "Test fixture binary MISSING for an in-lane coordinate — this run's lane \
         selected {promised} workspace row(s) and the audit located the built \
         artifact of none of them ({why}). A gated run already asserted the \
         lane's fixtures are built, so this is a broken promise, not an \
         environment skip (issue 0584). Build them (`just build-test-fixtures \
         lane=<this lane>`), or, if they ARE built, `row_binary` no longer \
         models where they land."
    );
    nros_tests::skip!("no generated entry could be checked here — {why}");
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
    let mut promised = 0usize;

    // Locate serially (cheap, and it counts `promised`), then run `nm` over the
    // located binaries on a pool (issue 1690): `nm` on an 8 MB binary is the
    // cost, and summed over every row it reached 54.8 s of a 60 s budget.
    let mut located: Vec<(&nros_tests::fixtures::lane::Row, PathBuf)> = Vec::new();
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
        // Not located: `built_row_binary` says why that alone fails nothing,
        // and `none_located` when it does.
        if let Some(bin) = built_row_binary(row, &mut promised) {
            located.push((row, bin));
        }
    }
    let symbols = par_map(&located, |(_, bin)| backend_symbols(bin));

    for ((row, bin), counts) in located.iter().zip(symbols) {
        let declared = row.coord.2.as_str();
        {
            let Some(counts) = counts else {
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

    if checked == 0 && unreadable == 0 {
        none_located(promised, "no workspace artifact was located");
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

/// How long an entry gets to reach its own exit.
///
/// A generated hosted entry spins forever by default (issue 1439), so
/// [`run_entry`] hands it a 1 ms `NROS_ENTRY_SPIN_MS` budget: it registers its
/// nodes, spins once, and returns. Generous because a Cyclone entry does discovery first, and a
/// loaded CI box is slow — but bounded, because a HANG is a finding too and
/// must not become a hung suite.
///
/// Issue 1690 — 20 s, measured: solo on this host the slowest of 73 rows took
/// 3.8 s (`workspace-cpp-native-xrce`, failing its Agent lookup), so 20 s is
/// five times the worst legitimate row. It was 45 s, which a single hung row
/// spent out of nextest's 60 s terminate on its own; with the rows on
/// [`par_map`] the worst case is now one budget plus the rest of the pool.
const RUN_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

/// Run one entry to completion, or kill it at [`RUN_BUDGET`].
///
/// Returns `(exit status if it exited on its own, combined output)`.
fn run_entry(bin: &Path) -> (Option<std::process::ExitStatus>, String) {
    let mut cmd = Command::new(bin);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    // The DDS bus is pinned to loopback by a profile FILE, never by a variable
    // a test exports (issue 1009), and OUR half needs it as much as a `ros2`
    // peer does (issue 1137) — without it this test would discover whatever
    // else is on the LAN and report its findings as ours.
    nros_tests::dds_isolation::apply_to_command(&mut cmd);
    // Each row on its own domain, so two rows running at once cannot see each
    // other and read a neighbour's traffic as their own.
    cmd.env(
        "ROS_DOMAIN_ID",
        nros_tests::unique_ros_domain_id().to_string(),
    );
    // Register, spin one bounded tick, exit — the question here is whether
    // registration succeeds, not whether the node keeps running.
    cmd.env("NROS_ENTRY_SPIN_MS", "1");
    nros_tests::process::set_new_process_group(&mut cmd);

    let Ok(mut child) = cmd.spawn() else {
        return (None, String::new());
    };
    let deadline = std::time::Instant::now() + RUN_BUDGET;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) if std::time::Instant::now() >= deadline => {
                nros_tests::process::kill_process_group(&mut child);
                break None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(_) => break None,
        }
    };
    let out = child.wait_with_output().ok();
    let text = out
        .map(|o| {
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            )
        })
        .unwrap_or_default();
    (status, text)
}

/// Issue 1310 — a row's entry must actually REGISTER ITS NODES, not merely
/// link the backend its coordinate names.
///
/// The assertion is deliberately narrow, and the narrowing is what makes it
/// runnable with no peer: an entry that reaches
/// [`ENTRY_NODE_REGISTER_ERROR`](nros_tests::output::ENTRY_NODE_REGISTER_ERROR)
/// FAILS, because that is a defect in the image regardless of what else is on
/// the bus. An entry that never got that far — no router, no XRCE Agent — is a
/// PRECONDITION this host does not meet and is reported as such, per row, so a
/// skip can never read as coverage.
#[test]
fn a_rows_entry_registers_its_nodes_at_runtime() {
    let mut ran = 0usize;
    let mut no_peer: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    let mut promised = 0usize;
    let mut targets: Vec<(&nros_tests::fixtures::lane::Row, PathBuf)> = Vec::new();
    for row in nros_tests::fixtures::lane::manifest_rows() {
        if row.kind != "workspace_fixture" {
            continue;
        }
        if let Some(bin) = built_row_binary(row, &mut promised) {
            targets.push((row, bin));
        }
    }
    let located = targets.len();

    // Issue 1690 — each row on the pool, each bounded by RUN_BUDGET, each
    // timed. Run one after another, the rows' SUM outlived nextest's 60 s
    // terminate, so a slow or hung row killed the whole test with no message
    // naming it. Now the slowest row bounds the wall time and a hang is a
    // per-row verdict below.
    let started = std::time::Instant::now();
    let runs = par_map(&targets, |(_, bin)| {
        let t0 = std::time::Instant::now();
        let (status, text) = run_entry(bin);
        (status, text, t0.elapsed())
    });
    let wall = started.elapsed();
    let mut slowest: Vec<(std::time::Duration, &str)> = targets
        .iter()
        .zip(&runs)
        .map(|((row, _), (_, _, took))| (*took, row.id.as_str()))
        .collect();
    slowest.sort_by_key(|a| std::cmp::Reverse(a.0));
    eprintln!(
        "rmw-coordinate-truth: ran {located} entr{} in {wall:.1?} on a pool of {}; slowest: {}",
        if located == 1 { "y" } else { "ies" },
        POOL_CAP.min(
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        ),
        slowest
            .iter()
            .take(5)
            .map(|(d, id)| format!("{id} {d:.1?}"))
            .collect::<Vec<_>>()
            .join(", ")
    );

    for ((row, bin), (status, text, took)) in targets.iter().zip(runs) {
        let registered_ok = status.map(|s| s.success()).unwrap_or(false)
            && text.contains(nros_tests::output::ENTRY_COMPLETE_MARKER);
        if registered_ok {
            ran += 1;
            continue;
        }
        // The SESSION is what separates a defect from an absent peer, and
        // `NodeRegister` alone cannot: an xrce entry with no Agent fails
        // session open and then reports the SAME `NodeRegister` line the real
        // defect ends with (it proceeds on a NullNodeRuntime). Keying on the
        // consequence would have accused xrce of issue 1295's bug on every host
        // without an Agent — measured while writing this test.
        let session_up = text.contains(nros_tests::output::SESSION_OPEN_MARKER)
            && !text.contains(nros_tests::output::SESSION_OPEN_FAILED_MARKER);
        if session_up && text.contains(nros_tests::output::ENTRY_NODE_REGISTER_ERROR) {
            failed.push(format!(
                "  {} ({}) — {}\n      {}",
                row.id,
                row.coord.2,
                bin.display(),
                text.lines()
                    .filter(|l| l.contains(nros_tests::output::ENTRY_ERROR_MARKER)
                        || l.contains(nros_tests::output::ENTRY_NODE_REGISTER_ERROR))
                    .collect::<Vec<_>>()
                    .join("\n      ")
            ));
            continue;
        }
        // The session never came up (no router, no Agent), or the entry
        // stopped somewhere else entirely: this host owes it a peer. Reported
        // per row with the reason, never counted as coverage.
        let why = if status.is_none() {
            // Issue 1690 — a hang names itself: the row, the binary and the
            // budget it outlived, instead of a suite-level TIMEOUT.
            format!(
                "HUNG: no exit within the {RUN_BUDGET:?} budget (killed after {took:.1?}): {}",
                bin.display()
            )
        } else if text.contains(nros_tests::output::SESSION_OPEN_FAILED_MARKER) {
            "the backend refused the session (this host runs no peer for it)".to_string()
        } else {
            "did not reach node registration".to_string()
        };
        no_peer.push(format!("  {} ({}) — {why}", row.id, row.coord.2));
    }

    if located == 0 {
        none_located(promised, "no workspace_fixture row's artifact was located");
    }
    assert!(
        failed.is_empty(),
        "{} generated entr{} opened a session and could NOT register {} nodes \
         (issue 1295's shape — the coordinate and the symbols are both green here):\n{}",
        failed.len(),
        if failed.len() == 1 { "y" } else { "ies" },
        if failed.len() == 1 { "its" } else { "their" },
        failed.join("\n"),
    );

    if ran == 0 {
        let why = if no_peer.is_empty() {
            "no workspace_fixture row has a built artifact in this lane".to_string()
        } else {
            format!(
                "every row needs a peer this host lacks:\n{}",
                no_peer.join("\n")
            )
        };
        nros_tests::skip!("no generated entry could be RUN here — {why}");
    }
    if !no_peer.is_empty() {
        eprintln!(
            "rmw-coordinate-truth: ran {ran} entr{}; {} reported a missing peer:\n{}",
            if ran == 1 { "y" } else { "ies" },
            no_peer.len(),
            no_peer.join("\n"),
        );
    }
}
