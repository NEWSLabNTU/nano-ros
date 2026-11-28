---
id: 2845
title: "Without `alloc`, an owned session is still shared from inside its owner, where the owner's next write disables the pointer"
status: open
type: bug
severity: low
area: [executor, memory]
related: [issue-2675, phase-573]
found: 2026-11-23
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

## Reverted for the release (2026-11-28)

The fix (#4995) landed and was reverted before release nros12 (B51). It grew
one stack frame by 16 B: the `run_bare` closure of
`packages/testing/nros-bench/large-msg-baremetal` on mps2-an385 bare metal,
which then exceeded the `check::max-frame` baseline row (4664 B). Measured in
one profile, from the stack-floor walk's report: 5896 B with the fix, 5880 B
with it reverted.

Where the 16 B came from, bisected: the fix's code commit is the first at
4680 B (its parent measures 4664 B). It grew the `Executor` value by 8 B (the
value's `memcpy` went from 1924 to 1932 bytes), and the `run_bare` closure
holds two copies of an `Executor`, so the frame grew by twice that.

It comes back after the release with those 16 B removed, by one of two
routes, both to be measured: win the 8 B back in the session store's layout,
or stop the closure holding a second `Executor` copy, which would save far
more than 16 B. Until then the issue's original state holds.
