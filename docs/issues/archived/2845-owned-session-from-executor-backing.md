---
id: 2845
title: "Without `alloc`, an owned session is still shared from inside its owner, where the owner's next write disables the pointer"
status: resolved
type: bug
severity: low
area: [executor, memory]
related: [issue-2675, phase-573]
found: 2026-11-23
resolved: 2026-11-27
resolved_in: "fix(executor, issue 2845): an owned session is carved outside its owner, in every build"
---
## What
Phase-573 W3 gave the per-sched-task model a sound shared session under
`alloc`: the owning executor puts its session in its own allocation and hands
that pointer out, so owner and borrowers reach the session as equals. Without
`alloc` the session stays inline in the owner (`SessionStore::Owned`), and
`session_handle` still hands out a pointer into it, because the C++ sched-task
path (`open_with_session_handle_in`) needs no `alloc`. Under Tree Borrows
that pointer is disabled by the owner's next `&mut self` write, which is
what the W3 Miri case `a_session_pointer_survives_its_owner_spinning`
shows under `alloc`.

## Fix (not started)
Carve the owned session from the executor's caller-supplied backing, as the
arena already is (`storage::carve`). The executor keeps a raw pointer to it,
so the session's tag is the backing's own and not a child of the executor,
and its 536 bytes (x86_64) land in `.bss` when the backing is static, where
`mem-report` sees them. That moves `ExecutorSizing::u64_len`, and with it
every derived backing size: the C and C++ sizes headers,
`NROS_EXECUTOR_BACKING_U64S`, and the Zephyr arena pairing. A C executor,
whose session is borrowed from `rclc_support_t`, must not pay for a slot it
never uses. Hence a separate change.

## Fixed (2026-11-27, phase-573 W6)

Implemented per the design in phase-573 W6, which nros 133 reviewed on #4995.
The user ruled on 2026-11-27 to retire `Heap`.

- `SessionStore` is `Carved | Owned | Borrowed`, and `Heap` is gone. A
  `Carved` session is written into a slot derived from a raw pointer: the
  backing's own (`ExecutorSizing::session`, placed by
  `nros-executor-layout`), or the C++ context's `session_slot`, reached by raw
  field projection. It is dropped in place, keyed on the variant. `Owned`
  (inline) is never shared.
- Who carves a slot:
  - Rust's self-allocating opens and the entry's static backing carve one
    (`EXECUTOR_BACKING_DEFAULT_U64S` is `DEFAULT.owning()`).
  - Spawned sched tasks borrow and carve none (`SCHED_TASK_BACKING_SLOT_U64S`).
  - `create_executor_in` carves one when the caller's backing has room, and
    otherwise opens as before, unshared.
  - C carves none, so its `_opaque` and header do not move.
  - C++ keeps the slot in `CppContext` beside `c`, and the C++ context
    overhead adds the probed `OWNED_SESSION_SLOT_SIZE`.
- Tests, all under Miri with Tree Borrows (`just check
  miri-executor-reentry`, now with a no-`alloc` run):
  - `a_carved_session_pointer_survives_its_owner_spinning` passes without
    `alloc`.
  - `a_projected_session_slot_survives_a_whole_context_reborrow` (the
    `CtxRef::deref_mut` shape) passes.
  - The negative control (`control_a_slot_derived_through_a_reference_…`,
    ignored) FAILS as designed: "the conflicting tag … transitioned to Frozen
    due to a reborrow".
- `check-config-fallback-macros`, `check-executor-backing-arena-pairing` and
  `node-std-tests` (every stated board backing at its own width) pass. The
  NuttX buildless snapshots need no change: no image is sized from them since
  issue 1569, and this change adds no macro.

