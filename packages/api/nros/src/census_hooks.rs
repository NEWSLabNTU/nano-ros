//! The census hooks — the executor-side entities the recording RMW backend
//! cannot see, for EVERY language's node API (issue 1419, issue 1556 item 1,
//! RFC-0100 Amendment 1).
//!
//! Publishers, subscriptions, services and clients reach the RMW session, so
//! `nros-rmw-metadata` records them with no help from here. **Timers and guard
//! conditions never touch the RMW** — they register directly on the executor —
//! so a backend cannot observe them at all, and they are precisely the entities
//! a SystemModel also cannot see (issue 0257). And the RMW's `create_publisher`
//! carries no node — by that layer the owning node is resolved away — so a
//! backend alone yields a census whose entities belong to no node.
//!
//! Four hooks close that: [`on_node_create`] opens a node and makes it current
//! (`configure()` / `register()` declares one node's entities at a time, so a
//! cursor is exact, not a guess); [`on_timer_create`] and
//! [`on_guard_condition_create`] record a callback slot the backend never sees;
//! [`on_param_declare`] records a parameter as the code declared it.
//!
//! # Why the hooks live HERE
//!
//! They were born in `nros-cpp` (phase-308, phase-463 W1), which made the
//! census a C++-only instrument: a Rust node's `create_node` / timers /
//! parameters never crossed them, and a C node that opens its own node through
//! `nros-c` reached the recording backend with no node attribution and its
//! timers not at all. `nros` is the crate every language's node API sits on —
//! `nros-c` and `nros-cpp` both depend on it, and a Rust node's `register()`
//! runs through its `node_runtime` — so ONE hook layer here serves all three.
//! A copy per API crate would be the second spelling this repository keeps
//! paying for.
//!
//! # Cost
//!
//! The CALLS are unconditional, in every shipping path; the BODIES exist only
//! under `metadata-mode`, a host-only feature no RTOS board can reach
//! (`check-rtos-feature-set-excludes-analysis`). Without it every hook is an
//! empty `#[inline]` function and compiles to nothing.
//!
//! # Layer rule (phase-308)
//!
//! These are ADAPTERS. No JSON, no schema struct, no slot arithmetic — those
//! live once in [`crate::node_metadata`] and `crate::metadata_mode` (plain
//! text, not a link: that module exists only under `metadata-mode`).
//! `check-census-hooks-complete` holds both the rule and every entry point's
//! call.

/// Issue 1419 -- the census executor's sizing, independent of the contract it
/// checks. ONE spelling for every hosted census funnel (`nros-cpp`'s and
/// `nros-board-linux`'s).
///
/// A native image's callback table, node table and arena are DERIVED from the
/// contract (`nros ws entity-inventory`, phase-412), with no headroom on
/// purpose, and the census producer is that same image. So a contract one
/// entity short sized its own census run one slot short: setup stopped at
/// `ExecutorFull` before the recorder saw the entity that did not fit. A census
/// opens its executor at the executor's own ceilings instead: 64 callback slots
/// (the `u64` ready-set bitmask), 64 nodes, and 16 MiB of `MaybeUninit` arena
/// (address space, not resident memory). Census-only: the backing is leaked
/// from the heap of a host process that writes one file and exits, and none of
/// it exists on the RTOS road, where `metadata-mode` is not compiled.
#[cfg(all(feature = "metadata-mode", feature = "rmw-cffi"))]
pub const CENSUS_SIZING: nros_node::ExecutorSizing = {
    let d = nros_node::ExecutorSizing::DEFAULT;
    nros_node::ExecutorSizing {
        cbs: 64,
        sc: d.sc,
        nodes: if d.nodes > 64 { d.nodes } else { 64 },
        arena: if d.arena > (16 << 20) {
            d.arena
        } else {
            16 << 20
        },
    }
};

/// A node was created — make it current so subsequent entities attribute to
/// it.
///
/// Re-opening a node of the same name makes it current again rather than
/// failing: a tiered Rust entry runs a component's `register()` once per tier
/// executor, and each tier's run creates the same node (issue 1419).
#[inline]
pub fn on_node_create(_name: &str, _namespace: &str, _domain_id: u32) {
    #[cfg(feature = "metadata-mode")]
    {
        // A refused begin means the recorder is full; every entity after it
        // would be silently dropped, so say so rather than produce a census
        // that under-counts.
        if !crate::metadata_mode::begin_node(_name, _namespace, _domain_id) {
            panic!(
                "nros metadata mode: recorder rejected node `{_name}` — raise the \
                 MetadataRecorder capacity"
            );
        }
    }
}

/// A timer was registered on the executor.
///
/// Timers carry no name at the C and C++ ABIs (they are bound by function
/// identity — `bind_timer<T, &T::method>`), so the recorded id is synthetic.
/// That is fine: the count is what the executor sizing reads.
///
/// `kind` names the entry point (wall / clock / oneshot / in-group) so the
/// census can tell a repeating timer from a one-shot delay; the period is
/// recorded as the code passed it.
#[inline]
pub fn on_timer_create(_kind: crate::node_metadata::TimerKind, _period_ms: u64) {
    #[cfg(feature = "metadata-mode")]
    {
        record(
            crate::node_metadata::EntityKind::Timer,
            _kind,
            "timer",
            Some(_period_ms),
        );
    }
}

/// A guard condition was registered on the executor. One callback slot, same
/// as a timer.
///
/// Recorded as `TimerKind::GuardCondition`: still a `timers[]` row (one slot
/// each, which is what the sizing consumers count) but with
/// `kind: "guard_condition"`, so the census does not read a guard as a timer of
/// period 0.
#[inline]
pub fn on_guard_condition_create() {
    #[cfg(feature = "metadata-mode")]
    {
        record(
            crate::node_metadata::EntityKind::Timer,
            crate::node_metadata::TimerKind::GuardCondition,
            "guard",
            None,
        );
    }
}

/// A node declared a parameter, with the type and default the code passed.
///
/// Called BEFORE the store answers: an adopted launch seed is still a
/// declaration the code makes, and a full store is a boot failure the census
/// should still describe. Attributed to the current node through the cursor,
/// like every other entity.
#[inline]
pub fn on_param_declare(_name: &str, _value: &crate::ParameterValue) {
    #[cfg(feature = "metadata-mode")]
    {
        ensure_scope();
        if !crate::metadata_mode::record_parameter(_name, _value) {
            panic!(
                "nros metadata mode: recorder rejected parameter `{_name}` -- a census \
                 built from this sidecar would say the node declares fewer than it does"
            );
        }
    }
}

/// The node an executor-side entity is attributed to when the code created it
/// before ANY node — legal in C (`nros_timer_init` takes only a support
/// context, and `nros-c`'s own `timer_clock_source.c` run test does exactly
/// that). Recording it under a named scope keeps the slot in the count and
/// makes the census say plainly that no node owns it; failing would turn a
/// legal program into a panic in every native image that links the recorder.
#[cfg(feature = "metadata-mode")]
pub const EXECUTOR_SCOPE: &str = "__executor__";

#[cfg(feature = "metadata-mode")]
fn ensure_scope() {
    if !crate::metadata_mode::has_current_node() {
        on_node_create(EXECUTOR_SCOPE, "/", 0);
    }
}

#[cfg(feature = "metadata-mode")]
fn record(
    kind: crate::node_metadata::EntityKind,
    timer_kind: crate::node_metadata::TimerKind,
    prefix: &str,
    period_ms: Option<u64>,
) {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    ensure_scope();
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let id = alloc::format!("{prefix}{n}");
    let rec = crate::metadata_mode::EntityRecord {
        callback_id: Some(&id),
        period_ms,
        timer_kind,
        ..crate::metadata_mode::EntityRecord::new(kind, &id, "")
    };
    if !crate::metadata_mode::record(rec) {
        panic!(
            "nros metadata mode: recorder rejected `{id}` — an executor sized from \
             this census would be too small"
        );
    }
}
