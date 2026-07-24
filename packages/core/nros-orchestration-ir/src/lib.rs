//! Shared per-tier orchestration IR — schema + resolver (RFC-0015).
//!
//! This leaf crate holds the small set of `system.toml` types that
//! describe scheduling tiers + the [`resolve_tiers`] algorithm that
//! lowers them (with `[[node_overrides]]` applied) into an ordered,
//! per-RTOS **resolved tier table**. It is depended on by BOTH:
//!
//! - the `nros` CLI (`nros-cli-core`), whose `codegen-system` bakes the
//!   resolved table into `nros-plan.json`, and
//! - the `nros::main!()` proc-macro (`nros-macros`), which resolves the
//!   same table at compile time to emit one task/`Executor` per tier.
//!
//! Keeping the schema + resolver here is the single source of truth so
//! the build-time and codegen paths can never drift. The crate is pure
//! host code (serde + thiserror); it carries no runtime/`no_std`
//! dependencies.
//!
//! The all-default-tier degenerate case (no `[tiers.*]`, no callback
//! groups) resolves to a single synthesized `"default"` tier — the
//! single-task shape that ships today.
//!
//! ## Board key mapping
//!
//! [`board_path_for`] is also kept here as the **single source of truth**
//! for the `deploy`-key → board-ZST-path mapping, consumed by both
//! `nros-macros` and `nros-cli-core`. See [`board_path_for`].

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

// phase-296 W5.13 follow-up — the RTOS realizer + its model→MapperInput adapter
// moved here from `nros-cli-core` so BOTH consumers (the CLI's codegen-system
// AND the `nros::main!` proc-macro) can DERIVE a schedule from the contract
// layer, not just resolve authored tiers. No behavior change vs the old
// `nros_cli_core::orchestration::{mapper_input, rtos_realizer}` location.
pub mod derive;
pub mod mapper_input;
pub mod rtos_realizer;

// =============================================================================
// Board key → board ZST path mapping
// =============================================================================

/// Board deployment key → canonical Rust ZST path (double-colon-prefixed,
/// ready for `syn::parse_str` or direct string inclusion in generated code).
///
/// This is the **single source of truth** for the board-key → board-crate
/// mapping, consumed by BOTH:
///
/// - `nros-macros` (`nros::main!(deploy = "…")`) — wraps the returned string
///   in `syn::parse_str::<syn::Path>()` to emit a `BoardEntry` impl call, and
/// - `nros-cli-core` (`nros codegen entry --lang rust`) — includes the string
///   directly in the emitted `main.rs` body.
///
/// Adding a new board requires ONE edit here, not two. Aliases (e.g.
/// `"qemu-arm-freertos"` for `"freertos"`) are normalised here so callers
/// need no extra translation.
///
/// Keys match the `deploy = "…"` strings in
/// `[package.metadata.nros.deploy.<board>]` (RFC-0014 §3).
pub fn board_path_for(key: &str) -> Option<&'static str> {
    Some(match key {
        "native" | "posix" => "::nros_board_native::NativeBoard",
        // FreeRTOS — MPS2-AN385 Cortex-M3 (the only FreeRTOS board today).
        // The RTOS calls `main()`; the board ZST impls `BoardEntry`.
        "freertos" | "freertos-qemu-mps2-an385" | "qemu-arm-freertos" => {
            "::nros_board_mps2_an385_freertos::Mps2An385"
        }
        "threadx-linux" => "::nros_board_threadx_linux::ThreadxLinux",
        "threadx-qemu-riscv64" | "qemu-riscv64-threadx" => {
            "::nros_board_threadx_qemu_riscv64::ThreadxQemuRiscv64"
        }
        "nuttx" | "qemu-arm-nuttx" => "::nros_board_nuttx_qemu_arm::QemuArmVirt",
        // Phase-285 W4 — the rv-virt (riscv32) NuttX sibling. Same OwnedSpin
        // framework routing as arm-nuttx (the board exports its own nsh_main).
        "nuttx-riscv" | "qemu-riscv-nuttx" => "::nros_board_nuttx_qemu_riscv::QemuRvVirt",
        // Phase 225.O — CI-runnable ESP32-C3 QEMU (OpenETH) board. Routed
        // through `Framework::Esp32` emit shape in the proc-macro.
        "esp32-qemu" | "qemu-esp32-baremetal" => "::nros_board_esp32_qemu::Esp32QemuEntry",
        // Phase 225.P — Zephyr owns `main`; the board ZST impls `NetworkWait`
        // only (NOT `BoardEntry`). The proc-macro routes through
        // `Framework::Zephyr` and emits a `rust_main` staticlib export.
        "zephyr" => "::nros_board_zephyr::ZephyrBoard",
        // Phase 216.B.3 — RTIC + STM32F4. Board ZST impls `RticBoardEntry`.
        "rtic-stm32f4" => "::nros_board_rtic_stm32f4::RticStm32F4",
        "rtic-mps2-an385" | "qemu-rtic-mps2-an385" => "::nros_board_rtic_mps2_an385::RticMps2An385",
        // Phase 244.D1 — pure bare-metal (no-RTOS) MPS2-AN385 direct-exec
        // board. Board ZST impls `nros_platform::BoardEntry`. Distinct from
        // `rtic-mps2-an385`, which routes through the RTIC framework emit.
        "qemu-mps2-an385" | "mps2-an385" => "::nros_board_mps2_an385::Mps2An385",
        // Phase 244.C5 — pure bare-metal STM32F4 direct-exec board.
        "stm32f4" => "::nros_board_stm32f4::Stm32F4",
        // Phase 216.C.3 — Embassy + STM32F4. Board ZST impls `EmbassyBoardEntry`.
        "embassy-stm32f4" => "::nros_board_embassy_stm32f4::EmbassyStm32F4",
        _ => return None,
    })
}

// =============================================================================
// system.toml schema (tier subset)
// =============================================================================

/// `[tiers.<name>]` — a symbolic priority tier (RFC-0015 §4.2). Carries the
/// RTOS-agnostic `spin_period_us` plus a per-RTOS sub-table
/// (`[tiers.<name>.<rtos>]`) giving the concrete priority/stack for each target.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierDef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spin_period_us: Option<u64>,
    // Phase 256 W4 (decision A) — the RTOS-AGNOSTIC real-time policy a callback
    // group runs under (absorbed from the retired `[[scheduling.contexts]]`
    // overlay). Per-RTOS placement (priority/stack) stays in the `<rtos>`
    // sub-tables; these describe HOW it is scheduled, identically on every RTOS.
    // All optional → a plain priority tier (today's shape) is byte-identical.
    /// Scheduling class — the plan's `SchedClass` (snake_case): `"best_effort"` |
    /// `"real_time"` (default for a priority tier) | `"time_triggered"` |
    /// `"interrupt"`. The W4.2 codegen lowering validates + maps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// Callback period (µs) for `periodic` / `time_triggered`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_us: Option<u64>,
    /// Execution-time budget (µs) — EDF/sporadic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_us: Option<u64>,
    /// Relative deadline (µs) — EDF.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_us: Option<u64>,
    /// On deadline miss — the plan's `DeadlinePolicy` (snake_case): `"ignore"`
    /// (default) | `"warn"` | `"skip"` | `"fault"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_policy: Option<String>,
    /// CPU core to pin the tier task to (SMP); `None` ⇒ unpinned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freertos: Option<TierRtosSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zephyr: Option<TierRtosSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threadx: Option<TierRtosSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nuttx: Option<TierRtosSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub posix: Option<TierRtosSpec>,
}

/// `[tiers.<name>.<rtos>]` — concrete per-RTOS task knobs. One shape for all
/// RTOSes; `priority` is `i64` to admit Zephyr's negative coop priorities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierRtosSpec {
    pub priority: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_bytes: Option<u32>,
    /// ThreadX preemption threshold (ignored on other RTOSes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preempt_threshold: Option<i64>,
    /// POSIX scheduler class (e.g. `"SCHED_FIFO"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sched_class: Option<String>,
}

/// `[[node.callback_groups]]` row (Phase 228.A, RFC-0015 §4.1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallbackGroupDecl {
    /// Logical id within the node (e.g. `"ctrl_loop"`, `"telemetry"`).
    pub id: String,
    /// `"MutuallyExclusive"` (default) or `"Reentrant"`. v1 treats every group
    /// as mutually-exclusive within its tier task; the field is recorded for
    /// the future multi-worker executor.
    #[serde(default = "default_cbg_type")]
    pub r#type: String,
    /// Symbolic tier name resolved against the system's `[tiers.*]`.
    #[serde(default = "default_tier_name")]
    pub tier: String,
}

fn default_cbg_type() -> String {
    "MutuallyExclusive".to_string()
}

fn default_tier_name() -> String {
    DEFAULT_TIER.to_string()
}

/// `[[node_overrides]]` row — reassigns a node's callback groups to tiers
/// at deploy time without touching the node package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeOverride {
    /// Node instance name (matches a `[[component]].name`).
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub callback_groups: Vec<CallbackGroupOverride>,
}

/// A single `id → tier` reassignment inside a `[[node_overrides]]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallbackGroupOverride {
    pub id: String,
    pub tier: String,
}

// =============================================================================
// resolved tier table
// =============================================================================

/// The synthesized tier used when a callback group names no tier (or none are
/// declared at all). It needs no `[tiers.default]` table.
pub const DEFAULT_TIER: &str = "default";

/// One resolved tier: a concrete RTOS task to emit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedTier {
    pub name: String,
    /// RTOS-specific numeric priority (the spawn-call value).
    pub priority: i64,
    pub stack_bytes: Option<u32>,
    pub spin_period_us: Option<u64>,
    pub preempt_threshold: Option<i64>,
    pub sched_class: Option<String>,
    // Phase 256 W4 — the RTOS-agnostic real-time policy (from `TierDef`), carried
    // through so the planner can lower a tier to a `PlanSchedContext` (the home the
    // retired `[[scheduling.contexts]]` overlay used to fill).
    pub class: Option<String>,
    pub period_us: Option<u64>,
    pub budget_us: Option<u64>,
    pub deadline_us: Option<u64>,
    pub deadline_policy: Option<String>,
    pub core: Option<u32>,
    /// `(node_name, callback_group_id)` pairs assigned to this tier, sorted.
    pub members: Vec<(String, String)>,
}

/// The ordered tier table for one deploy target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedTierTable {
    /// Tiers ordered by RAW numeric `priority`, DESCENDING. The system owner
    /// authors numbers in the target RTOS's own direction and v1 does not
    /// invert (see `resolve_tiers`), so `tiers[0]` — the BOOT tier every
    /// `run_tiers` runs first — is the semantically-HIGHEST tier only on
    /// bigger-number-wins RTOSes (posix/FreeRTOS/NuttX). On
    /// lower-number-wins RTOSes (Zephyr, ThreadX) `tiers[0]` is the
    /// LOWEST-priority tier (issue 0251 — deliberate, comments must not
    /// claim otherwise).
    pub tiers: Vec<ResolvedTier>,
}

impl ResolvedTierTable {
    /// True when this is the single-task degenerate case (one tier, the
    /// synthesized `default`). Codegen uses this to skip multi-task scaffolding.
    pub fn is_single_tier(&self) -> bool {
        self.tiers.len() == 1 && self.tiers[0].name == DEFAULT_TIER
    }

    /// True when at least one node has callback groups on MORE THAN ONE tier
    /// (the RFC-0047 sub-node capability: `group_tiers = { ctrl = "high",
    /// telem = "low" }`). Such a node cannot be expressed by the run_tiers
    /// shape (per-tier executors construct whole nodes), so entry codegen must
    /// keep the single-executor sched-context path (`bind_group_sched`) for
    /// the plan.
    pub fn has_group_split_node(&self) -> bool {
        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
        for (ti, tier) in self.tiers.iter().enumerate() {
            for (node, _group) in &tier.members {
                match seen.get(node.as_str()) {
                    Some(&first_ti) if first_ti != ti => return true,
                    Some(_) => {}
                    None => {
                        seen.insert(node.as_str(), ti);
                    }
                }
            }
        }
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TierResolveError {
    #[error(
        "callback group `{node}/{group}` names tier `{tier}`, which has no `[tiers.{tier}]` definition"
    )]
    UnknownTier {
        node: String,
        group: String,
        tier: String,
    },
    #[error("tier `{tier}` has no `[tiers.{tier}.{rtos}]` sub-table for the target RTOS")]
    MissingRtosSpec { tier: String, rtos: String },
    #[error("`[[node_overrides]]` targets node `{node}` which is not a component in the system")]
    UnknownOverrideNode { node: String },
}

/// Pick a tier's per-RTOS spec by target name.
fn rtos_spec<'a>(def: &'a TierDef, rtos: &str) -> Option<&'a TierRtosSpec> {
    match rtos {
        "freertos" => def.freertos.as_ref(),
        "zephyr" => def.zephyr.as_ref(),
        "threadx" => def.threadx.as_ref(),
        "nuttx" => def.nuttx.as_ref(),
        "posix" | "native" => def.posix.as_ref(),
        _ => None,
    }
}

/// Resolve a system's tiers against the per-node callback groups for one
/// target RTOS.
///
/// Inputs are the decomposed `system.toml` pieces so both the CLI (which
/// holds a full `SystemToml`) and the proc-macro (which parses a leaner
/// view) can call this without sharing the whole config type:
/// - `tiers` — the `[tiers.*]` table.
/// - `node_overrides` — the `[[node_overrides]]` rows.
/// - `component_names` — the system's component *instance* names, used to
///   validate override targets.
/// - `callback_groups` — component-instance-name → its declared groups
///   (`[package.metadata.nros.node].callback_groups`).
pub fn resolve_tiers(
    tiers: &BTreeMap<String, TierDef>,
    node_overrides: &[NodeOverride],
    component_names: &BTreeSet<&str>,
    callback_groups: &BTreeMap<String, Vec<CallbackGroupDecl>>,
    target_rtos: &str,
) -> Result<ResolvedTierTable, TierResolveError> {
    // Per-node group→tier overrides from the system.
    let mut overrides: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    for ov in node_overrides {
        if !component_names.contains(ov.name.as_str()) {
            return Err(TierResolveError::UnknownOverrideNode {
                node: ov.name.clone(),
            });
        }
        for cg in &ov.callback_groups {
            overrides.insert((ov.name.as_str(), cg.id.as_str()), cg.tier.as_str());
        }
    }

    // (node, group) → effective tier (override wins over the node's declaration).
    let mut members_by_tier: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (node, groups) in callback_groups {
        for g in groups {
            let tier = overrides
                .get(&(node.as_str(), g.id.as_str()))
                .copied()
                .unwrap_or(g.tier.as_str());
            members_by_tier
                .entry(tier.to_string())
                .or_default()
                .push((node.clone(), g.id.clone()));
        }
    }

    // Phase 273 W4 (RFC-0047): the v1 node-pinned-to-tier rule is lifted.
    // A single node may now have callback groups in different tiers (sub-node
    // tiering). Each group is individually bound to its sched context via
    // bind_group_sched; the caller is responsible for thread-safety when groups
    // in different tiers run concurrently (RFC-0047 §3).

    // Degenerate: nothing declared → a single synthesized default tier.
    if members_by_tier.is_empty() {
        return Ok(ResolvedTierTable {
            tiers: vec![default_tier(Vec::new())],
        });
    }

    let mut out = Vec::with_capacity(members_by_tier.len());
    for (name, mut members) in members_by_tier {
        members.sort();
        if name == DEFAULT_TIER && !tiers.contains_key(DEFAULT_TIER) {
            // The default tier needs no `[tiers.default]` table.
            out.push(default_tier(members));
            continue;
        }
        let def = tiers.get(&name).ok_or_else(|| {
            let (node, group) = members.first().cloned().unwrap_or_default();
            TierResolveError::UnknownTier {
                node,
                group,
                tier: name.clone(),
            }
        })?;
        let spec =
            rtos_spec(def, target_rtos).ok_or_else(|| TierResolveError::MissingRtosSpec {
                tier: name.clone(),
                rtos: target_rtos.to_string(),
            })?;
        out.push(ResolvedTier {
            name,
            priority: spec.priority,
            stack_bytes: spec.stack_bytes,
            spin_period_us: def.spin_period_us,
            preempt_threshold: spec.preempt_threshold,
            sched_class: spec.sched_class.clone(),
            class: def.class.clone(),
            period_us: def.period_us,
            budget_us: def.budget_us,
            deadline_us: def.deadline_us,
            deadline_policy: def.deadline_policy.clone(),
            core: def.core,
            members,
        });
    }

    // Highest RTOS priority first. (The system owner authors numbers correct for
    // the target RTOS's direction; v1 does not invert.)
    out.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.name.cmp(&b.name)));
    Ok(ResolvedTierTable { tiers: out })
}

fn default_tier(members: Vec<(String, String)>) -> ResolvedTier {
    ResolvedTier {
        name: DEFAULT_TIER.to_string(),
        priority: 0,
        stack_bytes: None,
        spin_period_us: None,
        preempt_threshold: None,
        sched_class: None,
        class: None,
        period_us: None,
        budget_us: None,
        deadline_us: None,
        deadline_policy: None,
        core: None,
        members,
    }
}

// =============================================================================
// platform-applicability validation (RFC-0052 / phase-296 W2)
// =============================================================================

/// Reject tier knobs the SELECTED target cannot honor — a bake-time error,
/// never a silent ignore (the domain-0 lesson). Shared by the CLI codegen
/// path and the `nros::main!` proc-macro so both bakes agree.
///
/// Rules (RFC-0052 rejection table):
/// - `preempt_threshold` in the selected sub-table → ThreadX only.
/// - `sched_class` in the selected sub-table → POSIX-family targets only
///   (`posix`/`native`/`nuttx`).
/// - `class = "interrupt"` → rejected (v1).
/// - `class = "time_triggered"` without `period_us` → rejected.
pub fn validate_tier_platform_applicability(
    table: &ResolvedTierTable,
    target_rtos: &str,
) -> Result<(), String> {
    let posix_family = matches!(target_rtos, "posix" | "native" | "nuttx");
    for t in &table.tiers {
        if t.preempt_threshold.is_some() && target_rtos != "threadx" {
            return Err(format!(
                "tier '{}': preempt_threshold is ThreadX-only, but the selected \
                 target is '{target_rtos}' — remove it from [tiers.{}.{target_rtos}] \
                 (other platforms' sub-tables may keep theirs)",
                t.name, t.name
            ));
        }
        if t.sched_class.is_some() && !posix_family {
            return Err(format!(
                "tier '{}': sched_class applies to POSIX-family targets only, \
                 but the selected target is '{target_rtos}'",
                t.name
            ));
        }
        match t.class.as_deref() {
            Some("interrupt") => {
                return Err(format!(
                    "tier '{}': class = \"interrupt\" is not supported (RFC-0052 v1)",
                    t.name
                ));
            }
            Some("time_triggered") if t.period_us.is_none() => {
                return Err(format!(
                    "tier '{}': class = \"time_triggered\" requires period_us",
                    t.name
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// RFC-0052 — SystemModel → orchestration-ir tier conversion.
//
// The resolved model's tier schema (`ros_launch_manifest_sched::TierDef`)
// differs from this crate's `TierDef` in two deliberate seams; conversion is
// the contract. Both the CLI's `codegen-system --model` and the
// `nros::main!(model = …)` proc-macro call this, so the mapping has ONE home.
// ---------------------------------------------------------------------------

fn rtos_spec_from_model(spec: &ros_launch_manifest_sched::TierPlatformSpec) -> TierRtosSpec {
    TierRtosSpec {
        priority: spec.priority,
        stack_bytes: spec.stack_bytes,
        preempt_threshold: spec.preempt_threshold,
        sched_class: spec.sched_class.clone(),
    }
}

/// Convert one resolved-model tier into the orchestration-ir shape for a
/// target RTOS.
///
/// Two seams (the schemas differ here on purpose):
/// - `core` lives per-platform in the model but at the tier head in the ir —
///   hoisted from the SELECTED target's sub-table.
/// - a per-platform `deadline_us` override (model) tightens the generic head
///   deadline.
pub fn tier_from_model(t: &ros_launch_manifest_sched::TierDef, target_rtos: &str) -> TierDef {
    let selected = t.platform(target_rtos);
    TierDef {
        spin_period_us: t.spin_period_us,
        class: t.class.clone(),
        period_us: selected.and_then(|sp| sp.period_us).or(t.period_us),
        budget_us: selected.and_then(|sp| sp.budget_us).or(t.budget_us),
        deadline_us: selected.and_then(|s| s.deadline_us).or(t.deadline_us),
        // phase-296 W5.9 — per-platform sporadic override (NuttX
        // SCHED_SPORADIC): the SELECTED platform's budget/period hoist into
        // this bake's head, so one platform's kernel sporadic server engages
        // without a generic head budget flipping every other platform's
        // executor into cooperative Sporadic gating.
        deadline_policy: t.deadline_policy.clone(),
        core: selected.and_then(|s| s.core),
        freertos: t.freertos.as_ref().map(rtos_spec_from_model),
        zephyr: t.zephyr.as_ref().map(rtos_spec_from_model),
        threadx: t.threadx.as_ref().map(rtos_spec_from_model),
        nuttx: t.nuttx.as_ref().map(rtos_spec_from_model),
        posix: t.posix.as_ref().map(rtos_spec_from_model),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirror-drift guard: every field of the shared-schema tier must survive
    /// `tier_from_model`. If either schema grows a field, this test is the
    /// tripwire — extend the conversion AND this fixture together.
    #[test]
    fn tier_from_model_covers_every_field() {
        use ros_launch_manifest_sched::{TierDef as ModelTierDef, TierPlatformSpec};
        let model_tier = ModelTierDef {
            class: Some("real_time".to_string()),
            deadline_us: Some(2000),
            period_us: Some(1000),
            budget_us: Some(500),
            deadline_policy: Some("skip".to_string()),
            spin_period_us: Some(250),
            posix: Some(TierPlatformSpec {
                priority: 80,
                stack_bytes: Some(65536),
                core: Some(2),
                sched_class: Some("SCHED_FIFO".to_string()),
                preempt_threshold: None,
                deadline_us: Some(1500), // per-platform tighten
                budget_us: Some(400),    // per-platform sporadic override
                period_us: Some(900),
            }),
            freertos: Some(TierPlatformSpec {
                priority: 5,
                stack_bytes: Some(32768),
                core: None,
                sched_class: None,
                preempt_threshold: None,
                deadline_us: None,
                budget_us: None,
                period_us: None,
            }),
            zephyr: None,
            threadx: Some(TierPlatformSpec {
                priority: 4,
                stack_bytes: None,
                core: Some(1),
                sched_class: None,
                preempt_threshold: Some(4),
                deadline_us: None,
                budget_us: None,
                period_us: None,
            }),
            nuttx: None,
        };

        // Selected target = posix: head core + deadline hoist from posix.
        let ir = tier_from_model(&model_tier, "posix");
        assert_eq!(ir.spin_period_us, Some(250));
        assert_eq!(ir.class.as_deref(), Some("real_time"));
        assert_eq!(ir.period_us, Some(900), "per-platform sporadic period wins");
        assert_eq!(ir.budget_us, Some(400), "per-platform sporadic budget wins");
        assert_eq!(ir.deadline_us, Some(1500), "per-platform override wins");
        assert_eq!(ir.deadline_policy.as_deref(), Some("skip"));
        assert_eq!(ir.core, Some(2), "core hoisted from selected platform");

        // Non-selected sub-table values must NOT leak: freertos declares no
        // sporadic override, so its bake keeps the GENERIC head budget/period.
        let ir_f = tier_from_model(&model_tier, "freertos");
        assert_eq!(ir_f.period_us, Some(1000), "head period stands");
        assert_eq!(ir_f.budget_us, Some(500), "head budget stands");
        let posix = ir.posix.as_ref().unwrap();
        assert_eq!(posix.priority, 80);
        assert_eq!(posix.stack_bytes, Some(65536));
        assert_eq!(posix.sched_class.as_deref(), Some("SCHED_FIFO"));
        let tx = ir.threadx.as_ref().unwrap();
        assert_eq!(tx.priority, 4);
        assert_eq!(tx.preempt_threshold, Some(4));
        assert_eq!(ir.freertos.as_ref().unwrap().stack_bytes, Some(32768));
        assert!(ir.zephyr.is_none() && ir.nuttx.is_none());

        // Selected target = threadx: its core hoists; head deadline stands.
        let ir_tx = tier_from_model(&model_tier, "threadx");
        assert_eq!(ir_tx.core, Some(1));
        assert_eq!(ir_tx.deadline_us, Some(2000));
    }

    fn names<'a>(items: &'a [&'a str]) -> BTreeSet<&'a str> {
        items.iter().copied().collect()
    }

    fn cbg(id: &str, tier: &str) -> CallbackGroupDecl {
        CallbackGroupDecl {
            id: id.to_string(),
            r#type: "MutuallyExclusive".to_string(),
            tier: tier.to_string(),
        }
    }

    fn posix_tier(priority: i64, spin: Option<u64>, stack: Option<u32>) -> TierDef {
        TierDef {
            spin_period_us: spin,
            posix: Some(TierRtosSpec {
                priority,
                stack_bytes: stack,
                preempt_threshold: None,
                sched_class: None,
            }),
            ..Default::default()
        }
    }

    /// Phase 256 W4 (decision A) — a `[tiers.<name>]` carrying the RTOS-agnostic
    /// real-time policy parses, and `resolve_tiers` carries those fields onto the
    /// `ResolvedTier` (the data the planner lowers to a `PlanSchedContext`).
    #[test]
    fn tier_carries_rt_policy_fields() {
        let def = TierDef {
            spin_period_us: Some(1000),
            class: Some("time_triggered".to_string()),
            period_us: Some(20000),
            budget_us: Some(5000),
            deadline_us: Some(18000),
            deadline_policy: Some("fault".to_string()),
            core: Some(1),
            posix: Some(TierRtosSpec {
                priority: 80,
                stack_bytes: Some(8192),
                preempt_threshold: None,
                sched_class: None,
            }),
            ..Default::default()
        };

        let mut tiers = BTreeMap::new();
        tiers.insert("control".to_string(), def);
        let mut cbgs = BTreeMap::new();
        cbgs.insert("control_node".to_string(), vec![cbg("loop", "control")]);
        let table = resolve_tiers(&tiers, &[], &names(&["control_node"]), &cbgs, "posix").unwrap();

        let t = &table.tiers[0];
        assert_eq!(t.name, "control");
        assert_eq!(t.priority, 80); // per-RTOS placement
        assert_eq!(t.class.as_deref(), Some("time_triggered")); // agnostic policy
        assert_eq!(t.period_us, Some(20000));
        assert_eq!(t.budget_us, Some(5000));
        assert_eq!(t.deadline_us, Some(18000));
        assert_eq!(t.deadline_policy.as_deref(), Some("fault"));
        assert_eq!(t.core, Some(1));
    }

    #[test]
    fn no_groups_degenerates_to_single_default_tier() {
        let table = resolve_tiers(
            &BTreeMap::new(),
            &[],
            &names(&["control_node", "telem_node"]),
            &BTreeMap::new(),
            "posix",
        )
        .unwrap();
        assert!(table.is_single_tier());
        assert_eq!(table.tiers[0].name, DEFAULT_TIER);
    }

    #[test]
    fn groups_with_no_tier_table_use_default() {
        let mut cbgs = BTreeMap::new();
        cbgs.insert("control_node".to_string(), vec![cbg("loop", "default")]);
        let table = resolve_tiers(
            &BTreeMap::new(),
            &[],
            &names(&["control_node"]),
            &cbgs,
            "posix",
        )
        .unwrap();
        assert!(table.is_single_tier());
        assert_eq!(
            table.tiers[0].members,
            vec![("control_node".to_string(), "loop".to_string())]
        );
    }

    #[test]
    fn resolves_two_tiers_ordered_by_priority() {
        let mut tiers = BTreeMap::new();
        tiers.insert("high".to_string(), posix_tier(80, Some(1000), Some(8192)));
        tiers.insert("low".to_string(), posix_tier(10, None, None));
        let mut cbgs = BTreeMap::new();
        cbgs.insert("control_node".to_string(), vec![cbg("ctrl", "high")]);
        cbgs.insert("telem_node".to_string(), vec![cbg("telem", "low")]);
        let table = resolve_tiers(
            &tiers,
            &[],
            &names(&["control_node", "telem_node"]),
            &cbgs,
            "posix",
        )
        .unwrap();
        assert_eq!(table.tiers.len(), 2);
        assert_eq!(table.tiers[0].name, "high"); // highest priority first
        assert_eq!(table.tiers[0].priority, 80);
        assert_eq!(table.tiers[0].spin_period_us, Some(1000));
        assert_eq!(table.tiers[1].name, "low");
    }

    /// Phase 273 W4 (RFC-0047): a single node is now ALLOWED to have callback
    /// groups in different tiers (sub-node tiering). The old v1 NodeSpansTiers
    /// error is lifted. Both groups must appear in the resolved tier table.
    #[test]
    fn node_spanning_tiers_is_allowed() {
        let mut tiers = BTreeMap::new();
        tiers.insert("high".to_string(), posix_tier(80, None, None));
        tiers.insert("low".to_string(), posix_tier(10, None, None));
        let mut cbgs = BTreeMap::new();
        cbgs.insert(
            "sub_node".to_string(),
            vec![cbg("ctrl", "high"), cbg("telem", "low")],
        );
        let table = resolve_tiers(&tiers, &[], &names(&["sub_node"]), &cbgs, "posix").unwrap();
        // Both groups resolved: high tier contains (sub_node, ctrl), low tier contains (sub_node, telem).
        let high = table.tiers.iter().find(|t| t.name == "high").unwrap();
        let low = table.tiers.iter().find(|t| t.name == "low").unwrap();
        assert!(
            high.members
                .contains(&("sub_node".to_string(), "ctrl".to_string())),
            "ctrl group must be in high tier"
        );
        assert!(
            low.members
                .contains(&("sub_node".to_string(), "telem".to_string())),
            "telem group must be in low tier"
        );
    }

    #[test]
    fn node_override_moves_a_group_to_another_tier() {
        let mut tiers = BTreeMap::new();
        tiers.insert("high".to_string(), posix_tier(80, None, None));
        tiers.insert("low".to_string(), posix_tier(10, None, None));
        let overrides = vec![NodeOverride {
            name: "telem_node".to_string(),
            callback_groups: vec![CallbackGroupOverride {
                id: "telem".to_string(),
                tier: "low".to_string(),
            }],
        }];
        let mut cbgs = BTreeMap::new();
        cbgs.insert("control_node".to_string(), vec![cbg("ctrl", "high")]);
        cbgs.insert("telem_node".to_string(), vec![cbg("telem", "high")]);
        let table = resolve_tiers(
            &tiers,
            &overrides,
            &names(&["control_node", "telem_node"]),
            &cbgs,
            "posix",
        )
        .unwrap();
        let low = table.tiers.iter().find(|t| t.name == "low").unwrap();
        assert_eq!(
            low.members,
            vec![("telem_node".to_string(), "telem".to_string())]
        );
    }

    #[test]
    fn unknown_tier_errors() {
        let mut cbgs = BTreeMap::new();
        cbgs.insert("control_node".to_string(), vec![cbg("ctrl", "ludicrous")]);
        let err = resolve_tiers(
            &BTreeMap::new(),
            &[],
            &names(&["control_node"]),
            &cbgs,
            "posix",
        )
        .unwrap_err();
        assert!(matches!(err, TierResolveError::UnknownTier { .. }));
    }

    #[test]
    fn missing_rtos_spec_errors() {
        let mut tiers = BTreeMap::new();
        tiers.insert(
            "high".to_string(),
            TierDef {
                // only freertos declared; resolving for posix must fail.
                freertos: Some(TierRtosSpec {
                    priority: 5,
                    stack_bytes: None,
                    preempt_threshold: None,
                    sched_class: None,
                }),
                ..Default::default()
            },
        );
        let mut cbgs = BTreeMap::new();
        cbgs.insert("control_node".to_string(), vec![cbg("ctrl", "high")]);
        let err =
            resolve_tiers(&tiers, &[], &names(&["control_node"]), &cbgs, "posix").unwrap_err();
        assert!(matches!(err, TierResolveError::MissingRtosSpec { .. }));
    }

    #[test]
    fn override_on_unknown_node_errors() {
        let overrides = vec![NodeOverride {
            name: "ghost".to_string(),
            callback_groups: vec![],
        }];
        let err = resolve_tiers(
            &BTreeMap::new(),
            &overrides,
            &names(&["control_node"]),
            &BTreeMap::new(),
            "posix",
        )
        .unwrap_err();
        assert!(matches!(err, TierResolveError::UnknownOverrideNode { .. }));
    }

    // -------------------------------------------------------------------------
    // board_path_for tests
    // -------------------------------------------------------------------------

    /// All primary (canonical) board keys must resolve to a non-None path.
    #[test]
    fn known_boards_resolve() {
        let known = [
            "native",
            "posix",
            "freertos",
            "freertos-qemu-mps2-an385",
            "qemu-arm-freertos",
            "threadx-linux",
            "threadx-qemu-riscv64",
            "qemu-riscv64-threadx",
            "nuttx",
            "qemu-arm-nuttx",
            "nuttx-riscv",
            "qemu-riscv-nuttx",
            "esp32-qemu",
            "qemu-esp32-baremetal",
            "zephyr",
            "rtic-stm32f4",
            "rtic-mps2-an385",
            "qemu-rtic-mps2-an385",
            "qemu-mps2-an385",
            "mps2-an385",
            "stm32f4",
            "embassy-stm32f4",
        ];
        for key in known {
            assert!(
                board_path_for(key).is_some(),
                "board_path_for({key:?}) returned None — missing from the table"
            );
        }
    }

    /// Unknown keys must return None (not silently fall back).
    #[test]
    fn unknown_board_returns_none() {
        assert!(board_path_for("totally-unknown-rtos").is_none());
    }

    /// `freertos` must map to the FreeRTOS board, not NativeBoard.
    #[test]
    fn freertos_key_maps_to_freertos_board() {
        for key in ["freertos", "freertos-qemu-mps2-an385", "qemu-arm-freertos"] {
            let path = board_path_for(key).expect("freertos keys must resolve");
            assert!(
                path.contains("nros_board_mps2_an385_freertos"),
                "key {key:?} resolved to {path:?} — expected mps2_an385_freertos"
            );
            assert!(
                !path.contains("NativeBoard"),
                "key {key:?} fell back to NativeBoard — bug in table"
            );
        }
    }

    /// `zephyr` must map to `ZephyrBoard` (not the old incorrect `Zephyr` name).
    #[test]
    fn zephyr_key_maps_to_zephyr_board() {
        let path = board_path_for("zephyr").expect("zephyr must resolve");
        assert!(
            path.contains("ZephyrBoard"),
            "zephyr resolved to {path:?} — expected ZephyrBoard"
        );
    }
}

#[cfg(test)]
mod applicability_tests {
    use super::*;

    fn table(t: ResolvedTier) -> ResolvedTierTable {
        ResolvedTierTable { tiers: vec![t] }
    }

    fn base() -> ResolvedTier {
        ResolvedTier {
            name: "rt".into(),
            priority: 5,
            stack_bytes: None,
            spin_period_us: None,
            preempt_threshold: None,
            sched_class: None,
            class: None,
            period_us: None,
            budget_us: None,
            deadline_us: None,
            deadline_policy: None,
            core: None,
            members: vec![],
        }
    }

    #[test]
    fn preempt_threshold_rejected_off_threadx() {
        let mut t = base();
        t.preempt_threshold = Some(4);
        assert!(validate_tier_platform_applicability(&table(t.clone()), "threadx").is_ok());
        let err = validate_tier_platform_applicability(&table(t), "zephyr").unwrap_err();
        assert!(err.contains("ThreadX-only"), "{err}");
    }

    #[test]
    fn sched_class_posix_family_only() {
        let mut t = base();
        t.sched_class = Some("SCHED_FIFO".into());
        for ok in ["posix", "native", "nuttx"] {
            assert!(validate_tier_platform_applicability(&table(t.clone()), ok).is_ok());
        }
        assert!(validate_tier_platform_applicability(&table(t), "freertos").is_err());
    }

    #[test]
    fn interrupt_class_and_periodless_tt_rejected() {
        let mut t = base();
        t.class = Some("interrupt".into());
        assert!(validate_tier_platform_applicability(&table(t), "posix").is_err());
        let mut t2 = base();
        t2.class = Some("time_triggered".into());
        assert!(validate_tier_platform_applicability(&table(t2.clone()), "posix").is_err());
        t2.period_us = Some(1000);
        assert!(validate_tier_platform_applicability(&table(t2), "posix").is_ok());
    }
}
