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
fn workspace_system_toml(lang: Lang) -> PathBuf {
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
        .join("src/demo_bringup/system.toml")
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

/// The `system.toml` key(s) that DECLARE a dim. All must be present for the dim
/// to count as declared (SporadicBudget needs both budget + period).
fn dim_keys(d: SchedDim) -> &'static [&'static str] {
    match d {
        SchedDim::CorePin => &["core"],
        SchedDim::EdfDeadline => &["deadline_us"],
        SchedDim::PreemptThreshold => &["preempt_threshold"],
        SchedDim::TimeSlice => &["time_slice_us"],
        SchedDim::SporadicBudget => &["budget_us", "period_us"],
        SchedDim::TierPriority => &["priority"],
    }
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

#[test]
fn sched_dims_are_declared_in_authored_system_toml() {
    // Parse each referenced workspace system.toml once.
    let mut docs: BTreeMap<&'static str, toml::Value> = BTreeMap::new();
    let load = |lang: Lang| -> (PathBuf, toml::Value) {
        let path = workspace_system_toml(lang);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {} : {e}", path.display()));
        let doc: toml::Value =
            toml::from_str(&text).unwrap_or_else(|e| panic!("parse {} : {e}", path.display()));
        (path, doc)
    };

    let mut missing: Vec<String> = Vec::new();
    for cell in SCHED_CELLS {
        let ws = match cell.lang {
            Lang::Rust => "realtime-rust",
            Lang::Cpp => "realtime-cpp",
            Lang::C => "realtime-c",
            _ => "",
        };
        let doc = docs.entry(ws).or_insert_with(|| load(cell.lang).1).clone();
        let rtos = rtos_key(cell.platform);
        for key in dim_keys(cell.dim) {
            if !key_declared(&doc, rtos, key) {
                missing.push(format!(
                    "  - {:?}/{:?}/{:?}: dim {:?} needs `{key}` under \
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
