---
id: 2855
title: "In C, a call from another thread while a callback runs answers NOT_INIT or REENTRANT, which read as the wrong failure"
status: open
type: bug
severity: low
area: [c-api, executor]
related: [phase-573]
found: 2026-11-23
---
## What
Two findings from the phase-573 W3 review, both about a C call made from
thread B while thread A's executor pass runs a callback:

1. **Action client.** While a client's callback runs in A's window, an
   `rclc_action_*` call on that client from B finds the entry running
   elsewhere (`running_elsewhere`, `spin.rs`), which `arena_core_mut` answers
   as `None`, so B gets `RCL_RET_NOT_INIT`. An rclc caller reads that as "not
   registered" and does not retry, though a retry after the callback returns
   would succeed.
2. **The in-callback flag.** `rclc_service_*` / `rclc_client_*` callers build
   `&mut *header` / `&*header` to the executor header and then call
   `refuse_in_dispatch`, unlocked, while the spinning thread writes
   `in_dispatch`. The raw-place read W3 introduced buys nothing at those
   sites, and the read is not synchronised. The flag is also executor-wide,
   so B's call is refused `REENTRANT` although B is in no callback.

## Fix direction
- A distinct answer for "busy in another thread's callback" (or a wait for the
  window to close), never NOT_INIT.
- Read the flag under the executor's lock, and per thread: the lock's thread
  identity already tells the owning pass's thread from a caller.

## Acceptance
A cross-thread test in each case: B's call during A's callback returns a code
that says busy (or succeeds after the window), and B is never told
`REENTRANT` for a callback it is not in.
