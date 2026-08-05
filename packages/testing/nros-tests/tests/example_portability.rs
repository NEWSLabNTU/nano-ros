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
        platform: "threadx-linux",
        reason: "W2.c — naming only, verified by diff 2026-08-05: `ActionClient` / \
                 \"action_client\" against the group's `FibonacciClient` / \
                 \"fibonacci_action_client\". A rename closes this one.",
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
        platform: "qemu-riscv64-threadx",
        reason: "W2 moved the ceremony this reason used to name (`extern crate \
                 alloc`, the board anchor, `cyclonedds_app_main!`, `#![no_main]`) \
                 into `src/app_main.rs`. Re-diffed 2026-08-05, what is left is ONE \
                 glue line — `mod app_main;` — which cannot move to `main.rs` \
                 because the CycloneDDS/CMake path links the STATICLIB, built from \
                 `lib.rs`'s module tree. Structural fix: let `nros::node!(Ty)`, \
                 which every copy already invokes, emit the glue cfg-gated on the \
                 deploy target. Plus body drift: a `u32` State vs `()`, a `GoalId` \
                 import, an extra \"Executing goal\" log, and a different \
                 goal-order spelling.",
    },
    Divergence {
        lang: "rust",
        program: "action-server",
        platform: "threadx-linux",
        reason: "W2.c — NOT naming only, verified by diff 2026-08-05. Beyond \
                 `ActionServer` / \"action_server\" vs `FibonacciServer`, this copy \
                 carries a `u32` State the group body does not, imports `GoalId`, \
                 logs an extra \"Executing goal\", spells the goal-order check \
                 differently (`matches!` vs `map`/`unwrap_or`), and is MISSING the \
                 group's `tick()`. Converging needs a decision about which body is \
                 canonical, not a rename.",
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
        platform: "qemu-riscv64-threadx",
        reason: "W2 moved the ceremony this reason used to name (`extern crate \
                 alloc`, the board anchor, `cyclonedds_app_main!`, `#![no_main]`) \
                 into `src/app_main.rs`. Re-diffed 2026-08-05, what is left is ONE \
                 glue line — `mod app_main;` — which cannot move to `main.rs` \
                 because the CycloneDDS/CMake path links the STATICLIB, built from \
                 `lib.rs`'s module tree. Structural fix: let `nros::node!(Ty)`, \
                 which every copy already invokes, emit the glue cfg-gated on the \
                 deploy target. Plus body drift: the failure arm logs at \
                 `log::error!` where the group logs `log::info!` (and drops the \
                 error value), plus a `reply`/`resp` binding rename.",
    },
    Divergence {
        lang: "rust",
        program: "service-client",
        platform: "threadx-linux",
        reason: "W2.c — NOT naming only, verified by diff 2026-08-05, and the \
                 widest of the four. The group body drives calls from a 1 s timer \
                 (`create_timer_for_callback_name(\"issue_call\", …)`) with a \
                 `pending` flag; this copy has neither timer nor flag and a smaller \
                 State. Two different state machines with the same output — pick one \
                 deliberately.",
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
        platform: "qemu-riscv64-threadx",
        reason: "W2 moved the ceremony this reason used to name (`extern crate \
                 alloc`, the board anchor, `cyclonedds_app_main!`, `#![no_main]`) \
                 into `src/app_main.rs`. Re-diffed 2026-08-05, what is left is ONE \
                 glue line — `mod app_main;` — which cannot move to `main.rs` \
                 because the CycloneDDS/CMake path links the STATICLIB, built from \
                 `lib.rs`'s module tree. Structural fix: let `nros::node!(Ty)`, \
                 which every copy already invokes, emit the glue cfg-gated on the \
                 deploy target. Plus body drift: the callback key is \"on_add\" \
                 vs the group's \"handle_add\", and State counts requests in a \
                 `u32` where the group types it `()`.",
    },
    Divergence {
        lang: "rust",
        program: "service-server",
        platform: "threadx-linux",
        reason: "W2.c — NOT naming only, verified by diff 2026-08-05. `ServiceServer` \
                 / \"service_server\" vs `AddTwoIntsServer` / \"add_two_ints_server\", \
                 AND the callback key differs (\"on_add\" vs the group's \"handle_add\") \
                 AND this copy counts handled requests in a `u32` State the group \
                 body types as `()`.",
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
        platform: "qemu-esp32-baremetal",
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
fn collect_sources() -> SourceMap {
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
    let files = classified_files(src)?;
    let logic: Vec<(String, String)> = files
        .into_iter()
        .filter(|(name, _, kind)| {
            let _ = name;
            *kind == FileKind::Logic
        })
        .map(|(name, body, _)| (name, body))
        .collect();
    if logic.is_empty() {
        return None;
    }
    Some(
        logic
            .into_iter()
            .map(|(name, body)| format!("--- {name}\n{body}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// What a source file is for.
///
/// **The Option B rule (phase-338, maintainer decision 2026-08-04): node logic
/// and platform boot glue live in separate FILES.** Portability is a property
/// of the logic; the glue is platform-specific by nature, and the goal is to
/// isolate it rather than pretend it can vanish — a staticlib target really
/// does need an `app_main` symbol that a hosted binary does not.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum FileKind {
    /// The node the user writes. Must be identical within a portability group.
    Logic,
    /// Boot/entry glue: `#![no_main]`, the panic handler, `nros::main!()`, a
    /// board `*_app_main!`. Platform-specific and NOT compared across
    /// platforms — but it must contain the ceremony, and the logic must not.
    Glue,
}

/// Rust separates the two by filename: `main.rs` is the glue file **when a
/// `lib.rs` exists beside it**, which is the split shape Option B standardizes.
///
/// The `has_lib` condition is what keeps the rule honest for a package that has
/// not been split yet — native's `talker/src/main.rs` is 91 lines of logic *and*
/// glue fused, and calling that "glue" would silently drop the only file it has
/// from the comparison. Un-split packages are therefore compared whole, and
/// their divergence shows up as the W3 work it is.
///
/// C and C++ have no separate glue file — `main.c` *is* the program the user
/// writes — so every file is logic there.
fn classify(lang_ext: &str, rel: &str, has_lib: bool) -> FileKind {
    match lang_ext {
        "rs" if rel == "main.rs" && has_lib => FileKind::Glue,
        // A named glue MODULE, for glue that cannot live in the `main.rs` bin
        // target: the ThreadX RV64 CycloneDDS path links the *staticlib*, so its
        // `#[no_mangle] app_main` must be reachable from `lib.rs`'s module tree.
        "rs" if GLUE_MODULES.contains(&rel.trim_end_matches(".rs")) => FileKind::Glue,
        _ => FileKind::Logic,
    }
}

/// Modules of the lib crate that hold platform boot glue rather than node logic.
/// Their `mod <name>;` declaration in the crate root is itself glue and is
/// stripped by [`normalize`], so declaring one does not make the logic file
/// diverge from platforms that need no such module.
const GLUE_MODULES: &[&str] = &["app_main"];

/// Every source file under `src/`, normalized and tagged, in filename order.
fn classified_files(src: &Path) -> Option<Vec<(String, String, FileKind)>> {
    let has_lib = src.join("lib.rs").is_file();
    let mut files: Vec<(String, String, FileKind)> = Vec::new();
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
            let kind = classify(ext, &rel, has_lib);
            files.push((rel, normalize(&text), kind));
        }
    }
    if files.is_empty() {
        return None;
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Some(files)
}

/// Boot ceremony that must live in a glue file, never in node logic.
///
/// These are the spellings the phase-338 audit found leaking into `lib.rs`:
/// the zenoh/xrce force-link anchors, a board's `*_app_main!`, and Zephyr's
/// `component_main!`. Each exists because rustc's staticlib DCE drops a
/// dependency's `#[no_mangle]` export without a direct reference — a real
/// constraint, and precisely why it belongs in a file that says so.
const CEREMONY_MARKERS: &[&str] = &[
    "force_link_backend!",
    "_app_main!",
    "component_main!",
    "#![no_main]",
];

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
        // A glue-module declaration is glue, not logic (see GLUE_MODULES).
        if GLUE_MODULES
            .iter()
            .any(|m| t == format!("mod {m};") || t == format!("pub mod {m};"))
        {
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

/// `(lang, program, platform)` -> normalized body.
type SourceMap = BTreeMap<(String, String, String), String>;
/// `(lang, program, group)` -> the `(platform, body)` copies in that group.
type GroupedCopies = BTreeMap<(String, String, &'static str), Vec<(String, String)>>;

/// Group the copies of one `(lang, program)` by group name, dropping platforms
/// that are not in any declared group.
fn by_group(sources: &SourceMap) -> GroupedCopies {
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
        if let Some((peer_plat, peer_body)) = peer
            && peer_body == this_body
        {
            stale.push(format!(
                "{}/{} on {} now matches {} — delete its KNOWN_DIVERGENCE entry",
                d.lang, d.program, d.platform, peer_plat
            ));
        }
    }
    assert!(
        stale.is_empty(),
        "KNOWN_DIVERGENCE has entries that are no longer true:\n  {}\n\n\
         The list is the phase's progress metric; it only ever shrinks.",
        stale.join("\n  ")
    );
}

/// The Option B invariant: **boot ceremony never appears in node logic.**
///
/// This is what makes comparing only the logic file honest. Without it, the
/// gate could be satisfied by moving a divergence into a file it does not read.
#[test]
fn ceremony_stays_out_of_node_logic() {
    let root = examples_dir();
    let mut offenders = Vec::new();
    let Ok(platforms) = fs::read_dir(&root) else {
        panic!("examples/ unreadable");
    };
    for platform in platforms.flatten() {
        if !platform.path().is_dir() {
            continue;
        }
        let plat = platform.file_name().to_string_lossy().to_string();
        if group_of(&plat).is_none() {
            continue;
        }
        for lang in LANGS {
            let Ok(programs) = fs::read_dir(platform.path().join(lang)) else {
                continue;
            };
            for program in programs.flatten() {
                let src = program.path().join("src");
                let Some(files) = classified_files(&src) else {
                    continue;
                };
                let prog = program.file_name().to_string_lossy().to_string();
                for (name, body, kind) in files {
                    if kind != FileKind::Logic {
                        continue;
                    }
                    for marker in CEREMONY_MARKERS {
                        if body.contains(marker) {
                            offenders.push(format!(
                                "{plat}/{lang}/{prog}/src/{name} contains `{marker}` — \
                                 move it into the glue file (src/main.rs)"
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "boot ceremony found in node logic ({} site(s)).\n  {}\n\n\
         phase-338 Option B: node logic and platform glue live in SEPARATE files. \
         Logic is compared across platforms; glue is not. Ceremony in a logic file \
         both breaks portability and hides from the comparison.",
        offenders.len(),
        offenders.join("\n  ")
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
