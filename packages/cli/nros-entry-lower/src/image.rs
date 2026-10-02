//! The IMAGE half of [`LoweredEntry`](crate::LoweredEntry) — what a C-ABI
//! entry pack renders beyond the per-node bake.
//!
//! phase-474 W2 / RFC-0091 §6b. The C and C++ emitters each projected the
//! same `Plan` into their own view structs — seven in one, four in the other,
//! six more shared through string-free helpers — and reading them side by side
//! showed the projection was the SAME one written twice: the same per-tier
//! setups, the same node index on each tier's executor (issue 1272), the same
//! tier rows (issue 1172), sched binds (issue 1283), boot config (issue 0794)
//! and services. These types are that projection, declared once, so a pack
//! renders from `LoweredEntry` itself and needs no Rust emitter of its own.
//!
//! Same rules as the rest of this crate (RFC-0091 §8b): every field is a
//! VALUE, every string is RAW, and nothing here is spelled in any language. A
//! pack quotes through its escaping filter, spells `NULL`/`nullptr` for a
//! `None`, writes its own integer suffixes, and decides its own symbol names.
//! The ENCODINGS a C ABI fixes (`core_plus1`, a `-1` preempt threshold, the
//! lifecycle code) are facts and are computed here, because a pack must not
//! have to know them.
//!
//! Every field is `#[serde(default)]` on `LoweredEntry`: the Rust parity
//! corpus (`testdata/parity/`, read by both the CLI and the proc-macro) states
//! none of them, and must keep deserialising.

use alloc::{string::String, vec::Vec};

use crate::{BoardFamily, CAbiRunners};

/// How a component is constructed — the fact that picks a node's body.
///
/// From the cmake metadata (RFC-0043 / RFC-0044 / phase-257): a C component is
/// reached through its `NROS_C_COMPONENT` factory/configure seam, a Rust one
/// self-creates through its install seam, and a C++ one is either a class the
/// entry default-constructs and `configure`s or an `rclcpp`-shape class that
/// OWNS its node and is constructed with a handle.
///
/// This is a property of the COMPONENT, not of the entry language: a C++ entry
/// constructs all four kinds, a C entry only `C`. Which kinds a pack accepts is
/// that pack's manifest data (`components = [...]`), not a Rust `match`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKind {
    C,
    Rust,
    Rclcpp,
    Configure,
}

impl ComponentKind {
    /// Every kind, so a consumer that must handle all of them iterates.
    pub const ALL: [ComponentKind; 4] = [
        ComponentKind::C,
        ComponentKind::Rust,
        ComponentKind::Rclcpp,
        ComponentKind::Configure,
    ];

    /// The serde spelling — what a manifest's `components` list names.
    pub fn as_str(self) -> &'static str {
        match self {
            ComponentKind::C => "c",
            ComponentKind::Rust => "rust",
            ComponentKind::Rclcpp => "rclcpp",
            ComponentKind::Configure => "configure",
        }
    }

    /// Whether this kind is a C++ CLASS, i.e. has a class name and header.
    pub fn is_cpp_class(self) -> bool {
        matches!(self, ComponentKind::Rclcpp | ComponentKind::Configure)
    }
}

/// One component seam an entry declares: a package, reached as `kind`.
///
/// Deduped by (identifier, kind) in first-seen order — forward declarations
/// are per PACKAGE while node storage is per NODE, so this list and
/// `LoweredEntry::nodes` differ in length whenever a launch runs one package
/// twice. `pkg` is RAW; a pack spells the identifier with `pkg_ident`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ComponentSeam {
    pub pkg: String,
    pub kind: ComponentKind,
}

/// One node on one executor: which node, and the index that executor's
/// `node_builder` hands it (issue 1272 — the index restarts per tier).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SetupNode {
    /// Index into `LoweredEntry::nodes`.
    pub node: usize,
    /// The node's position among the nodes ONE setup builds on ONE executor.
    pub on_executor: usize,
}

/// The board's C-ABI runners, as symbol NAMES.
///
/// A C symbol name is part of the C ABI, not of any one template (RFC-0091
/// §8b): the C pack calls these, and so would any language that speaks the C
/// ABI. Carried rather than assembled from a prefix — `BoardFamily::c_abi_runners`
/// says why. A pack whose board call is NOT the C ABI (C++'s `Board::` class)
/// ignores them.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LoweredRunners {
    pub run_components: Option<String>,
    pub run_tiers: Option<String>,
    /// See [`CAbiRunners::takes_executor_storage`].
    pub takes_executor_storage: bool,
}

impl From<CAbiRunners> for LoweredRunners {
    fn from(r: CAbiRunners) -> Self {
        LoweredRunners {
            run_components: r.run_components.map(String::from),
            run_tiers: r.run_tiers.map(String::from),
            takes_executor_storage: r.takes_executor_storage,
        }
    }
}

/// How the entry hands control to the board. `None` on a metadata probe,
/// which returns before the board wrapper.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LoweredBoot {
    pub runners: LoweredRunners,
    /// The plan runs one executor per tier (`ExecutorShape::Tiers`).
    pub tiers: bool,
    /// Issue 1551 — the tier runner takes every tier's executor storage from
    /// the entry. Only ever true with `tiers`.
    pub tier_storage: bool,
    /// Issue 1568 — the C-ABI `run_components` takes the single executor's
    /// storage from the entry. Only ever true WITHOUT `tiers`. A pack that
    /// does not call the C-ABI runner (C++'s `Board::run_components` reaches
    /// the same `.bss` shape through `GlobalStorageHolder`) ignores it.
    pub component_storage: bool,
    /// The tier count, when `tiers`; 0 otherwise.
    pub n_tiers: usize,
    /// Issues 1598 + 1232 — each spawned tier's TASK memory (stack + control
    /// block), declared by the entry for a family whose tier runner takes it
    /// ([`crate::CAbiRunners::tier_task_memory`]). `None` without `tiers` or
    /// for a family whose kernel allocates the stack itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_memory: Option<LoweredTaskMemory>,
}

/// The tier tasks' memory an entry declares (issues 1598 + 1232). One row per
/// tier, by tier index.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LoweredTaskMemory {
    /// The header that spells the per-RTOS declaration macros
    /// (`NROS_TIER_TASK_MEMORY_DEFINE` / `_MEMORY` / `_MEMORY_NONE`).
    pub header: String,
    pub rows: Vec<TaskStackRow>,
}

/// One tier's row of [`LoweredTaskMemory`].
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TaskStackRow {
    pub index: usize,
    /// `false` for the boot tier, which runs on the caller and has no memory.
    pub spawned: bool,
    /// The declared size, or the family's stated default; `0` lets the
    /// family header pick its board knob.
    pub bytes: u64,
}

impl LoweredTaskMemory {
    /// The rows for a tier table, from the family's lowering
    /// ([`crate::TierTaskMemory::stacks`]). `stack_bytes` 0 is "undeclared".
    pub fn for_tiers(mem: &crate::TierTaskMemory, stack_bytes: &[u64]) -> Self {
        let declared: alloc::vec::Vec<Option<u64>> =
            stack_bytes.iter().map(|b| (*b > 0).then_some(*b)).collect();
        LoweredTaskMemory {
            header: mem.header.into(),
            rows: mem
                .stacks(&declared)
                .into_iter()
                .enumerate()
                .map(|(index, bytes)| TaskStackRow {
                    index,
                    spawned: bytes.is_some(),
                    bytes: bytes.unwrap_or(0),
                })
                .collect(),
        }
    }

    /// The bytes the spawned tiers' stacks reserve, summed — what left the
    /// RTOS heap for `.bss` (issue 1598). A row of `0` defers to the family
    /// header's board knob, so it is counted as nothing: this is a floor on
    /// what moved, never more than the entry declares.
    pub fn spawned_bytes(&self) -> u64 {
        self.rows
            .iter()
            .filter(|r| r.spawned)
            .map(|r| r.bytes)
            .sum()
    }
}

/// The `run_tiers` executor layout: one setup per tier, and the rows of the
/// tier table the runner walks.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LoweredTiers {
    pub n: usize,
    pub setups: Vec<TierSetup>,
    pub rows: Vec<TierRow>,
}

/// One tier's setup function: the nodes it builds on the tier's executor.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TierSetup {
    pub index: usize,
    pub name: String,
    pub nodes: Vec<SetupNode>,
    /// Only tier 0 registers param services and lifecycle — process facts.
    pub emits_services: bool,
    /// phase-462 W1 — this tier's contract monitor rows, installed on its
    /// executor before its first node. `None` when it has none.
    pub monitors: Option<MonitorTable>,
}

/// One tier row, in neutral terms — what `nros_native_tier_spec_t` holds.
///
/// The ENCODINGS are ABI facts: `core_plus1` is the core index plus one with 0
/// meaning unpinned, `preempt_threshold` is -1 when unset. `None` strings are
/// unset; the pack spells that.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TierRow {
    pub index: usize,
    pub name: String,
    /// `(node name, node namespace, group)` per admitted group (issue 1172).
    pub groups: Vec<(String, String, String)>,
    pub priority: i64,
    pub stack_bytes: u64,
    pub spin_period_us: u64,
    pub core_plus1: u32,
    pub preempt_threshold: i64,
    pub class: Option<String>,
    pub period_us: u64,
    pub budget_us: u64,
    pub deadline_us: u64,
    pub deadline_policy: Option<String>,
}

/// The sched-context wiring on ONE executor (`ExecutorShape::SchedContexts`,
/// issue 1283): one context per tier, then the node-name and callback-group
/// bindings.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LoweredSched {
    pub n: usize,
    pub contexts: Vec<SchedContext>,
    pub node_binds: Vec<NodeBind>,
    pub group_binds: Vec<GroupBind>,
}

/// One tier's RTOS-agnostic policy, RAW, as
/// `nros_cpp_create_sched_context_from_policy` takes it (RFC-0052).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SchedContext {
    pub index: usize,
    pub class: Option<String>,
    pub period_us: u64,
    pub budget_us: u64,
    pub deadline_us: u64,
    pub deadline_policy: Option<String>,
    pub os_pri: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeBind {
    pub name: String,
    pub namespace: String,
    pub sched_context: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GroupBind {
    pub name: String,
    pub namespace: String,
    pub group: String,
    pub tier_index: usize,
}

/// The param-services and lifecycle registrations — PROCESS facts, which is
/// why a tiered entry emits them in tier 0 alone.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LoweredServices {
    pub param_services: bool,
    /// `None` when the plan declares no lifecycle; otherwise the autostart
    /// code the C ABI takes (0 none, 1 configure, 2 active).
    pub lifecycle_code: Option<u8>,
}

/// The `.nros_boot_config` blob's facts (RFC-0045, issue 0794). Identity is
/// per NODE, so only a one-node plan carries it; session facts are per IMAGE.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LoweredBootConfig {
    pub node_name: Option<String>,
    pub namespace: Option<String>,
    /// The blob's field is `uint32_t`; the model's is `u8`.
    pub domain: Option<u32>,
    pub locator: Option<String>,
    pub rmw: Option<String>,
    /// Session fields at least one node declared and the image could not
    /// agree on — rendered as a comment above the blob.
    pub conflicts: Vec<String>,
}

/// The contract monitor rows ONE executor installs (phase-462 W1, RFC-0052).
///
/// `tier` is the tier whose setup installs it, `None` for the single
/// executor. A pack derives its symbol names and banner from it.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MonitorTable {
    pub tier: Option<usize>,
    pub tier_name: Option<String>,
    pub rows: Vec<MonitorRow>,
    pub ages: Vec<AgeRow>,
}

impl MonitorTable {
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.ages.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MonitorRow {
    pub topic: String,
    pub fqn: String,
    pub min_rate_hz_milli: u32,
    pub max_latency_ms: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgeRow {
    pub topic: String,
    pub fqn: String,
    pub max_age_ms: u32,
}

/// phase-308 — a metadata probe's tail: the identity it stamps into the
/// sidecar it writes. A probe runs setup once and dumps; it has no board
/// wrapper and no boot-config blob.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LoweredProbe {
    pub package: String,
    pub component: String,
    pub executable: String,
    /// The sidecar's `language` field, in `nros_lang::Language`'s serde repr.
    pub language: String,
    pub out_path: String,
}

/// The family's serde spelling, for a consumer holding only the string.
///
/// Used by a pack's spelling filter (`cpp_board_class`), which receives the
/// serialised family from the template; an unknown string is `None`, never a
/// guess.
pub fn family_from_str(s: &str) -> Option<BoardFamily> {
    BoardFamily::ALL.into_iter().find(|f| f.as_str() == s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_family_round_trips_through_its_spelling() {
        for f in BoardFamily::ALL {
            assert_eq!(family_from_str(f.as_str()), Some(f));
        }
        assert_eq!(family_from_str("zigos"), None);
    }

    /// What moved to `.bss` is every SPAWNED tier's stack — the boot tier
    /// runs on the caller and reserves none — with an undeclared tier at the
    /// family default. Issue 1598's follow-up subtracts exactly this from the
    /// FreeRTOS heap default, so counting the boot tier would shrink the heap
    /// by a stack that never left it.
    #[test]
    fn spawned_bytes_counts_spawned_tiers_only() {
        let mem = crate::TierTaskMemory {
            header: "h",
            boot_tier: crate::BootTier::Last,
            default_stack_bytes: 262_144,
        };
        // tier 0 declares 4 KiB, tier 1 nothing, tier 2 is the boot tier.
        let lowered = LoweredTaskMemory::for_tiers(&mem, &[4096, 0, 999]);
        assert_eq!(lowered.spawned_bytes(), 4096 + 262_144);
        // A Zephyr-shaped default of 0 defers to a board knob, so it states
        // nothing rather than a number it does not know.
        let deferred = crate::TierTaskMemory {
            default_stack_bytes: 0,
            ..mem
        };
        assert_eq!(
            LoweredTaskMemory::for_tiers(&deferred, &[0, 0]).spawned_bytes(),
            0
        );
    }
}
