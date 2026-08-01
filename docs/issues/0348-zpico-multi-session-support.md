---
id: 348
title: "zpico supports only one zenoh session per process — multi-domain / multi-session topologies need g_session and 51 process globals moved into a per-session context"
status: resolved
type: limitation
area: core
related: [issue-0347, issue-0096]
resolved_by: phase-328
---

## Resolution (2026-08-01, phase-328)

Option **3** (full handle-passing) shipped. `zpico.c`'s `g_session` and every
per-session `g_*` table moved into a `struct zpico_session` allocated from a
compile-time pool (`ZPICO_MAX_SESSIONS`, default 1); every `zpico_*` entry
point takes a `zpico_session_t*` handle (the diagnostic counters and the ZID
uniquifier stay process-global; the two task-config setters stay process-wide
defaults, applied at open). zenoh-pico closures recover their session by
packing `{session_idx, slot_idx}` into the existing `void* ctx`. The Rust
`Context` owns one pool slot; `ZenohSession` is unchanged above it.

Verified: `two_sessions_deliver_cross_session_through_router`
(`ZPICO_MAX_SESSIONS=2`) — session A's subscriber receives what session B
(opened second) publishes, proving the two sessions are independent. Full
`zenoh_integration` suite 15/15 green at pool=2. Single-session footprint delta
+142 B `.bss` (the `g_session_inuse[1]` flag + struct-aggregation padding; the
21 KB table budget did not multiply). See
[phase-328](../roadmap/phase-328-zpico-multi-session.md). Detail below is the
original finding.
---

## Finding (2026-07-28, split out of issue 0347)

Issue 0347 made a second `ZenohTransport::open()` in one process **fail loudly**
(`ZPICO_ERR_SESSION`) instead of silently memsetting the first session's
registration tables and replacing its session handle. That fixed the silent
corruption — it did not add the capability. This issue is the capability.

`packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c` is single-session **by
construction**, and says so in passing (`zpico.rs:344`: *"zpico is
single-session/global already"*).

## What "multi-session" actually costs

**51 file-scope statics**, of which these are per-session state that would have
to move into a context struct:

| global | what it holds |
| --- | --- |
| `g_session`, `g_config`, `g_session_open` | the session itself |
| `g_publishers[]`, `g_subscribers[]`, `g_liveliness[]`, `g_queryables[]` | every registration table |
| `g_stored_queries[][]`, `g_stored_query_valid[]`, `g_last_reply_seq[]` | queryable reply state |
| `g_pending_gets[]` | outstanding gets |
| `g_spin_sem`/`g_spin_mutex`/`g_spin_cv`, `g_threadx_read_mutex` | the per-platform spin/wake primitives |
| `g_reply_waker` | the wake hook |

The `g_diag_*` counters (~18) are process-wide diagnostics and can stay global.

**The public C API is the harder half.** All ~38 `zpico_*` entry points in
`zpico-sys/c/include/zpico.h` are implicitly bound to the singleton and take no
session handle:

```c
int32_t zpico_init(const char* locator);
int32_t zpico_declare_subscriber(...);
int32_t zpico_publish(...);
```

Giving them a handle is a breaking ABI change across **51 consuming files**
(boards, Rust shim, the C ports for FreeRTOS / NuttX / ThreadX / bare metal).

## Why it is not urgent

Nothing in-tree needs it. Verified across every tracked `system.toml`: the
bridge workspaces pair zenoh with a *different* backend (cyclonedds / XRCE),
which is one zenoh session plus one of something else — unaffected. The
multi-session shape only arises for a **multi-domain zenoh** topology, which no
example, fixture or test uses today.

The executor already has the seam for it — `extra_sessions` +
`CffiRmw::open_with_rmw` — so the limitation is purely below the RMW boundary,
in the zpico C shim.

## Options, cheapest first

1. **Leave it refused.** Current state. Correct and honest; costs nothing.
2. **Context struct, singleton-compatible API.** Move the per-session globals
   into a `zpico_session_t` and keep the existing entry points as thin wrappers
   over a default instance, adding `zpico_*_ex(session, …)` variants for
   multi-session callers. No consumer changes required; new capability is
   opt-in. This is the option to take if (3) is ever wanted.
3. **Full handle-passing API.** Every `zpico_*` takes a session. Cleanest, and a
   breaking change across 51 files.

Embedded targets are the constraint on (2)/(3): the tables are statically sized
(`ZPICO_MAX_SUBSCRIBERS` et al) and duplicating them per session multiplies a
budget that issue 0316's audit already found tight. A per-session context would
need its slot counts to be a per-instance parameter, not a compile-time
constant — which is really an argument for doing (2) alongside the issue-0316
enumeration work rather than on its own.

## Acceptance, if picked up

- Two `ZenohTransport::open()` calls in one process yield two independent
  sessions; registrations made on the first survive the second opening.
- The regression test from 0347
  (`second_session_open_in_one_process_is_refused`) is replaced by one
  asserting cross-session delivery through a router.
- Embedded footprint is measured before and after — per-session tables must not
  silently multiply the static budget on a target that only ever opens one.
