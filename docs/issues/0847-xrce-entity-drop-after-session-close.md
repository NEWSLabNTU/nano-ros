---
id: 847
title: "An XRCE publisher outliving `executor.close()` segfaults in its own Drop:
  the entity destructor dereferences the session state that close already freed"
status: open
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
