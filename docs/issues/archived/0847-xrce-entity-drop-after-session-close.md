---
id: 847
title: "An XRCE publisher outliving `executor.close()` segfaults in its own Drop:
  the entity destructor dereferences the session state that close already freed"
status: resolved
type: bug
area: rmw
related: [issue-0819]
---

## Problem

Every `nros-bench/stress-xrce` run on this host dumps core at exit, on both the
talker and the listener, *after* all their work has completed and been reported.
`PUBLISH_DONE` and `RECV_DONE` both print; the process then dies with SIGSEGV.

```
Stack trace of thread 1912519:
#0  xrce_publisher_destroy
#1  <nros_rmw_cffi::CffiPublisher as core::ops::drop::Drop>::drop
#2  xrce_stress_test::main
```

`SEGV_MAPERR`, reproduced at every payload size tested (64, 3584, 4096, 6000,
8000), so it is not payload-dependent and not a large-message effect.

## Cause

The ordering is a plain use-after-free across the C ABI:

1. `executor.close()` runs `xrce_session_destroy`, which frees the
   `xrce_session_state_t`.
2. The publisher binding is still alive in the caller's scope, so its `Drop`
   runs afterwards, at end of scope.
3. `xrce_publisher_destroy` reads `ps->session_state` and calls
   `uxr_buffer_delete_entity(&st->session, …)` on freed memory.

```c
xrce_publisher_state *ps = (xrce_publisher_state *)publisher->backend_data;
xrce_session_state_t *st = ps->session_state;      /* freed by close() */
uint16_t req = uxr_buffer_delete_entity(&st->session, st->output_reliable,
                                        ps->datawriter_oid);
```

The same shape exists on `xrce_subscription_destroy`, and by inspection on the
service server/client destructors — each holds a raw `session_state` pointer and
dereferences it unconditionally. Fixing only the publisher would leave the class
in place.

Nothing about this is XRCE-specific in principle — it is the general question of
what an entity handle means once its session is gone — so the other backends
should be checked for the same ordering before a fix is chosen. It surfaces here
because `xrce_session_destroy` actually `free()`s, where a backend that leaks or
refcounts would merely be quiet about it.

## Why it stayed hidden

An explicit `close()` followed by entity drops is exactly what the bench
binaries do and exactly what the docs show, so this is not an exotic ordering.
It survived because a segfault *after* the last line of output looks like a
clean run to anything that greps stdout — which is what the test harness does.
`ManagedProcess` captures output and matches patterns; the exit status of a
process that already printed `RECV_DONE` is not what the large-message tests
assert on.

Found while reproducing #0819 (a separate defect, in the receive path, now
fixed) — the segfault was visible in the same runs and is unrelated to it.

## Direction

Not settled. Two shapes, and the choice is a design decision rather than a bug
fix:

* **Session-side**: `xrce_session_destroy` refuses while entities are
  outstanding, or nulls each live entity's back-pointer so the later destructor
  is a no-op. Needs the session to know its entities, which it partly does
  (the slot tables).
* **Binding-side**: `Executor::close` is what enforces the order — either by
  taking ownership such that live entity handles make `close()` unavailable, or
  by tearing entities down before the session. This puts the invariant where the
  lifetimes are actually visible, but only protects Rust callers; a C consumer
  of the same ABI can still write the crashing order.

A test must assert the process EXIT STATUS, not just its output, or the next
regression of this class is invisible in exactly the same way.

## Resolved 2026-09-03 — the memory outlives the pointers, because the pointers cannot be found

Both shapes the section above left open were considered against the code, and
the code decides between them.

**Not the binding side.** `Executor::close` enforcing the order protects Rust
callers only, and this is a C ABI: a C consumer writes the crashing order with
nothing to stop it. The issue said as much; implementing it would have left the
defect reachable.

**Not the back-pointer sweep either, and this is the part that is not obvious.**
Nulling each live entity's back-pointer at close needs the session to ENUMERATE
its entities. It half can: `xrce_session_state` carries slot pools for
subscribers, service servers and service clients — and **none for publishers**,
which is the entity the crash was reported on. That shape therefore needs a
fourth static pool, on the backend whose current campaign (phase-392) is
removing static RAM nobody can price.

**So: a refcount.** `live_entities` + `session_closed`, two fields, no new pools
and no per-entity cost. `xrce_session_destroy` tears down the uxr session and
the transport, marks the state closed, and frees it only when no entity still
points at it; the last entity destructor out frees it instead. Each destructor
checks `xrce_session_is_closed` BEFORE touching `st->session` and skips the
agent-side `DELETE_ENTITY`, which after close has nowhere to go — that check is
the one that stops the use-after-free, and skipping returns OK because this
ordering is supported rather than a caller error.

Applied to all four destructors together (publisher, subscription, service
server, service client), which is the whole class the issue named — attach at
each creator's single SUCCESS point, not where `session_state` is assigned,
because three of the four fail after that assignment and free their own state.

### Why Cyclone does not have this bug, which is what settled the shape

Cyclone stores `dds_entity_t` HANDLES, which the library validates: `dds_delete`
after the participant is gone returns an error instead of faulting. A raw
pointer cannot be validated once freed, so the memory has to outlive the pointer
instead. That is the difference, and it is worth remembering the next time a
backend hands out raw pointers across this ABI.

### Test

`packages/rmw/xrce/nros-rmw-xrce/tests/entity_lifetime.c`, run by
`just check rmw-xrce` (ctest). It asserts the **exit status**, which is what
this issue asked for: the failure mode is a use-after-free inside a destructor,
so on a broken build the binary does not report, it DIES, and ctest reads that.

It needs no XRCE agent. `xrce_session_destroy` could not be called directly —
MEASURED, it faults in `uxr_delete_session -> wait_session_status` without a
live transport, which is a property of the uxr client and nothing to do with
this defect — so the lifetime decision was split into `xrce_session_mark_closed`,
which is the function that actually owns the free and is the one the test drives.

Mutation-checked: restoring the pre-fix unconditional `nros_xrce_free(st)` makes
the test abort; restored, 2/2 pass. An earlier version of the test set the
closed flag by hand instead of calling the real function, and that version
passed under the mutation — it looked like coverage and was not.

### Not covered

The end-to-end assertion on `nros-bench/stress-xrce`'s exit status. The harness
still greps stdout, so a process that segfaults after printing `RECV_DONE` still
reads as a clean run — the property that hid this for as long as it did. That is
a harness gap rather than a backend one and it outlives this issue.
