---
id: 1835
title: "Two threads opening a zenoh session at once could claim the same pool slot and corrupt its config"
status: resolved
type: bug
severity: medium
area: [rmw, zenoh, runtime]
related: [1802, 0348, 0064]
found: 2026-10-12
---

## Measured

While chasing issue 1802, the `zenoh_integration` binary was run under
libtest's own threads, several tests in one process. Two cores came back,
both inside a session open on `g_sessions[0]`:

- a SIGSEGV in `_z_hashmap_insert(map=<g_sessions+96>)`, the slot's config;
- `double free or corruption (fasttop)` from `z_config_drop(<g_sessions+96>)`
  inside `z_open`, reached from `zpico_open` and `Context::with_config`.

## Cause

Opening a session was not reentrant, in three process-global places:

1. `zpico_session_acquire` claims a pool slot with an unlocked test-and-set of
   `g_session_inuse[i]`. Two threads can both see slot 0 free, both claim it,
   and both `memset` it.
2. The config each one builds lives in that one slot, so one thread's
   `z_config_drop` inside `z_open` frees what the other is still inserting.
3. `Context::with_config` stages its locator in `static mut LOC_BUF`
   (issue 0064). A second open overwrites it while the first one is still
   retrying.

nextest runs each test in its own process, so no in-tree lane opens two
sessions at once. A multi-threaded application that opens from two threads
would hit it, and so does a bridge, which has `ZPICO_MAX_SESSIONS > 1`.

## Fix

`Context::new` and `Context::with_config` hold one process-wide open lock,
`OpenGuard` (a `portable_atomic::AtomicBool`, this crate's idiom), across the
slot claim, the locator staging, init and the connect with its retries. That
makes all three one critical section.

A waiter is waiting on someone else's connect, so it sleeps 1 ms between
tries rather than spinning. ThreadX has no free sleep outside the spin path,
so there it spins. With one thread, as on every embedded image, the lock is
never contended.

The other callers of `zpico_session_acquire` (the Zephyr C bring-up in
`zpico_zephyr.c`, `stress-zenoh-zephyr`, the `pool-exhaustion-threadx-linux`
bin) open one session at boot on one thread, so the Rust open path is the
only one that can race.

Test: `concurrent_opens_never_share_a_session_slot` has 4 threads open
together, released by a barrier, for 10 rounds. It asserts that every open
either succeeds or is refused with `Full`, and that no more sessions are held
at once than `ZPICO_MAX_SESSIONS`. A shared slot would exceed that.

- Without the lock: 14 of 20 runs went wrong, 10 SIGSEGV and 4 FAIL.
- With it: 20 of 20 pass.
