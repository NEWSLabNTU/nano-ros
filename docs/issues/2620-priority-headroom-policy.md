---
id: 2620
title: "The priority realizer packs allocations at the urgent end of the pool; no headroom policy was ever chosen"
status: open
type: limitation
severity: low
area: [orchestration, platform]
found: 2026-11-15
related: [2619]
---

**Owner:** unassigned. **Home:** RFC-0079 "Open questions" (headroom policy).
Phase-459 is archived.

## What happens

`PriorityPlan::nth_app_priority(k)` (`nros-orchestration-ir/src/priority_plan.rs`)
gives rank `k` the `k`-th most urgent priority of `pool.app`: `hi - k` on a
bigger-is-urgent port, `lo + k` on a smaller-is-urgent one. So allocations PACK
at the urgent end of the pool. That is the implementation's default, not a
decision: RFC-0079 lists the policy as open.

The two choices cost different things:

- **Packed (today).** No room is left above the most urgent sched task for a
  static lease, so a later pin above the derived set has to cross into a
  reserved band.
- **Even spread.** An inserted sched task finds a free level between two others
  without moving them, and the static-lease room goes.

## What closes it

A ruling on the policy, recorded in RFC-0079, then `nth_app_priority`
implementing it with a test per direction.
