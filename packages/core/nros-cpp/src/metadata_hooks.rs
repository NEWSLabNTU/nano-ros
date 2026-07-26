//! phase-308 — the two-function gap the recording RMW backend cannot close.
//!
//! Publishers, subscriptions, services and clients reach the RMW session, so
//! `nros-rmw-metadata` records them with no help from this crate. **Timers and
//! guard conditions never touch the RMW** — they register directly on the
//! executor — so a backend cannot observe them at all. That matters more here
//! than anywhere: a timer is precisely the entity the SystemModel also cannot
//! see, and missing them would reproduce the bug the sidecars exist to fix
//! (issue 0257).
//!
//! Plus one more, found while reading the seam: the RMW's `create_publisher`
//! carries no node — by that layer the owning node is already resolved away.
//! So a backend alone yields a sidecar whose entities belong to no node. The
//! `node_create` hook opens each node and makes it current; `configure()`
//! declares one node's entities at a time, so a cursor is exact, not a guess.
//!
//! Three hooks total. Every one is a no-op unless `metadata-mode` is on, so the
//! call sites in the shipping paths are unconditional and cost nothing.
//!
//! This module records; it does not serialize. No JSON, no schema struct, no
//! slot arithmetic — those live once in `nros::node_metadata` (phase-308's
//! layer constraint).

/// A node was created — make it current so subsequent entities attribute to it.
#[inline]
pub(crate) fn on_node_create(_name: &str, _namespace: &str, _domain_id: u32) {
    #[cfg(feature = "metadata-mode")]
    {
        // A refused begin means the recorder is full; every entity after it
        // would be silently dropped, so say so rather than produce a sidecar
        // that under-counts.
        if !nros::metadata_mode::begin_node(_name, _namespace, _domain_id) {
            panic!(
                "nros metadata mode: recorder rejected node `{_name}` — raise the \
                 MetadataRecorder capacity"
            );
        }
    }
}

/// A timer was registered on the executor.
///
/// Timers carry no name at this ABI (they are bound by function identity —
/// `bind_timer<T, &T::method>`), so the recorded id is synthetic. That is fine:
/// the count is what the executor sizing reads, and a C++ timer has no
/// user-visible name to preserve.
#[inline]
pub(crate) fn on_timer_create(_period_ms: u64) {
    #[cfg(feature = "metadata-mode")]
    {
        record(
            nros::node_metadata::EntityKind::Timer,
            "timer",
            Some(_period_ms),
        );
    }
}

/// A guard condition was registered on the executor. One callback slot, same as
/// a timer.
#[inline]
pub(crate) fn on_guard_condition_create() {
    #[cfg(feature = "metadata-mode")]
    {
        // No dedicated EntityKind: a guard condition occupies one callback slot
        // exactly like a timer, and the sizing consumers count slots. Recording
        // it as a timer keeps the COUNT right; if the schema ever needs to tell
        // them apart, add the kind then rather than inventing one here.
        record(nros::node_metadata::EntityKind::Timer, "guard", None);
    }
}

#[cfg(feature = "metadata-mode")]
fn record(kind: nros::node_metadata::EntityKind, prefix: &str, period_ms: Option<u64>) {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let id = std::format!("{prefix}{n}");
    if !nros::metadata_mode::record_entity(kind, &id, "", Some(&id), period_ms) {
        panic!(
            "nros metadata mode: recorder rejected `{id}` — an executor sized from \
             this sidecar would be too small"
        );
    }
}
