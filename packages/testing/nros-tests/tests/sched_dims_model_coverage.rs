//! phase-329 W2 bake gate — every `SchedDim` a `SCHED_CELLS` cell expects for a
//! `(platform, lang)` coordinate is DECLARED in the authored `ws-realtime`
//! `system.toml`.
//!
//! ## Why this is the missing W2 piece (issue 0380)
//!
//! `sched_dims_applied_e2e` boots a fixture and checks a dim is HONORED at
//! runtime; `matrix::sched_dims_table_covers_every_dim` checks the TABLE has a
//! cell per dim. Neither catches the 0380 failure: a realtime model that SILENTLY
//! LOST a dim. When regeneration stripped `deadline_us`, nothing structural said
//! which `(dim × platform)` cells should exist to lose — the QEMU e2e just
//! quietly stopped exercising EDF and still "passed" (fallback is a legal shape).
//!
//! Post phase-330 (RFC-0063) the SystemModel is a BUILD ARTIFACT — never
//! committed — so the dims are authored in `system.toml`'s
//! `[tiers.<tier>.<rtos>]` blocks (and lower into the model from there). THAT file
//! is the SSoT this gate reads. A stripped dim now fails HERE, in a tier-1 host
//! test that runs before any QEMU boot, instead of silently no-op'ing downstream.
//!
//! The mapping tables below (lang→workspace, platform→rtos key, dim→toml key)
//! panic on an unmapped coordinate, so adding a `SCHED_CELLS` cell for a new
//! platform/lang forces the author to point this gate at the dim's authored home
//! rather than let it pass vacuously.

use nros_tests::matrix::{Lang, PlatformId, SCHED_CELLS, SchedDim};
use std::{collections::BTreeMap, path::PathBuf};

/// The authored bringup `system.toml` for a language's `ws-realtime` workspace.
///
/// issue 0260 — keyed on the DIM as well as the language, because a workspace
/// can carry more than one bringup and a cell declares its dims in exactly one
/// of them. `CorePinPlacement` is declared in `smp_bringup` (`core = 1`), which
/// only an SMP image can honour; `demo_bringup` deliberately declares no `core`
/// so the uniprocessor rows keep their arm. Resolving every cell to
/// `demo_bringup` would check the wrong file and report a dim ABSENT that is
/// authored two directories over — a gate examining something other than what
/// the cell uses, which is the failure this gate exists to catch.
fn workspace_system_toml(lang: Lang, dim: SchedDim) -> PathBuf {
    let bringup = match dim {
        SchedDim::CorePinPlacement => "src/smp_bringup/system.toml",
        // issue 1537 — the derived-tier image has its own bringup, because
        // what it must NOT declare (tiers) is what `demo_bringup` does declare.
        SchedDim::DerivedTierBelowTransport => "src/derived_bringup/system.toml",
        _ => "src/demo_bringup/system.toml",
    };
    let ws = match lang {
        Lang::Rust => "realtime-rust",
        Lang::Cpp => "realtime-cpp",
        Lang::C => "realtime-c",
        other => panic!(
            "SCHED_CELLS carries lang {other:?} with no ws-realtime workspace mapping — \
             add it to `workspace_system_toml`"
        ),
    };
    nros_tests::project_root()
        .join("examples/workspaces")
        .join(ws)
        .join(bringup)
}

/// The `[tiers.<tier>.<rtos>]` sub-table key a platform's kernel dims live under.
fn rtos_key(p: PlatformId) -> &'static str {
    match p {
        PlatformId::ZephyrNativeSim => "zephyr",
        PlatformId::NuttxArm => "nuttx",
        PlatformId::ThreadxLinux => "threadx",
        PlatformId::FreertosMps2 => "freertos",
        // phase-337 W8.b renamed the host BOARD variant to `Linux`; the RTOS key
        // stays `posix`, which is the point of the split — the board layer names
        // what we support, the platform layer names the software stack.
        PlatformId::Linux => "posix",
        other => panic!(
            "SCHED_CELLS carries platform {other:?} with no [tiers.*.<rtos>] key mapping — \
             add it to `rtos_key`"
        ),
    }
}

/// The `system.toml` key(s) that DECLARE a dim.
///
/// Two nesting levels, and they mean different things: the OUTER slice is
/// ALL-OF (every group must be satisfied — `SporadicBudget` needs both a budget
/// and a period), the INNER slice is ANY-OF (the accepted spellings of one
/// field, canonical first).
///
/// ## Why both spellings, and why this gate would go QUIET without them
///
/// `ros-launch-manifest` v0.1.11 moved the unit out of the field NAME and into
/// the VALUE — `deadline_us = 10000` became `deadline = "10000us"` — because
/// `budget_us = 8` written by an author who meant 8 ms is a thousandfold error
/// that type-checks. rlm keeps the old names as serde ALIASES through the
/// deprecation window (`check/src/rules/deprecated_unit_suffix.rs` carries the
/// authoritative rename table this mirrors), so both spellings parse and an
/// un-migrated `system.toml` behaves identically.
///
/// This gate must mirror that leniency, and the reason is the exact failure
/// class it was built for (issue 0380). It asks "is this dim DECLARED?". A
/// name-only lookup does not ERROR when the data migrates ahead of it — it
/// answers "absent", the dim reads as uncovered, and the report blames the
/// authored file for a rename that happened in the consumer. The mirror image
/// is worse: pin this to the NEW names only and every not-yet-migrated file
/// reads as a stripped model. Either way a gate that should be describing the
/// data starts describing its own staleness. A gate that goes quiet — or that
/// goes red about the wrong thing — is worse than one that goes red honestly,
/// so both spellings count, exactly as rlm counts them.
///
/// Note this gate tests key PRESENCE only; it never reads the value, so the new
/// `us`/`ms` suffix on the value is not its concern (a malformed value fails
/// loudly in `toml::from_str` at load, which is the right place for it).
fn dim_keys(d: SchedDim) -> &'static [&'static [&'static str]] {
    match d {
        SchedDim::CorePin => &[&["core"]],
        // issue 0260 — same `core` key; the difference is the IMAGE (multi-core)
        // and therefore what the runtime assert can claim, not the declaration.
        SchedDim::CorePinPlacement => &[&["core"]],
        SchedDim::EdfDeadline => &[&["deadline", "deadline_us"]],
        SchedDim::PreemptThreshold => &[&["preempt_threshold"]],
        SchedDim::TimeSlice => &[&["time_slice", "time_slice_us"]],
        SchedDim::SporadicBudget => &[&["budget", "budget_us"], &["period", "period_us"]],
        SchedDim::TierPriority => &[&["priority"]],
        // issue 1537 — nothing is AUTHORED for this dim; that is the dim. What
        // must hold of its bringup is checked by `derived_bringup_problems`.
        SchedDim::DerivedTierBelowTransport => &[],
    }
}

/// issue 1537 — the inverse of a declared dim: the derived-tier cell's bringup
/// must AUTHOR no tiers (else `nros::main!` reads them and never derives, and
/// the cell's runtime assert passes over a code path it never exercised), must
/// state the rates the derivation ranks by (the contract beside the launch
/// file), and its image must name the lowered-band fragment, which must set the
/// band. Each problem is a string naming what is wrong.
fn derived_bringup_problems(system_toml: &std::path::Path, doc: &toml::Value) -> Vec<String> {
    let bringup = system_toml.parent().expect("system.toml has a parent");
    let mut out = Vec::new();
    if doc.get("tiers").is_some() {
        out.push(format!(
            "{} declares a `[tiers]` table — the macro would READ it and never derive",
            system_toml.display()
        ));
    }
    // A node with no callback group gets no derived tier, and a table where
    // every node is groupless derives NOTHING — the image boots
    // single-executor and never calls the boot report. Measured: that is what
    // this bringup's first version built.
    let components = doc
        .get("component")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    if components.is_empty() {
        out.push(format!(
            "{} declares no [[component]]",
            system_toml.display()
        ));
    }
    for c in &components {
        let grouped = c
            .get("group_tiers")
            .and_then(|g| g.as_table())
            .is_some_and(|g| !g.is_empty());
        if !grouped {
            out.push(format!(
                "component {:?} names no callback group (`group_tiers`) — it would get no \
                 derived tier",
                c.get("name").and_then(|n| n.as_str()).unwrap_or("?")
            ));
        }
    }
    let contract = bringup.join("launch/system.contract.yaml");
    if !contract.is_file() {
        out.push(format!(
            "{} is missing — with no rates there is nothing to derive from",
            contract.display()
        ));
    }
    let conf = doc
        .get("image")
        .and_then(|i| i.get("zephyr"))
        .and_then(|z| z.get("conf"))
        .and_then(|c| c.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();
    const FRAGMENT: &str = "prj-lowered-band.conf";
    if conf.last() != Some(&FRAGMENT) {
        out.push(format!(
            "`[image.zephyr] conf` must name `{FRAGMENT}` LAST (Kconfig merges last-wins, issue 0876); it is {conf:?}"
        ));
    }
    let fragment = bringup.join("boards/native_sim_native_64").join(FRAGMENT);
    let sets_band = std::fs::read_to_string(&fragment)
        .map(|t| {
            t.lines()
                .any(|l| l.trim() == "CONFIG_NROS_ZENOH_READ_PRIORITY=100")
        })
        .unwrap_or(false);
    if !sets_band {
        out.push(format!(
            "{} does not set CONFIG_NROS_ZENOH_READ_PRIORITY=100 — on a default band the projection and the image's plan agree, and the cell cannot see a regression",
            fragment.display()
        ));
    }
    out
}

/// Is `key` declared for `rtos` anywhere in the parsed system.toml — either under
/// a `[tiers.<tier>.<rtos>]` sub-table (kernel-scoped) or directly on a
/// `[tiers.<tier>]` table (generic, applies to every platform)?
fn key_declared(doc: &toml::Value, rtos: &str, key: &str) -> bool {
    let Some(tiers) = doc.get("tiers").and_then(|t| t.as_table()) else {
        return false;
    };
    for (_tier_name, tier) in tiers {
        let Some(tier_tbl) = tier.as_table() else {
            continue;
        };
        // Generic dim on [tiers.<tier>] — applies to all platforms.
        if tier_tbl.get(key).is_some() {
            return true;
        }
        // Kernel-scoped dim on [tiers.<tier>.<rtos>].
        if tier_tbl
            .get(rtos)
            .and_then(|r| r.as_table())
            .and_then(|r| r.get(key))
            .is_some()
        {
            return true;
        }
    }
    false
}

/// Is ANY accepted spelling of one field declared? `aliases` is one inner group
/// from [`dim_keys`] — canonical name first, deprecated names after — mirroring
/// rlm's serde `alias` on the same field.
fn any_alias_declared(doc: &toml::Value, rtos: &str, aliases: &[&str]) -> bool {
    aliases.iter().any(|key| key_declared(doc, rtos, key))
}

#[test]
fn sched_dims_are_declared_in_authored_system_toml() {
    // Parse each referenced workspace system.toml once.
    // Keyed on the PATH, not the workspace: since issue 0260 a workspace can
    // carry more than one bringup, so (lang, dim) — not lang alone — decides
    // which file a cell is checked against.
    let mut docs: BTreeMap<PathBuf, toml::Value> = BTreeMap::new();
    let load = |path: &PathBuf| -> toml::Value {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {} : {e}", path.display()));
        toml::from_str(&text).unwrap_or_else(|e| panic!("parse {} : {e}", path.display()))
    };

    let mut missing: Vec<String> = Vec::new();
    for cell in SCHED_CELLS {
        let ws = match cell.lang {
            Lang::Rust => "realtime-rust",
            Lang::Cpp => "realtime-cpp",
            Lang::C => "realtime-c",
            _ => "",
        };
        let path = workspace_system_toml(cell.lang, cell.dim);
        let doc = docs
            .entry(path.clone())
            .or_insert_with(|| load(&path))
            .clone();
        let rtos = rtos_key(cell.platform);
        if cell.dim == SchedDim::DerivedTierBelowTransport {
            missing.extend(derived_bringup_problems(&path, &doc).into_iter().map(|p| {
                format!(
                    "  - {:?}/{:?}/{:?}: {p}",
                    cell.dim, cell.platform, cell.lang
                )
            }));
        }
        for aliases in dim_keys(cell.dim) {
            if !any_alias_declared(&doc, rtos, aliases) {
                // Name every spelling that would have satisfied it, so a reader
                // of the failure can tell "the dim was stripped" from "the dim
                // is spelled a way this gate does not know about".
                let spellings = aliases
                    .iter()
                    .map(|k| format!("`{k}`"))
                    .collect::<Vec<_>>()
                    .join(" or ");
                missing.push(format!(
                    "  - {:?}/{:?}/{:?}: dim {:?} needs {spellings} under \
                     [tiers.*.{rtos}] (or a generic [tiers.*]) in ws-realtime-{} — \
                     it is ABSENT (stripped model? issue 0380)",
                    cell.dim, cell.platform, cell.lang, cell.dim, ws
                ));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "phase-329 W2 bake gate: {} SCHED_CELLS dim(s) are not declared in the \
         authored ws-realtime system.toml — a stripped dim would silently stop \
         being honored at boot instead of failing here:\n{}",
        missing.len(),
        missing.join("\n")
    );
}
