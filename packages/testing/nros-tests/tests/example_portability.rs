//! phase-338 W1 — the example **source portability** gate.
//!
//! nano-ros claims that the source a user writes is the same on every
//! supported target. This asserts it instead of hoping.
//!
//! For each `(language, program)` pair, every platform copy is normalized
//! (comments and blank lines stripped, whitespace collapsed — prose is not
//! under test) and the copies **within a portability group** must be
//! byte-identical.
//!
//! ## Groups, not one global set
//!
//! Measured 2026-08-04: three groups exist, and two of them are legitimate
//! execution-model differences rather than defects.
//!
//! * **A — scheduled.** An RTOS or host OS runs the callbacks.
//! * **B — bare-metal.** No scheduler, so the node uses
//!   `DispatchStrategy::Deferred` plus an explicit `tick()`, and the
//!   `nros_log` facade instead of `log`.
//! * **C — Zephyr.** Zephyr's component authoring shape (`Talker.c` /
//!   `Talker.hpp`), which is the convention Zephyr users expect.
//!
//! A group with one member trivially passes; it is still declared so the
//! reason is recorded and so a second member cannot be added silently.
//!
//! ## The ratchet
//!
//! [`KNOWN_DIVERGENCE`] lists the copies that do **not** match their group
//! today. Every entry names the wave that removes it. Two tests keep it
//! honest:
//!
//! * [`copies_within_a_group_are_identical`] ignores listed divergences, so
//!   the gate is green at baseline and stays green while phase-338 lands.
//! * [`no_stale_divergence_entries`] fails when a listed copy has *become*
//!   identical — so a fix cannot land without deleting its entry. The list
//!   only ever shrinks, and its length is the phase's progress metric.
//!
//! Deleting the last entry for a `(lang, program, group)` is what "this
//! program is portable" means, mechanically.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

// ---------------------------------------------------------------------------
// Declared groups
// ---------------------------------------------------------------------------

struct Group {
    name: &'static str,
    /// Why these platforms can share one body, and why they cannot share with
    /// the other groups. Recorded so nobody has to re-derive it.
    reason: &'static str,
    platforms: &'static [&'static str],
}

const GROUPS: &[Group] = &[
    Group {
        name: "A-scheduled",
        reason: "an RTOS or host OS scheduler runs the callbacks; the node body \
                 is plain declarative + callback code",
        platforms: &[
            "native",
            "threadx-linux",
            "qemu-arm-freertos",
            "qemu-arm-nuttx",
            "qemu-riscv-nuttx",
            "qemu-riscv64-threadx",
        ],
    },
    Group {
        name: "B-baremetal",
        reason: "no scheduler: the node declares DispatchStrategy::Deferred and an \
                 explicit tick(), and uses the nros_log facade because `log` needs std",
        platforms: &["qemu-arm-baremetal", "qemu-esp32-baremetal", "stm32f4"],
    },
    Group {
        name: "C-zephyr",
        reason: "Zephyr component authoring shape (Talker.c / Talker.hpp) — the \
                 convention Zephyr users expect, not a portability failure",
        platforms: &["zephyr"],
    },
    Group {
        name: "D-px4",
        reason: "PX4 firmware module shape (uORB middleware, no CDR) — RFC-0011; \
                 phase-325 owns this axis",
        platforms: &["px4"],
    },
];

/// A copy that does not yet match its group.
struct Divergence {
    lang: &'static str,
    program: &'static str,
    platform: &'static str,
    /// The wave that removes it, then why it differs. An entry with no wave is
    /// a permanent exception and must say so.
    reason: &'static str,
}

/// Copies that do not match their group **today**.
///
/// Baseline recorded 2026-08-04, before any phase-338 fix. Shrinking this list
/// is the phase's progress metric; [`no_stale_divergence_entries`] makes the
/// shrinking mandatory rather than optional.
const KNOWN_DIVERGENCE: &[Divergence] = &[
    // Baseline recorded 2026-08-04 by walking the tree; every entry names the
    // wave that removes it. Grouped by cause, alphabetical within.
    Divergence {
        lang: "c",
        program: "action-client",
        platform: "qemu-arm-nuttx",
        reason: "W2 — NuttX carries a 3-attempt retry loop around send_goal / service \
                 call that the other platforms lack. A robustness accommodation, not a \
                 platform constraint: unify by giving every copy the retry.",
    },
    Divergence {
        lang: "c",
        program: "listener",
        platform: "native",
        reason: "PERMANENT — native carries the NROS_SUB_TYPE env switch so tests can \
                 select int32 vs string. A declared affordance, not drift; delete this \
                 entry if the test that needs it goes away.",
    },
    Divergence {
        lang: "cpp",
        program: "action-client",
        platform: "qemu-arm-nuttx",
        reason: "W2 — NuttX carries a 3-attempt retry loop around send_goal / service \
                 call that the other platforms lack. A robustness accommodation, not a \
                 platform constraint: unify by giving every copy the retry.",
    },
    Divergence {
        lang: "cpp",
        program: "service-client",
        platform: "qemu-arm-nuttx",
        reason: "W2 — NuttX carries a 3-attempt retry loop around send_goal / service \
                 call that the other platforms lack. A robustness accommodation, not a \
                 platform constraint: unify by giving every copy the retry.",
    },
    Divergence {
        lang: "rust",
        program: "action-client",
        platform: "native",
        reason: "W3 — hosted `main.rs` (register_linked_rmw + env_logger + banner + \
                 init/executor-open) against the group's `lib.rs`; all of it is \
                 ceremony the generated entry already owns on embedded.",
    },
    Divergence {
        lang: "rust",
        program: "action-client",
        platform: "qemu-riscv64-threadx",
        reason: "W2 — link ceremony: `extern crate alloc`, `extern crate \
                 nros_board_threadx_qemu_riscv64 as _`, \
                 `cyclonedds_app_main!(register)`, plus stray \
                 `#![no_std]`/`#![no_main]`.",
    },
    Divergence {
        lang: "rust",
        program: "action-client",
        platform: "threadx-linux",
        reason: "W2.c — node struct and NAME drifted (`ActionClient` / \"action_client\" \
                 against the group's `FibonacciClient`); pure naming, no behaviour.",
    },
    Divergence {
        lang: "rust",
        program: "action-client-entry",
        platform: "qemu-arm-nuttx",
        reason: "W2.c/W2.d — the re-exported node-package name differs \
                 (`<platform>_rs_<program>`) and the `#![no_std]`/`#![no_main]` \
                 attributes are inconsistent for the same generated entry.",
    },
    Divergence {
        lang: "rust",
        program: "action-client-entry",
        platform: "threadx-linux",
        reason: "W2.c/W2.d — the re-exported node-package name differs \
                 (`<platform>_rs_<program>`) and the `#![no_std]`/`#![no_main]` \
                 attributes are inconsistent for the same generated entry.",
    },
    Divergence {
        lang: "rust",
        program: "action-client-rtic",
        platform: "stm32f4",
        reason: "phase-337 W7.a — stm32f4 leaves the matrix (0 Runtime cells); its RTIC \
                 variants diverge broadly from the qemu-arm-baremetal siblings. Resolve \
                 by removal, not by converging a board we are dropping.",
    },
    Divergence {
        lang: "rust",
        program: "action-server",
        platform: "native",
        reason: "W3 — hosted `main.rs` (register_linked_rmw + env_logger + banner + \
                 init/executor-open) against the group's `lib.rs`; all of it is \
                 ceremony the generated entry already owns on embedded.",
    },
    Divergence {
        lang: "rust",
        program: "action-server",
        platform: "qemu-riscv64-threadx",
        reason: "W2 — link ceremony: `extern crate alloc`, `extern crate \
                 nros_board_threadx_qemu_riscv64 as _`, \
                 `cyclonedds_app_main!(register)`, plus stray \
                 `#![no_std]`/`#![no_main]`.",
    },
    Divergence {
        lang: "rust",
        program: "action-server",
        platform: "threadx-linux",
        reason: "W2.c — node struct and NAME drifted (`ActionClient` / \"action_client\" \
                 against the group's `FibonacciClient`); pure naming, no behaviour.",
    },
    Divergence {
        lang: "rust",
        program: "action-server-entry",
        platform: "qemu-arm-nuttx",
        reason: "W2.c/W2.d — the re-exported node-package name differs \
                 (`<platform>_rs_<program>`) and the `#![no_std]`/`#![no_main]` \
                 attributes are inconsistent for the same generated entry.",
    },
    Divergence {
        lang: "rust",
        program: "action-server-entry",
        platform: "threadx-linux",
        reason: "W2.c/W2.d — the re-exported node-package name differs \
                 (`<platform>_rs_<program>`) and the `#![no_std]`/`#![no_main]` \
                 attributes are inconsistent for the same generated entry.",
    },
    Divergence {
        lang: "rust",
        program: "action-server-rtic",
        platform: "stm32f4",
        reason: "phase-337 W7.a — stm32f4 leaves the matrix (0 Runtime cells); its RTIC \
                 variants diverge broadly from the qemu-arm-baremetal siblings. Resolve \
                 by removal, not by converging a board we are dropping.",
    },
    Divergence {
        lang: "rust",
        program: "listener",
        platform: "native",
        reason: "W3 — hosted `main.rs` (register_linked_rmw + env_logger + banner + \
                 init/executor-open) against the group's `lib.rs`; all of it is \
                 ceremony the generated entry already owns on embedded.",
    },
    Divergence {
        lang: "rust",
        program: "listener",
        platform: "qemu-riscv64-threadx",
        reason: "W2 — link ceremony: `extern crate alloc`, `extern crate \
                 nros_board_threadx_qemu_riscv64 as _`, \
                 `cyclonedds_app_main!(register)`, plus stray \
                 `#![no_std]`/`#![no_main]`.",
    },
    Divergence {
        lang: "rust",
        program: "listener",
        platform: "qemu-esp32-baremetal",
        reason: "W3.c — group B is not yet internally consistent; measure the \
                 irreducible part after the log/nros_log facade is unified and \
                 DISPATCH/tick get defaults.",
    },
    Divergence {
        lang: "rust",
        program: "listener-entry",
        platform: "qemu-arm-nuttx",
        reason: "W2.c/W2.d — the re-exported node-package name differs \
                 (`<platform>_rs_<program>`) and the `#![no_std]`/`#![no_main]` \
                 attributes are inconsistent for the same generated entry.",
    },
    Divergence {
        lang: "rust",
        program: "listener-entry",
        platform: "threadx-linux",
        reason: "W2.c/W2.d — the re-exported node-package name differs \
                 (`<platform>_rs_<program>`) and the `#![no_std]`/`#![no_main]` \
                 attributes are inconsistent for the same generated entry.",
    },
    Divergence {
        lang: "rust",
        program: "listener-rtic",
        platform: "stm32f4",
        reason: "phase-337 W7.a — stm32f4 leaves the matrix (0 Runtime cells); its RTIC \
                 variants diverge broadly from the qemu-arm-baremetal siblings. Resolve \
                 by removal, not by converging a board we are dropping.",
    },
    Divergence {
        lang: "rust",
        program: "service-client",
        platform: "native",
        reason: "W3 — hosted `main.rs` (register_linked_rmw + env_logger + banner + \
                 init/executor-open) against the group's `lib.rs`; all of it is \
                 ceremony the generated entry already owns on embedded.",
    },
    Divergence {
        lang: "rust",
        program: "service-client",
        platform: "qemu-riscv64-threadx",
        reason: "W2 — link ceremony: `extern crate alloc`, `extern crate \
                 nros_board_threadx_qemu_riscv64 as _`, \
                 `cyclonedds_app_main!(register)`, plus stray \
                 `#![no_std]`/`#![no_main]`.",
    },
    Divergence {
        lang: "rust",
        program: "service-client",
        platform: "threadx-linux",
        reason: "W2.c — node struct and NAME drifted (`ActionClient` / \"action_client\" \
                 against the group's `FibonacciClient`); pure naming, no behaviour.",
    },
    Divergence {
        lang: "rust",
        program: "service-client-entry",
        platform: "qemu-arm-nuttx",
        reason: "W2.c/W2.d — the re-exported node-package name differs \
                 (`<platform>_rs_<program>`) and the `#![no_std]`/`#![no_main]` \
                 attributes are inconsistent for the same generated entry.",
    },
    Divergence {
        lang: "rust",
        program: "service-client-entry",
        platform: "threadx-linux",
        reason: "W2.c/W2.d — the re-exported node-package name differs \
                 (`<platform>_rs_<program>`) and the `#![no_std]`/`#![no_main]` \
                 attributes are inconsistent for the same generated entry.",
    },
    Divergence {
        lang: "rust",
        program: "service-client-rtic",
        platform: "stm32f4",
        reason: "phase-337 W7.a — stm32f4 leaves the matrix (0 Runtime cells); its RTIC \
                 variants diverge broadly from the qemu-arm-baremetal siblings. Resolve \
                 by removal, not by converging a board we are dropping.",
    },
    Divergence {
        lang: "rust",
        program: "service-server",
        platform: "native",
        reason: "W3 — hosted `main.rs` (register_linked_rmw + env_logger + banner + \
                 init/executor-open) against the group's `lib.rs`; all of it is \
                 ceremony the generated entry already owns on embedded.",
    },
    Divergence {
        lang: "rust",
        program: "service-server",
        platform: "qemu-riscv64-threadx",
        reason: "W2 — link ceremony: `extern crate alloc`, `extern crate \
                 nros_board_threadx_qemu_riscv64 as _`, \
                 `cyclonedds_app_main!(register)`, plus stray \
                 `#![no_std]`/`#![no_main]`.",
    },
    Divergence {
        lang: "rust",
        program: "service-server",
        platform: "threadx-linux",
        reason: "W2.c — node struct and NAME drifted (`ActionClient` / \"action_client\" \
                 against the group's `FibonacciClient`); pure naming, no behaviour.",
    },
    Divergence {
        lang: "rust",
        program: "service-server-entry",
        platform: "qemu-arm-nuttx",
        reason: "W2.c/W2.d — the re-exported node-package name differs \
                 (`<platform>_rs_<program>`) and the `#![no_std]`/`#![no_main]` \
                 attributes are inconsistent for the same generated entry.",
    },
    Divergence {
        lang: "rust",
        program: "service-server-entry",
        platform: "threadx-linux",
        reason: "W2.c/W2.d — the re-exported node-package name differs \
                 (`<platform>_rs_<program>`) and the `#![no_std]`/`#![no_main]` \
                 attributes are inconsistent for the same generated entry.",
    },
    Divergence {
        lang: "rust",
        program: "service-server-rtic",
        platform: "stm32f4",
        reason: "phase-337 W7.a — stm32f4 leaves the matrix (0 Runtime cells); its RTIC \
                 variants diverge broadly from the qemu-arm-baremetal siblings. Resolve \
                 by removal, not by converging a board we are dropping.",
    },
    Divergence {
        lang: "rust",
        program: "talker",
        platform: "native",
        reason: "W3 — hosted `main.rs` (register_linked_rmw + env_logger + banner + \
                 init/executor-open) against the group's `lib.rs`; all of it is \
                 ceremony the generated entry already owns on embedded.",
    },
    Divergence {
        lang: "rust",
        program: "talker",
        platform: "qemu-riscv64-threadx",
        reason: "W2 — link ceremony: `extern crate alloc`, `extern crate \
                 nros_board_threadx_qemu_riscv64 as _`, \
                 `cyclonedds_app_main!(register)`, plus stray \
                 `#![no_std]`/`#![no_main]`.",
    },
    Divergence {
        lang: "rust",
        program: "talker",
        platform: "qemu-esp32-baremetal",
        reason: "W3.c — group B is not yet internally consistent; measure the \
                 irreducible part after the log/nros_log facade is unified and \
                 DISPATCH/tick get defaults.",
    },
    Divergence {
        lang: "rust",
        program: "talker",
        platform: "stm32f4",
        reason: "W3.c — group B is not yet internally consistent; measure the \
                 irreducible part after the log/nros_log facade is unified and \
                 DISPATCH/tick get defaults.",
    },
    Divergence {
        lang: "rust",
        program: "talker-entry",
        platform: "qemu-arm-nuttx",
        reason: "W2.c/W2.d — the re-exported node-package name differs \
                 (`<platform>_rs_<program>`) and the `#![no_std]`/`#![no_main]` \
                 attributes are inconsistent for the same generated entry.",
    },
    Divergence {
        lang: "rust",
        program: "talker-entry",
        platform: "threadx-linux",
        reason: "W2.c/W2.d — the re-exported node-package name differs \
                 (`<platform>_rs_<program>`) and the `#![no_std]`/`#![no_main]` \
                 attributes are inconsistent for the same generated entry.",
    },
    Divergence {
        lang: "rust",
        program: "talker-rtic",
        platform: "stm32f4",
        reason: "phase-337 W7.a — stm32f4 leaves the matrix (0 Runtime cells); its RTIC \
                 variants diverge broadly from the qemu-arm-baremetal siblings. Resolve \
                 by removal, not by converging a board we are dropping.",
    },
];

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

const LANGS: &[&str] = &["rust", "c", "cpp"];

fn examples_dir() -> PathBuf {
    nros_tests::project_root().join("examples")
}

/// `examples/<platform>/<lang>/<program>/src/**` — a bounded walk that never
/// descends into a program's `build*/` or `target*/` output.
fn collect_sources() -> BTreeMap<(String, String, String), String> {
    let mut out = BTreeMap::new();
    let root = examples_dir();
    let Ok(platforms) = fs::read_dir(&root) else {
        return out;
    };
    for platform in platforms.flatten() {
        if !platform.path().is_dir() {
            continue;
        }
        let plat = platform.file_name().to_string_lossy().to_string();
        for lang in LANGS {
            let lang_dir = platform.path().join(lang);
            let Ok(programs) = fs::read_dir(&lang_dir) else {
                continue;
            };
            for program in programs.flatten() {
                let src = program.path().join("src");
                if !src.is_dir() {
                    continue;
                }
                let prog = program.file_name().to_string_lossy().to_string();
                if let Some(body) = normalized_body(&src) {
                    out.insert((lang.to_string(), prog, plat.clone()), body);
                }
            }
        }
    }
    out
}

/// The comparable body of one example: every source file under `src/`,
/// normalized and concatenated in filename order so two copies are compared
/// as a whole rather than file by file.
fn normalized_body(src: &Path) -> Option<String> {
    let mut files: Vec<(String, String)> = Vec::new();
    let mut stack = vec![src.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "rs" | "c" | "h" | "cpp" | "hpp" | "cc") {
                continue;
            }
            let rel = path
                .strip_prefix(src)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            let text = fs::read_to_string(&path).ok()?;
            files.push((rel, normalize(&text)));
        }
    }
    if files.is_empty() {
        return None;
    }
    files.sort();
    Some(
        files
            .into_iter()
            .map(|(name, body)| format!("--- {name}\n{body}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Strip what is not under test: block comments, whole-line comments (including
/// `///` and `//!` doc comments, which is where the platform names live), blank
/// lines, and indentation.
fn normalize(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    let mut stripped = String::new();
    // Remove /* ... */ blocks first so their inner lines cannot survive.
    while let Some(start) = rest.find("/*") {
        stripped.push_str(&rest[..start]);
        match rest[start..].find("*/") {
            Some(end) => rest = &rest[start + end + 2..],
            None => {
                rest = "";
                break;
            }
        }
    }
    stripped.push_str(rest);

    for line in stripped.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with('*') {
            continue;
        }
        out.push_str(&t.split_whitespace().collect::<Vec<_>>().join(" "));
        out.push('\n');
    }
    out
}

fn group_of(platform: &str) -> Option<&'static Group> {
    GROUPS.iter().find(|g| g.platforms.contains(&platform))
}

fn is_known(lang: &str, program: &str, platform: &str) -> bool {
    KNOWN_DIVERGENCE
        .iter()
        .any(|d| d.lang == lang && d.program == program && d.platform == platform)
}

/// Group the copies of one `(lang, program)` by group name, dropping platforms
/// that are not in any declared group.
fn by_group(
    sources: &BTreeMap<(String, String, String), String>,
) -> BTreeMap<(String, String, &'static str), Vec<(String, String)>> {
    let mut out: BTreeMap<_, Vec<(String, String)>> = BTreeMap::new();
    for ((lang, program, platform), body) in sources {
        let Some(group) = group_of(platform) else {
            continue;
        };
        out.entry((lang.clone(), program.clone(), group.name))
            .or_default()
            .push((platform.clone(), body.clone()));
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Every declared platform belongs to exactly one group, and every group names
/// platforms that exist. A group that names nothing is a typo, not a promise.
#[test]
fn groups_are_well_formed() {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for group in GROUPS {
        assert!(
            !group.reason.trim().is_empty(),
            "group {} carries no reason — an undocumented group is tribal memory",
            group.name
        );
        assert!(
            !group.platforms.is_empty(),
            "group {} names no platforms",
            group.name
        );
        for platform in group.platforms {
            assert!(
                seen.insert(platform),
                "platform {platform} appears in more than one group"
            );
        }
    }

    let root = examples_dir();
    let missing: Vec<&str> = seen
        .iter()
        .copied()
        .filter(|p| !root.join(p).is_dir())
        .collect();
    assert!(
        missing.is_empty(),
        "groups name platforms with no examples/ directory: {missing:?}"
    );
}

/// Every known divergence carries a reason, and names a real copy.
#[test]
fn divergence_entries_are_well_formed() {
    let sources = collect_sources();
    let mut problems = Vec::new();
    for d in KNOWN_DIVERGENCE {
        if d.reason.trim().is_empty() {
            problems.push(format!(
                "{}/{} on {}: no reason — every exception states why and which wave removes it",
                d.lang, d.program, d.platform
            ));
        }
        let key = (
            d.lang.to_string(),
            d.program.to_string(),
            d.platform.to_string(),
        );
        if !sources.contains_key(&key) {
            problems.push(format!(
                "{}/{} on {}: no such example — stale entry, delete it",
                d.lang, d.program, d.platform
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "malformed divergences:\n  {}",
        problems.join("\n  ")
    );
}

/// THE gate: within a portability group, every copy of a program is the same
/// source. Listed divergences are excluded and reported.
#[test]
fn copies_within_a_group_are_identical() {
    let sources = collect_sources();
    assert!(
        !sources.is_empty(),
        "found no example sources under examples/ — the walk is broken, \
         not the tree (a silent empty pass is how a gate stops gating)"
    );

    let mut failures = Vec::new();
    for ((lang, program, group), members) in by_group(&sources) {
        let comparable: Vec<&(String, String)> = members
            .iter()
            .filter(|(platform, _)| !is_known(&lang, &program, platform))
            .collect();
        if comparable.len() < 2 {
            continue;
        }
        let (ref_plat, ref_body) = comparable[0];
        for (platform, body) in &comparable[1..] {
            if body != ref_body {
                failures.push(format!(
                    "{lang}/{program} [{group}]: {platform} differs from {ref_plat}\n\
                     \x20   Either make them identical, or add a KNOWN_DIVERGENCE entry\n\
                     \x20   naming the wave that will — silence is not an option."
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "example copies diverge inside their portability group:\n\n{}",
        failures.join("\n\n")
    );
}

/// The ratchet: a copy listed as divergent that now matches its group must be
/// removed from the list. Without this, fixes land and the list rots into a
/// permanent apology.
#[test]
fn no_stale_divergence_entries() {
    let sources = collect_sources();
    let grouped = by_group(&sources);
    let mut stale = Vec::new();

    for d in KNOWN_DIVERGENCE {
        let Some(group) = group_of(d.platform) else {
            continue;
        };
        let key = (d.lang.to_string(), d.program.to_string(), group.name);
        let Some(members) = grouped.get(&key) else {
            continue;
        };
        let Some((_, this_body)) = members.iter().find(|(p, _)| p == d.platform) else {
            continue;
        };
        // Compare against a peer that is itself not listed as divergent.
        let peer = members
            .iter()
            .find(|(p, _)| p != d.platform && !is_known(d.lang, d.program, p));
        if let Some((peer_plat, peer_body)) = peer {
            if peer_body == this_body {
                stale.push(format!(
                    "{}/{} on {} now matches {} — delete its KNOWN_DIVERGENCE entry",
                    d.lang, d.program, d.platform, peer_plat
                ));
            }
        }
    }
    assert!(
        stale.is_empty(),
        "KNOWN_DIVERGENCE has entries that are no longer true:\n  {}\n\n\
         The list is the phase's progress metric; it only ever shrinks.",
        stale.join("\n  ")
    );
}

/// Progress report — always passes, prints the scoreboard with `--nocapture`.
#[test]
fn report_portability_baseline() {
    let sources = collect_sources();
    let grouped = by_group(&sources);
    let (mut portable, mut split) = (0usize, 0usize);
    for ((lang, program, group), members) in &grouped {
        if members.len() < 2 {
            continue;
        }
        let diverging = members
            .iter()
            .filter(|(p, _)| is_known(lang, program, p))
            .count();
        if diverging == 0 {
            portable += 1;
        } else {
            split += 1;
            println!("  SPLIT  {lang}/{program} [{group}] — {diverging} copy(ies) outstanding");
        }
    }
    println!(
        "\nportability: {portable} (lang,program,group) triples fully identical, \
         {split} still split, {} divergence entries outstanding",
        KNOWN_DIVERGENCE.len()
    );
}
