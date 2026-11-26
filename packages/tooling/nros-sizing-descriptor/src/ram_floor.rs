//! The RAM an image needs AT LEAST, from the bytes its descriptor states
//! (RFC-0135 D8, ruled 2026-11-26).
//!
//! A descriptor states counts, and a handful of facts in bytes. Turning the
//! counts into bytes would take a cost model of every pool, which would drift
//! from the real link; the ruling is that the pre-build check uses only what
//! is stated in bytes, as a LOWER bound ("needs at least N"), and the exact
//! figure comes from the linked image's size report against the same budget.
//!
//! Only a [`Fact::Stated`] contributes. An absent or refused fact adds
//! nothing, which keeps the sum a floor: it can only under-count.
//!
//! The executor's backing is not here: the descriptor states the counts it is
//! sized from, not its bytes, so it joins the sum only once a descriptor
//! states it.

use crate::{
    SizingDescriptor, fact::Fact, transient_local_publishers, transient_local_retain_bytes,
    transient_local_retain_depth,
};

/// The stated part of an image's RAM, and what it is made of.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RamFloor {
    /// Each stated contribution: what it is, and its bytes. Zero rows are
    /// left out.
    pub parts: Vec<(&'static str, usize)>,
}

impl RamFloor {
    /// The sum of the parts.
    #[must_use]
    pub fn total(&self) -> usize {
        self.parts.iter().map(|(_, b)| b).sum()
    }
}

/// The bytes `desc` states the image's RAM holds:
///
/// - the stacks of the sched tasks its entry spawns
///   (`spawned_sched_task_stack_bytes`);
/// - the heap it is configured with (`[target] heap_budget_bytes`);
/// - the transient-local retention rings: one per such publisher, each
///   `retain depth x slot bytes`.
#[must_use]
pub fn stated_ram_floor(desc: &SizingDescriptor) -> RamFloor {
    let mut parts = Vec::new();
    let mut push = |what, f: Fact<usize>| {
        if let Fact::Stated(b) = f
            && b > 0
        {
            parts.push((what, b));
        }
    };
    push(
        "sched task stacks",
        desc.image.spawned_sched_task_stack_bytes(),
    );
    push("heap", desc.target.heap_budget_bytes());
    if let (Fact::Stated(n), Fact::Stated(slot), Fact::Stated(depth)) = (
        transient_local_publishers(desc),
        transient_local_retain_bytes(desc),
        transient_local_retain_depth(desc),
    ) {
        push(
            "transient-local retention",
            Fact::Stated(n.saturating_mul(slot).saturating_mul(depth.max(1))),
        );
    }
    RamFloor { parts }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{Basis, Status};

    #[test]
    fn only_stated_bytes_count() {
        let mut d = SizingDescriptor::new("talker", Status::Derived, Basis::Contract);
        assert_eq!(stated_ram_floor(&d), RamFloor::default());
        d.image.set_spawned_sched_task_stack_bytes(Some(4096));
        d.target = crate::Target::new(Some(4), Some(8), Some(65536));
        let f = stated_ram_floor(&d);
        assert_eq!(f.parts, vec![("sched task stacks", 4096), ("heap", 65536)]);
        assert_eq!(f.total(), 69632);
    }
}
